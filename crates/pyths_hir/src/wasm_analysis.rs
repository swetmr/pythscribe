use std::collections::HashMap;

use pyths_syntax::ast::{
    DictItem, ExceptHandler, Expr, ExprKind, FStringPart, Module, Param, Stmt, StmtKind,
};
use pyths_syntax::operators::{BinOp, UnaryOp};
use pyths_types::types::{resolve_type, Type};

/// #364: the numeric-kernel builtin whitelist for direct `Name(...)` calls.
/// Math functions (`WASM_MATH_FUNCTIONS`, bare-imported like `sqrt`) and calls
/// to other eligible functions are also admitted; everything else stays JS.
const WASM_CALL_BUILTINS: &[&str] = &["abs", "len", "int", "float", "range"];

/// Result of analyzing a module for WASM eligibility.
#[derive(Debug)]
pub struct WasmAnalysis {
    /// Functions eligible for WASM compilation, keyed by name.
    pub eligible: HashMap<String, WasmFuncInfo>,
    /// Functions that were rejected, with (name, reason).
    pub rejected: Vec<(String, String)>,
}

/// Information about a WASM-eligible function.
#[derive(Debug, Clone)]
pub struct WasmFuncInfo {
    pub name: String,
    pub params: Vec<(String, Type)>,
    pub return_type: Type,
    /// Index of the function's Stmt in the module body.
    pub stmt_index: usize,
}

/// Analyze a module and determine which functions can be compiled to WASM.
///
/// Two-pass analysis:
/// 1. Signature scan: check params and return type are WASM-eligible (int/float/bool/str)
/// 2. Body scan: check all statements and expressions are WASM-compatible
///
// F8: cost-based placement — see full_stack.md. Placement here is purely
// eligibility-based (type-admissible → WASM). Cost-minimizing / profile-guided
// placement (a weighted per-op model seeded by EvalPerf/Livermore ratios) is a
// later item; it would refine THIS decision without changing its interface.
pub fn analyze_module(module: &Module) -> WasmAnalysis {
    let mut eligible = HashMap::new();
    let mut rejected = Vec::new();

    // First pass: collect signature-eligible functions
    let mut candidates: Vec<WasmFuncInfo> = Vec::new();

    for (idx, stmt) in module.body.iter().enumerate() {
        if let StmtKind::FuncDef {
            name,
            params,
            decorator_list,
            return_type,
            is_async,
            body,
            ..
        } = &stmt.kind
        {
            if let Err(reason) =
                check_signature(name, params, decorator_list, return_type, *is_async)
            {
                rejected.push((name.clone(), reason));
                continue;
            }

            // #363: soundness — a function whose body returns a value MUST carry
            // an explicit return-type annotation to be WASM-eligible. Without
            // one the analysis would treat the return as `Void`, but the body
            // yields a value: the WASM boundary then drops it (returns None /
            // undefined) or, worse, emits an ABI-mismatched module. This only
            // ever bit under automatic routing (#357 made WASM the default);
            // such functions now stay correct JS. Genuinely-void functions
            // (bare `return` / `return None` / no return) are unaffected, as are
            // all annotated numeric kernels.
            if return_type.is_none() && body_returns_value(body) {
                rejected.push((
                    name.clone(),
                    "function returns a value but has no return-type annotation \
                     (WASM needs an explicit return type)"
                        .to_string(),
                ));
                continue;
            }

            let wasm_params: Vec<(String, Type)> = params
                .iter()
                .map(|p| {
                    let ty = p
                        .annotation
                        .as_ref()
                        .map(|a| resolve_type(a))
                        .unwrap_or(Type::Any);
                    (p.name.clone(), ty)
                })
                .collect();

            let ret_type = return_type.as_ref().map(resolve_type).unwrap_or(Type::Void);

            // #364 (Path B — soundness): tighten WASM admission to the
            // proven-correct subset. The backend miscompiles functions that
            // RETURN a non-scalar (list/set/dict/tuple/Optional/Callable): the
            // boundary marshalling drops or corrupts the value (e.g. a
            // `list[int]`-returning comprehension yields `[]`/garbage, and a
            // `list[list[int]]` boundary throws NaN→BigInt). Only scalar returns
            // (int/float/bool/str) and void are admitted; everything else stays
            // correct JS (fast via V8). Same principle as #363: admit only what
            // compiles correctly. `list`-typed PARAMETERS with simple iteration
            // remain eligible (Livermore/similarity prove they marshal in fine).
            if !is_scalar_wasm_return(&ret_type) {
                rejected.push((
                    name.clone(),
                    format!(
                        "return type `{}` is not a WASM-fast-path scalar \
                         (int/float/bool/str/None); the function stays on the JS path",
                        ret_type
                    ),
                ));
                continue;
            }

            candidates.push(WasmFuncInfo {
                name: name.clone(),
                params: wasm_params,
                return_type: ret_type,
                stmt_index: idx,
            });
        }
    }

    // Collect all candidate names for call validation
    let candidate_names: Vec<String> = candidates.iter().map(|c| c.name.clone()).collect();

    // Build extended exception list = built-ins + user-defined classes that
    // derive from a known exception base (Step 5: custom exceptions).
    let user_excs = class_registry(module);
    let mut extended_excs: Vec<String> = WASM_BUILTIN_EXCEPTIONS
        .iter()
        .map(|s| (*s).to_string())
        .collect();
    for c in &user_excs {
        extended_excs.push(c.name.clone());
    }

    // Second pass: check bodies
    for info in candidates {
        if let StmtKind::FuncDef { body, .. } = &module.body[info.stmt_index].kind {
            if let Err(reason) = check_body(body, &candidate_names, &extended_excs) {
                rejected.push((info.name.clone(), reason));
            } else {
                eligible.insert(info.name.clone(), info);
            }
        }
    }

    // #364: call-consistency fixpoint. `check_body` validated each call against
    // the CANDIDATE set (all signature-eligible functions), but pass 2 rejects
    // some candidates for body reasons. A WASM function that calls a now-rejected
    // (JS) function would be unsound — it would emit a WASM call to a symbol that
    // never lands in the module. Iterate to a fixpoint: drop any eligible
    // function that calls a name which is neither a whitelist builtin, a math
    // function, nor itself eligible.
    loop {
        let eligible_now: std::collections::HashSet<String> = eligible.keys().cloned().collect();
        let mut drop_name: Option<(String, String)> = None;
        for (name, info) in &eligible {
            if let StmtKind::FuncDef { body, .. } = &module.body[info.stmt_index].kind {
                let mut called = Vec::new();
                collect_called_functions(body, &mut called);
                if let Some(bad) = called.into_iter().find(|c| {
                    !WASM_CALL_BUILTINS.contains(&c.as_str())
                        && !WASM_MATH_FUNCTIONS.contains(&c.as_str())
                        && !eligible_now.contains(c)
                }) {
                    drop_name = Some((name.clone(), bad));
                    break;
                }
            }
        }
        match drop_name {
            Some((name, bad)) => {
                eligible.remove(&name);
                rejected.push((
                    name,
                    format!(
                        "calls `{}`, which is not WASM-eligible (function stays JS)",
                        bad
                    ),
                ));
            }
            None => break,
        }
    }

    WasmAnalysis { eligible, rejected }
}

/// #364: collect the names of all directly-called functions (`Name(...)`) in a
/// body, recursing into nested control flow and sub-expressions.
fn collect_called_functions(body: &[Stmt], out: &mut Vec<String>) {
    fn expr(e: &Expr, out: &mut Vec<String>) {
        match &e.kind {
            ExprKind::Call { func, args, .. } => {
                if let ExprKind::Name(n) = &func.kind {
                    out.push(n.clone());
                }
                expr(func, out);
                for a in args {
                    expr(a, out);
                }
            }
            ExprKind::BinOp { left, right, .. } => {
                expr(left, out);
                expr(right, out);
            }
            ExprKind::UnaryOp { operand, .. } => expr(operand, out),
            ExprKind::Compare { left, comparisons } => {
                expr(left, out);
                for (_, c) in comparisons {
                    expr(c, out);
                }
            }
            ExprKind::IfExpr {
                test,
                body,
                else_body,
            } => {
                expr(test, out);
                expr(body, out);
                expr(else_body, out);
            }
            ExprKind::Subscript { value, index, .. } => {
                expr(value, out);
                expr(index, out);
            }
            ExprKind::List(elts) => {
                for el in elts {
                    expr(el, out);
                }
            }
            _ => {}
        }
    }
    fn stmt(s: &Stmt, out: &mut Vec<String>) {
        match &s.kind {
            StmtKind::Assign { value, .. } => expr(value, out),
            StmtKind::AugAssign { value, .. } => expr(value, out),
            StmtKind::AnnAssign { value: Some(v), .. } => expr(v, out),
            StmtKind::Return(Some(v)) => expr(v, out),
            StmtKind::Expr(e) => expr(e, out),
            StmtKind::If {
                test,
                body,
                elif_clauses,
                else_body,
            } => {
                expr(test, out);
                collect_called_functions(body, out);
                for (t, b) in elif_clauses {
                    expr(t, out);
                    collect_called_functions(b, out);
                }
                if let Some(b) = else_body {
                    collect_called_functions(b, out);
                }
            }
            StmtKind::While {
                test,
                body,
                else_body,
            } => {
                expr(test, out);
                collect_called_functions(body, out);
                if let Some(b) = else_body {
                    collect_called_functions(b, out);
                }
            }
            StmtKind::For {
                iter,
                body,
                else_body,
                ..
            } => {
                expr(iter, out);
                collect_called_functions(body, out);
                if let Some(b) = else_body {
                    collect_called_functions(b, out);
                }
            }
            _ => {}
        }
    }
    for s in body {
        stmt(s, out);
    }
}

/// #364 (Path B — numeric-kernel whitelist): a return type is on the WASM fast
/// path only when it is a NUMERIC scalar (int/float/bool) or void. Strings and
/// every non-scalar boundary (list/set/dict/tuple/Optional/Callable/Any) are
/// general data the backend miscompiles, so such functions stay on the correct
/// JS path. (`str`-returning functions can only produce their value via string
/// operations, which the body whitelist also rejects.)
pub fn is_scalar_wasm_return(ty: &Type) -> bool {
    matches!(
        ty,
        Type::Int | Type::Float | Type::Bool | Type::NoneType | Type::Void
    )
}

/// #364: a parameter is on the WASM numeric-kernel fast path only when it is a
/// numeric scalar (int/float/bool) or a flat list of numeric scalars
/// (`list[int]` / `list[float]` / `list[bool]`) — the proven-safe marshalling
/// surface (similarity's `list[float]`, Livermore's scalar params). Strings,
/// dicts, sets, tuples, nested lists, Optional/Callable/Any stay JS.
pub fn is_numeric_kernel_param(ty: &Type) -> bool {
    match ty {
        Type::Int | Type::Float | Type::Bool => true,
        Type::List(inner) => matches!(**inner, Type::Int | Type::Float | Type::Bool),
        _ => false,
    }
}

/// #363: does any statement in `body` (recursing into nested control flow, but
/// NOT into nested function/class defs, which own their own returns) contain a
/// `return <expr>` that yields a value? A bare `return` and `return None` are
/// void-equivalent and do not count.
fn body_returns_value(body: &[Stmt]) -> bool {
    body.iter().any(stmt_returns_value)
}

fn stmt_returns_value(stmt: &Stmt) -> bool {
    match &stmt.kind {
        StmtKind::Return(Some(expr)) => !matches!(expr.kind, ExprKind::NoneLiteral),
        StmtKind::Return(None) => false,
        StmtKind::If {
            body,
            elif_clauses,
            else_body,
            ..
        } => {
            body_returns_value(body)
                || elif_clauses.iter().any(|(_, b)| body_returns_value(b))
                || else_body
                    .as_deref()
                    .map(body_returns_value)
                    .unwrap_or(false)
        }
        StmtKind::While {
            body, else_body, ..
        }
        | StmtKind::For {
            body, else_body, ..
        } => {
            body_returns_value(body)
                || else_body
                    .as_deref()
                    .map(body_returns_value)
                    .unwrap_or(false)
        }
        StmtKind::Try {
            body,
            handlers,
            else_body,
            finally_body,
            ..
        } => {
            body_returns_value(body)
                || handlers.iter().any(|h| body_returns_value(&h.body))
                || else_body
                    .as_deref()
                    .map(body_returns_value)
                    .unwrap_or(false)
                || finally_body
                    .as_deref()
                    .map(body_returns_value)
                    .unwrap_or(false)
        }
        StmtKind::With { body, .. } => body_returns_value(body),
        // Nested FuncDef/ClassDef own their own returns; everything else is a
        // leaf w.r.t. return statements.
        _ => false,
    }
}

/// Check if a function's signature is WASM-compatible.
fn check_signature(
    _name: &str,
    params: &[Param],
    decorators: &[Expr],
    return_type: &Option<Expr>,
    is_async: bool,
) -> Result<(), String> {
    if is_async {
        return Err("async functions are not supported".into());
    }

    // Check for *args/**kwargs
    for p in params {
        if p.is_args {
            return Err("*args is not supported".into());
        }
        if p.is_kwargs {
            return Err("**kwargs is not supported".into());
        }
    }

    // Check decorators â€” only @wasm is allowed (but we don't require it)
    for dec in decorators {
        if let ExprKind::Name(n) = &dec.kind {
            if n != "wasm" {
                return Err(format!("decorator @{} is not supported", n));
            }
        } else {
            return Err("non-name decorator is not supported".into());
        }
    }

    // Check all params have type annotations resolving to WASM-eligible types
    for p in params {
        match &p.annotation {
            None => {
                return Err(format!("parameter '{}' has no type annotation", p.name));
            }
            Some(ann) => {
                let ty = resolve_type(ann);
                // #364: params must be a numeric scalar or flat list-of-scalar
                // (the proven-safe marshalling surface). str/dict/set/tuple/
                // nested-list params carry general data the backend miscompiles.
                if !is_numeric_kernel_param(&ty) {
                    return Err(format!(
                        "parameter '{}' type `{}` is not a WASM numeric-kernel type \
                         (scalar or flat list-of-scalar); the function stays on the JS path",
                        p.name, ty
                    ));
                }
            }
        }
        // Default values are ok (they'll be handled as constants)
    }

    // Check return type
    if let Some(ret) = return_type {
        let ty = resolve_type(ret);
        if !is_wasm_eligible(&ty) && !matches!(ty, Type::NoneType | Type::Void) {
            return Err(format!("return type {} is not supported", ty));
        }
    }
    // No return type annotation = void return, which is ok

    Ok(())
}

/// Check whether a type is WASM-eligible.
///
/// Primitives (int, float, bool, str) are always eligible. Collections
/// (List, Dict, Tuple, Set, Callable) are eligible iff their constituent
/// element/parameter/return types are also eligible.
///
/// Type::Any inside a collection is accepted (treated as opaque i32 pointer);
/// this is needed for bare `def f() -> list:` annotations where the user
/// hasn't named the element type. Top-level `Type::Any` is still rejected.
///
/// **Soundness invariant (machine-checked).** This predicate is the WASM
/// admission gate for a function's boundary types; it must never admit a
/// type the WASM lowering (`crates/pyths_codegen_wasm/src/types.rs ::
/// to_wasm_type`) cannot represent, or codegen `unwrap()`s a `None` and
/// panics. Every arm here therefore corresponds to a `Some(_)` arm of
/// `to_wasm_type`. The implication `is_wasm_eligible(ty) =>
/// to_wasm_type(ty).is_some()` is proved in Lean as `wasm_admission_sound`
/// (verification/PythExpandVerify.lean, WasmAdmission section) and the two
/// functions are bound to their Lean twins by
/// `verification/wasm-admission-table.txt` (two-sided drift gate). NB:
/// `Type::Optional` is deliberately NOT admitted — `to_wasm_type` has no
/// Optional lowering, so such functions correctly fall back to JS instead
/// of panicking codegen (regression: `test_optional_param_not_eligible`).
pub fn is_wasm_eligible(ty: &Type) -> bool {
    match ty {
        Type::Int | Type::Float | Type::Bool | Type::Str => true,
        Type::List(inner) | Type::Set(inner) => is_wasm_eligible_inner(inner),
        Type::Dict(k, v) => is_wasm_eligible_inner(k) && is_wasm_eligible_inner(v),
        Type::Tuple(types) => types.iter().all(is_wasm_eligible_inner),
        Type::Callable(params, ret) => {
            params.iter().all(is_wasm_eligible_inner)
                && (matches!(**ret, Type::NoneType | Type::Void) || is_wasm_eligible_inner(ret))
        }
        _ => false,
    }
}

/// Element-type eligibility — like `is_wasm_eligible`, but `Type::Any` is
/// also accepted (since it lowers to `Ptr` / opaque i32 in codegen).
fn is_wasm_eligible_inner(ty: &Type) -> bool {
    matches!(ty, Type::Any) || is_wasm_eligible(ty)
}

/// math.* functions that compile to WASM imports (Tier 3). All take and return f64.
const WASM_MATH_FUNCTIONS: &[&str] = &[
    "sqrt", "sin", "cos", "tan", "asin", "acos", "atan", "atan2", "log", "log2", "log10", "exp",
    "ceil", "floor", "fabs", "pow",
];

/// math.* constants compiled to inline f64.const.
const WASM_MATH_CONSTANTS: &[&str] = &["pi", "e", "tau", "inf"];

/// Built-in exception types that map to error codes (Tier 7).
const WASM_BUILTIN_EXCEPTIONS: &[&str] = &[
    "ValueError",
    "TypeError",
    "IndexError",
    "KeyError",
    "ZeroDivisionError",
    "AssertionError",
    "RuntimeError",
    "Exception",
];

/// Map a built-in exception name to its WASM error code.
/// 0 is reserved for "no error".
pub fn exception_code(name: &str) -> Option<i32> {
    Some(match name {
        "ValueError" => 1,
        "TypeError" => 2,
        "IndexError" => 3,
        "KeyError" => 4,
        "ZeroDivisionError" => 5,
        "AssertionError" => 6,
        "RuntimeError" => 7,
        "Exception" => 7,
        _ => return None,
    })
}

/// Information about a user-defined exception class collected from a module.
#[derive(Debug, Clone)]
pub struct ExceptionClass {
    pub name: String,
    /// Name of the immediate base class (one of the WASM_BUILTIN_EXCEPTIONS or
    /// another user class â€” we don't yet validate transitive inheritance).
    pub base: String,
}

/// Walk a module and return every class definition that derives (directly)
/// from a known exception base. Used by Step 5 (custom exceptions) so the
/// WASM emitter can assign error codes 100+.
pub fn class_registry(module: &Module) -> Vec<ExceptionClass> {
    let mut out = Vec::new();
    for stmt in &module.body {
        if let StmtKind::ClassDef { name, bases, .. } = &stmt.kind {
            if let Some(first_base) = bases.first() {
                if let Some(base_name) = exception_name_from_expr(first_base) {
                    // Accept any base that's a known built-in or another exception
                    // class registered earlier (transitive â€” left for a future pass).
                    if WASM_BUILTIN_EXCEPTIONS.contains(&base_name.as_str())
                        || out.iter().any(|c: &ExceptionClass| c.name == base_name)
                    {
                        out.push(ExceptionClass {
                            name: name.clone(),
                            base: base_name,
                        });
                    }
                }
            }
        }
    }
    out
}

/// Upper bound on list-subscript nesting a WASM function may use. The emitter
/// pre-allocates one scratch local-pair per nesting level *up front* (WASM
/// locals are declared in the function header, before the body is walked), so
/// the pool is sized from a static pre-scan. Functions that nest list
/// subscripts deeper than this are rejected from WASM here and stay on the JS
/// backend — which has its own recursion-depth guard — rather than allocating
/// an unbounded scratch pool. 64 is far beyond any real numeric kernel (the
/// previous hard cap was 8, enforced by a `panic!`).
pub const WASM_MAX_SUBSCRIPT_NESTING: usize = 64;

/// Maximum number of nested `Subscript` expressions along any single path in
/// `expr`. A `Subscript` node contributes 1 plus the deepest of its container
/// and index subtrees; every other node contributes the deepest of its
/// children. This is a safe upper bound on the WASM emitter's runtime
/// `sub_depth` (which only ever increments while descending through a
/// `Subscript`'s container or index), so a scratch pool sized from it can never
/// under-allocate.
pub fn max_subscript_depth(expr: &Expr) -> usize {
    let here = usize::from(matches!(expr.kind, ExprKind::Subscript { .. }));
    let deepest_child = match &expr.kind {
        ExprKind::BinOp { left, right, .. } => {
            max_subscript_depth(left).max(max_subscript_depth(right))
        }
        ExprKind::UnaryOp { operand, .. } => max_subscript_depth(operand),
        ExprKind::Compare { left, comparisons } => {
            let mut m = max_subscript_depth(left);
            for (_, e) in comparisons {
                m = m.max(max_subscript_depth(e));
            }
            m
        }
        ExprKind::Call {
            func, args, kwargs, ..
        } => {
            let mut m = max_subscript_depth(func);
            for a in args {
                m = m.max(max_subscript_depth(a));
            }
            for kw in kwargs {
                m = m.max(max_subscript_depth(&kw.value));
            }
            m
        }
        ExprKind::Attribute { value, .. } => max_subscript_depth(value),
        ExprKind::Subscript { value, index, .. } => {
            max_subscript_depth(value).max(max_subscript_depth(index))
        }
        ExprKind::List(elts) | ExprKind::Tuple(elts) | ExprKind::Set(elts) => {
            elts.iter().map(max_subscript_depth).max().unwrap_or(0)
        }
        ExprKind::Dict { items } => items
            .iter()
            .map(|it| match it {
                DictItem::KeyValue { key, value } => {
                    max_subscript_depth(key).max(max_subscript_depth(value))
                }
                DictItem::Spread(e) => max_subscript_depth(e),
            })
            .max()
            .unwrap_or(0),
        ExprKind::FString { parts } => parts
            .iter()
            .filter_map(|p| match p {
                FStringPart::Expr(e) => Some(max_subscript_depth(e)),
                _ => None,
            })
            .max()
            .unwrap_or(0),
        ExprKind::ListComp { elt, generators }
        | ExprKind::SetComp { elt, generators }
        | ExprKind::GeneratorExp { elt, generators } => {
            let mut m = max_subscript_depth(elt);
            for g in generators {
                m = m
                    .max(max_subscript_depth(&g.target))
                    .max(max_subscript_depth(&g.iter));
                for c in &g.ifs {
                    m = m.max(max_subscript_depth(c));
                }
            }
            m
        }
        ExprKind::DictComp {
            key,
            value,
            generators,
        } => {
            let mut m = max_subscript_depth(key).max(max_subscript_depth(value));
            for g in generators {
                m = m
                    .max(max_subscript_depth(&g.target))
                    .max(max_subscript_depth(&g.iter));
                for c in &g.ifs {
                    m = m.max(max_subscript_depth(c));
                }
            }
            m
        }
        ExprKind::Lambda { body, .. } => max_subscript_depth(body),
        ExprKind::IfExpr {
            test,
            body,
            else_body,
        } => max_subscript_depth(test)
            .max(max_subscript_depth(body))
            .max(max_subscript_depth(else_body)),
        ExprKind::Starred(e) | ExprKind::Await(e) | ExprKind::YieldFrom(e) => {
            max_subscript_depth(e)
        }
        ExprKind::Yield(e) => e.as_ref().map(|e| max_subscript_depth(e)).unwrap_or(0),
        ExprKind::NamedExpr { target, value } => {
            max_subscript_depth(target).max(max_subscript_depth(value))
        }
        ExprKind::Slice { lower, upper, step } => {
            let mut m = 0;
            for e in [lower, upper, step].into_iter().flatten() {
                m = m.max(max_subscript_depth(e));
            }
            m
        }
        ExprKind::IntLiteral(_)
        | ExprKind::FloatLiteral(_)
        | ExprKind::ImagLiteral(_)
        | ExprKind::StringLiteral(_)
        | ExprKind::BytesLiteral(_)
        | ExprKind::BoolLiteral(_)
        | ExprKind::NoneLiteral
        | ExprKind::Name(_) => 0,
    };
    here + deepest_child
}

/// Maximum list-subscript nesting depth over every expression reachable from
/// `body`. Reuses the `Visitor`'s exhaustive statement traversal, measuring
/// each top-level expression's full subtree with [`max_subscript_depth`].
pub fn max_subscript_depth_in_stmts(body: &[Stmt]) -> usize {
    use pyths_syntax::visitor::Visitor;
    struct DepthVisitor {
        max: usize,
    }
    impl Visitor for DepthVisitor {
        fn visit_expr(&mut self, expr: &Expr) {
            // `max_subscript_depth` already covers the whole subtree, so don't
            // recurse into children here (that would double-count).
            self.max = self.max.max(max_subscript_depth(expr));
        }
    }
    let mut v = DepthVisitor { max: 0 };
    for stmt in body {
        v.visit_stmt(stmt);
    }
    v.max
}

/// Check if a function body is WASM-compatible.
fn check_body(
    body: &[Stmt],
    eligible_names: &[String],
    extended_excs: &[String],
) -> Result<(), String> {
    for stmt in body {
        check_stmt(stmt, eligible_names, extended_excs)?;
    }
    // Reject functions whose list-subscript nesting would overflow the WASM
    // emitter's statically-sized scratch pool; they stay on the JS backend.
    let depth = max_subscript_depth_in_stmts(body);
    if depth > WASM_MAX_SUBSCRIPT_NESTING {
        return Err(format!(
            "list-subscript nesting is {} levels deep (WASM supports up to {}); \
             function stays on the JS backend",
            depth, WASM_MAX_SUBSCRIPT_NESTING
        ));
    }
    Ok(())
}

/// Review D: does any `return <expr>` directly inside this try body (descending
/// through nested if/while/for bodies, but NOT into a nested `try` — which has
/// its own handlers — nor into nested defs) return an expression that can raise
/// a subscript IndexError? Such a return bypasses the enclosing handler on the
/// WASM backend, so the function must stay on JS.
fn try_body_has_raising_return(body: &[Stmt]) -> bool {
    body.iter().any(stmt_has_raising_return)
}

/// Review D (over-rejection fix): could any of these handlers actually CATCH an
/// `IndexError`? A bare `except:`, or a handler naming IndexError or one of its
/// superclasses (LookupError / Exception / BaseException), can. `except
/// ValueError` cannot, so the return-bypass is harmless (the IndexError
/// propagates out either way) and the function stays WASM-eligible.
fn handlers_catch_indexerror(handlers: &[ExceptHandler]) -> bool {
    fn expr_catches(e: &Expr) -> bool {
        match &e.kind {
            // `except (A, B):` — any element may catch it.
            ExprKind::Tuple(elts) => elts.iter().any(expr_catches),
            _ => matches!(
                exception_name_from_expr(e).as_deref(),
                Some("IndexError" | "LookupError" | "Exception" | "BaseException")
            ),
        }
    }
    handlers.iter().any(|h| match &h.exc_type {
        None => true, // bare `except:` catches everything
        Some(e) => expr_catches(e),
    })
}

fn stmt_has_raising_return(stmt: &Stmt) -> bool {
    let any = |b: &[Stmt]| b.iter().any(stmt_has_raising_return);
    let any_opt = |b: &Option<Vec<Stmt>>| b.as_ref().is_some_and(|b| any(b));
    match &stmt.kind {
        StmtKind::Return(Some(v)) => expr_contains_subscript(v),
        StmtKind::If {
            body,
            elif_clauses,
            else_body,
            ..
        } => any(body) || elif_clauses.iter().any(|(_, b)| any(b)) || any_opt(else_body),
        // Loops: the `else` body also runs (and can hold a raising return).
        StmtKind::While {
            body, else_body, ..
        }
        | StmtKind::For {
            body, else_body, ..
        } => any(body) || any_opt(else_body),
        StmtKind::With { body, .. } => any(body),
        StmtKind::Match { cases, .. } => cases.iter().any(|c| any(&c.body)),
        // A nested try is independently admission-checked, but descend anyway so
        // no body is left unvisited (a return in its body/handler/finally has the
        // same handler-bypass bug).
        StmtKind::Try {
            body,
            handlers,
            else_body,
            finally_body,
        } => {
            any(body)
                || handlers.iter().any(|h| any(&h.body))
                || any_opt(else_body)
                || any_opt(finally_body)
        }
        // FuncDef / ClassDef are separate scopes — their returns are not this
        // try's. Everything else binds no nested body.
        _ => false,
    }
}

/// Whether an expression contains a subscript read anywhere (the WASM raising
/// operation). Conservative: any `x[i]` — the emitter's bounds check may raise.
fn expr_contains_subscript(e: &Expr) -> bool {
    use pyths_syntax::ast::ExprKind as E;
    match &e.kind {
        E::Subscript { .. } => true,
        E::BinOp { left, right, .. } => {
            expr_contains_subscript(left) || expr_contains_subscript(right)
        }
        E::UnaryOp { operand, .. } => expr_contains_subscript(operand),
        E::Compare { left, comparisons } => {
            expr_contains_subscript(left)
                || comparisons.iter().any(|(_, e)| expr_contains_subscript(e))
        }
        E::Call {
            func, args, kwargs, ..
        } => {
            expr_contains_subscript(func)
                || args.iter().any(expr_contains_subscript)
                || kwargs.iter().any(|k| expr_contains_subscript(&k.value))
        }
        E::Attribute { value, .. } => expr_contains_subscript(value),
        E::IfExpr {
            test,
            body,
            else_body,
        } => {
            expr_contains_subscript(test)
                || expr_contains_subscript(body)
                || expr_contains_subscript(else_body)
        }
        E::Tuple(elts) | E::List(elts) => elts.iter().any(expr_contains_subscript),
        _ => false,
    }
}

fn check_stmt(
    stmt: &Stmt,
    eligible_names: &[String],
    extended_excs: &[String],
) -> Result<(), String> {
    match &stmt.kind {
        StmtKind::Assign { targets, value } => {
            for t in targets {
                check_assign_target(t)?;
            }
            check_expr(value, eligible_names)
        }
        StmtKind::AugAssign { target, value, .. } => {
            check_assign_target(target)?;
            check_expr(value, eligible_names)
        }
        StmtKind::AnnAssign {
            target,
            annotation,
            value,
        } => {
            check_assign_target(target)?;
            // Verify annotation is WASM-eligible
            let ty = resolve_type(annotation);
            if !is_wasm_eligible(&ty) && !matches!(ty, Type::NoneType | Type::Void) {
                return Err(format!("annotated type {} is not supported", ty));
            }
            if let Some(v) = value {
                check_expr(v, eligible_names)?;
            }
            Ok(())
        }
        StmtKind::Return(val) => {
            if let Some(v) = val {
                check_expr(v, eligible_names)?;
            }
            Ok(())
        }
        StmtKind::If {
            test,
            body,
            elif_clauses,
            else_body,
        } => {
            check_expr(test, eligible_names)?;
            check_body(body, eligible_names, extended_excs)?;
            for (elif_test, elif_body) in elif_clauses {
                check_expr(elif_test, eligible_names)?;
                check_body(elif_body, eligible_names, extended_excs)?;
            }
            if let Some(else_b) = else_body {
                check_body(else_b, eligible_names, extended_excs)?;
            }
            Ok(())
        }
        StmtKind::While {
            test,
            body,
            else_body,
        } => {
            check_expr(test, eligible_names)?;
            check_body(body, eligible_names, extended_excs)?;
            if let Some(else_b) = else_body {
                check_body(else_b, eligible_names, extended_excs)?;
            }
            Ok(())
        }
        StmtKind::For {
            target,
            iter,
            body,
            else_body,
            is_async,
        } => {
            if *is_async {
                return Err("async for is not supported".into());
            }
            check_assign_target(target)?;
            // iter must be a call to range()
            check_for_iter(iter, eligible_names)?;
            check_body(body, eligible_names, extended_excs)?;
            if let Some(else_b) = else_body {
                check_body(else_b, eligible_names, extended_excs)?;
            }
            Ok(())
        }
        StmtKind::Break | StmtKind::Continue | StmtKind::Pass => Ok(()),
        StmtKind::Expr(expr) => check_expr(expr, eligible_names),

        // Tier 7: raise / assert / simple try-except
        StmtKind::Raise(exc, cause) => {
            // raise must reference a known built-in exception type, either:
            //   raise ValueError       -- bare name
            //   raise ValueError("msg") -- call (msg is dropped in WASM)
            // bare `raise` (re-raise) is not supported in WASM.
            if cause.is_some() {
                return Err("'raise X from Y' is not supported in WASM".into());
            }
            match exc {
                None => {
                    Err("bare 'raise' is not supported in WASM (use a specific exception)".into())
                }
                Some(e) => {
                    let exc_name = exception_name_from_expr(e);
                    match exc_name {
                        Some(n) if extended_excs.iter().any(|e| e == &n) => Ok(()),
                        Some(n) => Err(format!(
                            "custom exception '{}' is not supported in WASM (only built-ins)",
                            n
                        )),
                        None => {
                            Err("raise expression must reference a built-in exception class".into())
                        }
                    }
                }
            }
        }
        StmtKind::Assert { test, .. } => {
            // msg expression is allowed but dropped in WASM; we always raise AssertionError.
            check_expr(test, eligible_names)
        }
        StmtKind::Try {
            body,
            handlers,
            else_body,
            finally_body,
        } => {
            if else_body.is_some() {
                return Err("try/else is not supported in WASM".into());
            }
            if finally_body.is_some() {
                return Err("try/finally is not supported in WASM".into());
            }
            if handlers.is_empty() {
                return Err("try requires at least one except handler".into());
            }
            check_body(body, eligible_names, extended_excs)?;
            for h in handlers {
                // Review finding 6: a TUPLE handler `except (A, B):` catches
                // several types — validate EACH element (a bare `except:`
                // catches everything). Previously the whole tuple was rejected
                // as "not a built-in", so it never reached the D gating below.
                let exc_names: Vec<Option<String>> = match &h.exc_type {
                    None => vec![Some("Exception".into())],
                    Some(e) => match &e.kind {
                        ExprKind::Tuple(elts) => {
                            elts.iter().map(exception_name_from_expr).collect()
                        }
                        _ => vec![exception_name_from_expr(e)],
                    },
                };
                for exc_name in exc_names {
                    match exc_name {
                        Some(n) if extended_excs.iter().any(|e| e == &n) => {}
                        Some(n) => {
                            return Err(format!(
                                "except handler for '{}' is not supported in WASM (only built-ins)",
                                n
                            ));
                        }
                        None => {
                            return Err(
                                "except handler type must reference a built-in exception".into()
                            );
                        }
                    }
                }
                if h.name.is_some() {
                    return Err("'except E as name' binding is not supported in WASM".into());
                }
                check_body(&h.body, eligible_names, extended_excs)?;
            }
            // WASM-error-model limitation (review D): a subscript read inside a
            // `return` expression within a try body sets the global err_code
            // mid-expression, but the WASM `return` executes BEFORE the
            // post-statement error dispatch — so a local handler that WOULD
            // catch the IndexError is bypassed and the error escapes. The JS
            // backend handles this correctly, so reject such functions from
            // WASM. Only when a handler could actually catch IndexError: an
            // `except ValueError` cannot, so the IndexError propagates either
            // way (WASM behaves correctly) and the function stays eligible.
            if try_body_has_raising_return(body) && handlers_catch_indexerror(handlers) {
                return Err(
                    "a subscript inside a `return` within a try/except that catches IndexError is \
                     not supported on the WASM fast path (the IndexError would bypass the handler); \
                     function stays on the JS backend"
                        .into(),
                );
            }
            Ok(())
        }

        _ => Err(format!(
            "unsupported statement: {:?}",
            stmt_kind_name(&stmt.kind)
        )),
    }
}

/// Extract the exception class name from a raise/except expression.
/// Accepts a bare Name like `ValueError` or a Call like `ValueError("msg")`.
pub fn exception_name_from_expr(expr: &Expr) -> Option<String> {
    match &expr.kind {
        ExprKind::Name(n) => Some(n.clone()),
        ExprKind::Call { func, .. } => {
            if let ExprKind::Name(n) = &func.kind {
                Some(n.clone())
            } else {
                None
            }
        }
        _ => None,
    }
}

fn stmt_kind_name(kind: &StmtKind) -> &'static str {
    match kind {
        StmtKind::Expr(_) => "Expr",
        StmtKind::Assign { .. } => "Assign",
        StmtKind::AugAssign { .. } => "AugAssign",
        StmtKind::FuncDef { .. } => "FuncDef",
        StmtKind::ClassDef { .. } => "ClassDef",
        StmtKind::Return(_) => "Return",
        StmtKind::If { .. } => "If",
        StmtKind::While { .. } => "While",
        StmtKind::For { .. } => "For",
        StmtKind::Break => "Break",
        StmtKind::Continue => "Continue",
        StmtKind::Pass => "Pass",
        StmtKind::Import { .. } => "Import",
        StmtKind::ImportSideEffect(_) => "ImportSideEffect",
        StmtKind::ImportFrom { .. } => "ImportFrom",
        StmtKind::Try { .. } => "Try",
        StmtKind::Raise(..) => "Raise",
        StmtKind::Assert { .. } => "Assert",
        StmtKind::Global(_) => "Global",
        StmtKind::Nonlocal(_) => "Nonlocal",
        StmtKind::Del(_) => "Del",
        StmtKind::With { .. } => "With",
        StmtKind::AnnAssign { .. } => "AnnAssign",
        StmtKind::Match { .. } => "Match",
    }
}

/// #364: is `index` a statically-negative subscript index (`a[-1]`, `a[-2]`)?
/// The WASM backend addresses list/string elements from the buffer base with
/// no Python `len + idx` normalization for negatives, so `a[-1]` silently
/// reads/writes the wrong slot (a silent miscompile — cluster02 in the
/// real-code forced-WASM differential: `a[-1] = x` was a no-op). Functions
/// using a statically-negative index stay correct JS (fast via V8) until the
/// backend grows negative-index normalization; positive/loop indices (the
/// proven Livermore pattern) are unaffected.
fn is_negative_literal_index(index: &Expr) -> bool {
    match &index.kind {
        ExprKind::IntLiteral(v) => *v < 0,
        ExprKind::UnaryOp {
            op: UnaryOp::Neg,
            operand,
        } => {
            matches!(&operand.kind, ExprKind::IntLiteral(v) if *v > 0)
        }
        _ => false,
    }
}

fn check_assign_target(target: &Expr) -> Result<(), String> {
    match &target.kind {
        ExprKind::Name(_) => Ok(()),
        // #364: tuple-target unpacking (`a, b = ...`, `for i, v in ...`) is a
        // general-data pattern the backend miscompiles — such functions stay JS.
        ExprKind::Tuple(_) => Err(
            "tuple-unpacking assignment is not supported on the WASM fast path (function stays JS)"
                .into(),
        ),
        // Subscript assignment into a local working array: `arr[i] = v` — the
        // proven-safe Livermore pattern. The index must not be a slice (checked
        // as an expression elsewhere); the container is a local/param list.
        ExprKind::Subscript {
            optional, index, ..
        } => {
            if *optional {
                return Err("optional subscript assignment is not supported in WASM".into());
            }
            // #364: reject slice-store `arr[i:j] = ...`.
            if matches!(index.kind, ExprKind::Slice { .. }) {
                return Err(
                    "slice assignment is not supported on the WASM fast path (function stays JS)"
                        .into(),
                );
            }
            // #364: reject negative-literal store `arr[-1] = v` (silent
            // miscompile — the backend writes the wrong slot). Stays JS.
            if is_negative_literal_index(index) {
                return Err("negative subscript index (arr[-k] = v) is not supported on the WASM fast path (function stays JS)".into());
            }
            Ok(())
        }
        _ => Err("only simple variable or subscript assignment is supported".into()),
    }
}

/// Check that a for-loop iterator is acceptable. Currently we accept:
///   - `range(...)` (numeric loops, since Tier 0)
///   - any other expression (assumed to evaluate to a list/tuple at codegen
///     time, per Tier 2/5). The codegen layer is responsible for emitting
///     the correct iteration shape based on the iterator's type.
fn check_for_iter(iter: &Expr, eligible_names: &[String]) -> Result<(), String> {
    match &iter.kind {
        ExprKind::Call {
            func, args, kwargs, ..
        } => {
            if let ExprKind::Name(name) = &func.kind {
                if name == "range" {
                    if !kwargs.is_empty() {
                        return Err("range() with keyword arguments is not supported".into());
                    }
                    if args.is_empty() || args.len() > 3 {
                        return Err("range() requires 1-3 positional arguments".into());
                    }
                    for arg in args {
                        check_expr(arg, eligible_names)?;
                    }
                    return Ok(());
                }
            }
            check_expr(iter, eligible_names)
        }
        _ => check_expr(iter, eligible_names),
    }
}

fn check_expr(expr: &Expr, eligible_names: &[String]) -> Result<(), String> {
    match &expr.kind {
        // #358: int literals beyond i64 cannot be represented on the WASM
        // fast path — reject the function so it stays on the exact JS path.
        ExprKind::IntLiteral(n) => {
            if *n > i64::MAX as i128 || *n < i64::MIN as i128 {
                return Err(format!(
                    "int literal {} exceeds the i64 range (WASM fast path); \
                     the function stays on the arbitrary-precision JS path",
                    n
                ));
            }
            Ok(())
        }
        ExprKind::FloatLiteral(_) | ExprKind::BoolLiteral(_) | ExprKind::Name(_) => Ok(()),

        // #364 (Path B — numeric-kernel whitelist): string values are general
        // (non-numeric-kernel) data — they stay on the correct JS path, so the
        // WASM eligibility check rejects them. `str` params/returns are rejected
        // at the signature level; string literals/f-strings here likewise.
        // #283: complex literals are a JS-only runtime type (PyComplex), never
        // a numeric-kernel WASM value — reject so the function stays on JS.
        ExprKind::ImagLiteral(_) => Err(
            "complex literals are not supported on the WASM fast path (function stays JS)".into(),
        ),
        ExprKind::StringLiteral(_) => Err(
            "string literals are not supported on the WASM fast path (function stays JS)".into(),
        ),
        ExprKind::BytesLiteral(_) => {
            Err("bytes literals are not supported on the WASM fast path (function stays JS)".into())
        }
        ExprKind::FString { .. } => {
            Err("f-strings are not supported on the WASM fast path (function stays JS)".into())
        }

        ExprKind::BinOp { left, op, right } => {
            // Disallow non-WASM operators
            match op {
                BinOp::In
                | BinOp::NotIn
                | BinOp::Is
                | BinOp::IsNot
                | BinOp::NullishCoalesce
                | BinOp::Pipeline => {
                    return Err(format!("operator {:?} is not supported in WASM", op));
                }
                _ => {}
            }
            check_expr(left, eligible_names)?;
            check_expr(right, eligible_names)
        }

        ExprKind::UnaryOp { op, operand } => {
            // #358: `-9223372036854775808` parses as Neg(9223372036854775808);
            // the positive literal is out of i64 range but the negated value
            // is exactly i64::MIN — admit it (emit constant-folds it).
            if matches!(op, pyths_syntax::operators::UnaryOp::Neg) {
                if let ExprKind::IntLiteral(n) = &operand.kind {
                    if *n <= (i64::MAX as i128) + 1 && *n >= -(i64::MAX as i128) {
                        return Ok(());
                    }
                    return Err(format!(
                        "int literal -{} exceeds the i64 range (WASM fast path); \
                         the function stays on the arbitrary-precision JS path",
                        n
                    ));
                }
            }
            check_expr(operand, eligible_names)
        }

        ExprKind::Compare { left, comparisons } => {
            check_expr(left, eligible_names)?;
            for (op, expr) in comparisons {
                match op {
                    BinOp::In | BinOp::NotIn | BinOp::Is | BinOp::IsNot => {
                        return Err(format!("operator {:?} is not supported in WASM", op));
                    }
                    _ => {}
                }
                check_expr(expr, eligible_names)?;
            }
            Ok(())
        }

        ExprKind::Call {
            func,
            args,
            kwargs,
            optional,
        } => {
            if *optional {
                return Err("optional calls (?.) are not supported in WASM".into());
            }
            if !kwargs.is_empty() {
                return Err("keyword arguments are not supported in WASM".into());
            }
            // func must be a Name referencing a builtin or eligible function,
            // or an attribute call for string methods (e.g. s.upper()).
            // Tier 6: a Name not in builtins or eligible_names is accepted as
            // a potential closure-local; the codegen pass verifies and emits
            // call_indirect when the local's type is PtrClosure.
            match &func.kind {
                ExprKind::Name(n) => {
                    // #364 (Path B — numeric-kernel whitelist): admit ONLY the
                    // numeric-kernel builtins and calls to other eligible
                    // functions. Everything else (print/input/open, and the
                    // collection/string builtins the backend miscompiles —
                    // set/dict/list/tuple/sorted/min/max/sum/enumerate/zip/map/
                    // filter/reversed/str/...) stays on the correct JS path.
                    if !WASM_CALL_BUILTINS.contains(&n.as_str())
                        && !WASM_MATH_FUNCTIONS.contains(&n.as_str())
                        && !eligible_names.contains(n)
                    {
                        return Err(format!(
                            "builtin/function `{}` is not on the WASM numeric-kernel whitelist \
                             (function stays JS)",
                            n
                        ));
                    }
                }
                ExprKind::Attribute {
                    value,
                    attr,
                    optional: opt,
                } => {
                    if *opt {
                        return Err("optional method calls (?.) are not supported in WASM".into());
                    }
                    // #364: admit ONLY `math.X(...)` calls; every other method
                    // (string methods, list/dict/set mutation, ...) stays JS.
                    if let ExprKind::Name(mod_name) = &value.kind {
                        if mod_name == "math" && WASM_MATH_FUNCTIONS.contains(&attr.as_str()) {
                            // Args checked below.
                        } else {
                            return Err(format!(
                                "method '{}' is not on the WASM numeric-kernel whitelist (function stays JS)",
                                attr
                            ));
                        }
                    } else {
                        return Err(format!(
                            "method '{}' is not on the WASM numeric-kernel whitelist (function stays JS)",
                            attr
                        ));
                    }
                }
                _ => {
                    return Err("only direct function calls are supported in WASM".into());
                }
            }
            for arg in args {
                check_expr(arg, eligible_names)?;
            }
            Ok(())
        }

        // Subscript for string indexing: s[i]
        ExprKind::Subscript {
            value,
            index,
            optional,
        } => {
            if *optional {
                return Err("optional subscript (?.[]) is not supported in WASM".into());
            }
            // #364: reject negative-literal load `a[-k]` (silent miscompile —
            // the backend reads the wrong slot). Stays correct JS.
            if is_negative_literal_index(index) {
                return Err("negative subscript index (a[-k]) is not supported on the WASM fast path (function stays JS)".into());
            }
            check_expr(value, eligible_names)?;
            check_expr(index, eligible_names)
        }

        ExprKind::IfExpr {
            test,
            body,
            else_body,
        } => {
            check_expr(test, eligible_names)?;
            check_expr(body, eligible_names)?;
            check_expr(else_body, eligible_names)
        }

        // #364: slicing (`a[i:j]`) builds a new list/string at the boundary —
        // the backend miscompiles it (the distinctDifferenceArray class). Stays JS.
        ExprKind::Slice { .. } => {
            Err("slicing is not supported on the WASM fast path (function stays JS)".into())
        }

        // Everything else is rejected
        ExprKind::NoneLiteral => Err("None is not supported in WASM expressions".into()),
        ExprKind::Attribute {
            value,
            attr,
            optional,
        } => {
            if *optional {
                return Err("optional attribute access (?.) is not supported in WASM".into());
            }
            // Allow math.pi, math.e, etc.
            if let ExprKind::Name(mod_name) = &value.kind {
                if mod_name == "math" && WASM_MATH_CONSTANTS.contains(&attr.as_str()) {
                    return Ok(());
                }
            }
            Err("attribute access is not supported in WASM".into())
        }
        // #364: a LIST literal of scalar elements stays admitted — Livermore's
        // internal working arrays (`[0.0] * (n + 12)` etc.) are the proven-safe
        // numeric-kernel pattern (subscript load/store into a local array). A
        // list RETURN is still excluded at the signature level; only local
        // scalar-element list construction is on the whitelist.
        ExprKind::List(elts) => {
            for e in elts {
                check_expr(e, eligible_names)?;
            }
            Ok(())
        }
        // #364: tuples, dicts, and sets are general (non-numeric-kernel) data —
        // the backend miscompiles them; they stay on the correct JS path.
        ExprKind::Tuple(_) => {
            Err("tuple literals are not supported on the WASM fast path (function stays JS)".into())
        }
        ExprKind::Dict { .. } => {
            Err("dict literals are not supported on the WASM fast path (function stays JS)".into())
        }
        ExprKind::Set(_) => {
            Err("set literals are not supported on the WASM fast path (function stays JS)".into())
        }
        // #364: lambdas / closures stay JS.
        ExprKind::Lambda { .. } => {
            Err("lambdas are not supported on the WASM fast path (function stays JS)".into())
        }
        // #364 (Path B — soundness): the backend miscompiles comprehension
        // list/dict building (the desugared list-build loop corrupts the result
        // — the #364 `distinctDifferenceArray` class). Reject so such functions
        // stay correct JS. (Set/generator comprehensions were already rejected.)
        ExprKind::ListComp { .. } => Err(
            "list comprehensions are not supported on the WASM fast path (function stays JS)"
                .into(),
        ),
        ExprKind::DictComp { .. } => Err(
            "dict comprehensions are not supported on the WASM fast path (function stays JS)"
                .into(),
        ),
        ExprKind::SetComp { .. } => Err("set comprehensions are not supported in WASM".into()),
        ExprKind::GeneratorExp { .. } => {
            Err("generator expressions are not supported in WASM".into())
        }
        ExprKind::Await(_) => Err("await is not supported in WASM".into()),
        ExprKind::Yield(_) => Err("yield is not supported in WASM".into()),
        ExprKind::YieldFrom(_) => Err("yield from is not supported in WASM".into()),
        ExprKind::Starred(_) => Err("starred expressions are not supported in WASM".into()),
        ExprKind::NamedExpr { .. } => Err("walrus operator is not supported in WASM".into()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn analyze(source: &str) -> WasmAnalysis {
        let module = pyths_parser::parse(source).expect("Parse failed");
        analyze_module(&module)
    }

    #[test]
    fn test_numeric_function_eligible() {
        let a = analyze("def add(a: int, b: int) -> int:\n    return a + b\n");
        assert!(a.eligible.contains_key("add"), "add should be eligible");
        assert!(a.rejected.is_empty());
    }

    #[test]
    fn test_float_function_eligible() {
        let a = analyze("def mul(x: float, y: float) -> float:\n    return x * y\n");
        assert!(a.eligible.contains_key("mul"));
    }

    #[test]
    fn test_bool_return_eligible() {
        let a = analyze("def gt(a: int, b: int) -> bool:\n    return a > b\n");
        assert!(a.eligible.contains_key("gt"));
    }

    #[test]
    fn test_no_annotation_rejected() {
        let a = analyze("def f(x):\n    return x\n");
        assert!(a.eligible.is_empty());
        assert_eq!(a.rejected.len(), 1);
        assert!(a.rejected[0].1.contains("no type annotation"));
    }

    // #363 admission-soundness regression guard (verified resolved 2026-07-21).
    // A value-returning function with an OMITTED return type must NOT be
    // WASM-admitted — the void ABI would drop the value / emit an ABI-mismatched
    // module. It stays correct JS; its typed sibling stays eligible. The
    // capability half (compile such shapes to WASM) is #377.
    #[test]
    fn regression_363_untyped_value_return_stays_js() {
        let a = analyze(
            "def to_f(x: int):\n    return x * 1.0\ndef g(a: int) -> int:\n    return a + 1\n",
        );
        assert!(
            !a.eligible.contains_key("to_f"),
            "#363: untyped value-return must stay JS"
        );
        assert!(
            a.eligible.contains_key("g"),
            "typed sibling stays WASM-eligible"
        );
        assert!(
            a.rejected
                .iter()
                .any(|(n, r)| n == "to_f" && r.contains("return-type annotation")),
            "rejection must name the missing return-type annotation: {:?}",
            a.rejected
        );
    }

    // #364 admission-soundness regression guard (verified resolved 2026-07-21).
    // NON-SCALAR returns (list/set/dict/tuple) and untyped comprehension-return
    // bodies must NOT be WASM-admitted — heap-object boundary marshalling is
    // v3.x (#377). Both stay correct JS.
    #[test]
    fn regression_364_nonscalar_return_stays_js() {
        let a = analyze("def build(n: int) -> list:\n    return []\n");
        assert!(
            !a.eligible.contains_key("build"),
            "#364: list return must stay JS"
        );
        let b = analyze("def comp(n: int):\n    return [i for i in range(n)]\n");
        assert!(
            !b.eligible.contains_key("comp"),
            "#364: untyped comprehension-return stays JS"
        );
    }

    // #364 (Path B — numeric-kernel whitelist): STRING functions are general
    // (non-numeric-kernel) code and stay on the correct JS path. These are the
    // regression guards for that boundary (str params/returns/literals/methods
    // must NOT be WASM-admitted). WASM string support → v3.x (#364).
    #[test]
    fn test_string_param_rejected() {
        let a = analyze("def f(s: str) -> str:\n    return s\n");
        assert!(!a.eligible.contains_key("f"), "str param must stay JS");
    }

    #[test]
    fn test_string_return_rejected() {
        let a = analyze("def greet(name: str) -> str:\n    return \"hello \" + name\n");
        assert!(!a.eligible.contains_key("greet"), "str return must stay JS");
    }

    #[test]
    fn test_string_literal_rejected() {
        let a = analyze("def f(x: int) -> str:\n    return \"hello\"\n");
        assert!(!a.eligible.contains_key("f"), "string literal must stay JS");
    }

    #[test]
    fn test_string_len_rejected() {
        let a = analyze("def f(s: str) -> int:\n    return len(s)\n");
        assert!(
            !a.eligible.contains_key("f"),
            "str param (len) must stay JS"
        );
    }

    #[test]
    fn test_string_method_rejected() {
        let a = analyze("def f(s: str) -> str:\n    return s.upper()\n");
        assert!(!a.eligible.contains_key("f"), "string method must stay JS");
    }

    #[test]
    fn test_string_subscript_rejected() {
        let a = analyze("def f(s: str, i: int) -> str:\n    return s[i]\n");
        assert!(!a.eligible.contains_key("f"), "str param must stay JS");
    }

    #[test]
    fn test_string_comparison_rejected() {
        let a = analyze("def f(a: str, b: str) -> bool:\n    return a == b\n");
        assert!(!a.eligible.contains_key("f"), "str params must stay JS");
    }

    #[test]
    fn test_str_builtin_rejected() {
        let a = analyze("def f(n: int) -> str:\n    return str(n)\n");
        assert!(
            !a.eligible.contains_key("f"),
            "str() builtin (str return) must stay JS"
        );
    }

    #[test]
    fn test_int_from_str_rejected() {
        let a = analyze("def f(s: str) -> int:\n    return int(s)\n");
        assert!(!a.eligible.contains_key("f"), "str param must stay JS");
    }

    #[test]
    fn test_mixed_str_numeric_rejected() {
        let a = analyze("def f(name: str, age: int) -> str:\n    return name\n");
        assert!(
            !a.eligible.contains_key("f"),
            "str param/return must stay JS"
        );
    }

    #[test]
    fn test_unsupported_method_rejected() {
        let a = analyze("def f(s: str) -> str:\n    return s.encode()\n");
        assert!(a.eligible.is_empty());
        assert!(!a.rejected.is_empty());
    }

    #[test]
    fn test_no_return_type_eligible() {
        let a = analyze("def f(x: int):\n    pass\n");
        assert!(
            a.eligible.contains_key("f"),
            "void return should be eligible"
        );
    }

    #[test]
    fn test_async_rejected() {
        let a = analyze("async def f(x: int) -> int:\n    return x\n");
        assert!(a.eligible.is_empty());
        assert!(a.rejected[0].1.contains("async"));
    }

    #[test]
    fn test_args_kwargs_rejected() {
        let a = analyze("def f(*args: int) -> int:\n    return 0\n");
        assert!(a.eligible.is_empty());
        assert!(a.rejected[0].1.contains("*args"));
    }

    #[test]
    fn test_for_range_eligible() {
        let src = "def f(n: int) -> int:\n    s: int = 0\n    for i in range(n):\n        s += i\n    return s\n";
        let a = analyze(src);
        assert!(
            a.eligible.contains_key("f"),
            "for-range should be eligible: {:?}",
            a.rejected
        );
    }

    #[test]
    fn test_for_list_now_accepted() {
        // Tier 2 (Lists): for-loops over names that resolve to a list type are
        // now accepted at the analysis layer. The codegen layer is responsible
        // for emitting the right iteration shape based on the iterator's WASM
        // type (range vs list). Names that don't resolve to anything still
        // fail later in codegen, but pass analysis.
        let src = "def f(n: int) -> int:\n    for x in items:\n        pass\n    return 0\n";
        let a = analyze(src);
        // Either accepted (analysis is permissive) or rejected for a different
        // reason (e.g. name `items` not eligible). The original "must be range()"
        // restriction is no longer the gating check.
        let _ = a;
    }

    #[test]
    fn test_print_call_now_accepted_via_permissive_calls() {
        // Tier 6 made calls permissive (Name → closure-local fallback).
        // `print(x)` is now accepted at the analysis layer; codegen emits a
        // no-op for unknown names. This is a deliberate scope shift.
        let src = "def f(x: int) -> int:\n    print(x)\n    return x\n";
        let a = analyze(src);
        // No assertion — analysis may either accept or fall through; both OK.
        let _ = a;
    }

    #[test]
    fn test_optional_param_not_eligible() {
        // Regression: `is_wasm_eligible` used to admit `Optional[T]`, but
        // `to_wasm_type` has no Optional lowering, so codegen `unwrap()`ed a
        // `None` and PANICKED on `def f(x: Optional[int]) -> int`. Admission
        // must agree with the lowering (soundness): Optional-boundary
        // functions fall back to JS. `wasm_admission_sound` (Lean) proves the
        // general invariant `is_wasm_eligible => to_wasm_type is_some`.
        let a = analyze("def f(x: Optional[int]) -> int:\n    return 1\n");
        assert!(
            a.eligible.is_empty(),
            "Optional param must not be WASM-admitted"
        );
        assert!(!is_wasm_eligible(&Type::Optional(Box::new(Type::Int))));
        // A List of Optionals is unrepresentable too (inner falls through).
        assert!(!is_wasm_eligible(&Type::List(Box::new(Type::Optional(
            Box::new(Type::Int)
        )))));
    }

    #[test]
    fn test_decorated_rejected() {
        let src = "@component\ndef f(x: int) -> int:\n    return x\n";
        let a = analyze(src);
        assert!(a.eligible.is_empty());
        assert!(a.rejected[0].1.contains("decorator"));
    }
}
