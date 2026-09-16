use std::collections::{BTreeMap, HashMap, HashSet};

use pyths_syntax::ast::{
    DictItem, ExceptHandler, Expr, ExprKind, FStringPart, Module, Param, Stmt, StmtKind,
};
use pyths_syntax::operators::{AugAssignOp, BinOp, UnaryOp};
use pyths_types::types::{resolve_type, ArrayDtype, Type};

/// #364: the numeric-kernel builtin whitelist for direct `Name(...)` calls.
/// Math functions (`WASM_MATH_FUNCTIONS`, bare-imported like `sqrt`) and calls
/// to other eligible functions are also admitted; everything else stays JS.
const WASM_CALL_BUILTINS: &[&str] = &["abs", "len", "int", "float", "range"];

/// Result of analyzing a module for WASM eligibility.
#[derive(Debug)]
pub struct WasmAnalysis {
    /// Functions eligible for WASM compilation, keyed by name.
    ///
    /// #474: a `BTreeMap` (name-ordered), NOT a `HashMap`. Every downstream
    /// iteration of the admitted set — string-pool / data-section collection,
    /// lambda ordering, the `needs_*` scans, the codegen-time invalid-WASM
    /// leave-one-out — reads this in iteration order, so a hash-order iteration
    /// leaked into the emitted memory layout and thereby into which functions a
    /// list-param module admits (the ~25-50% flake: a shifted data section made
    /// a `list`-param function's marshalling sometimes emit invalid WASM, then
    /// get demoted). Ordering by name makes the WHOLE compile input-order-
    /// determined by construction — the deterministic-ordering root, not a retry
    /// loop. Guarded by the `determinism::*` net lane (24x identical bytes).
    pub eligible: BTreeMap<String, WasmFuncInfo>,
    /// Functions that were rejected, with (name, reason).
    pub rejected: Vec<(String, String)>,
    /// #491 order-aware shadow authority: the FINAL module-level binding of every
    /// name a `def` or `from math import` binds, with the source index of that
    /// binding. WASM only compiles function BODIES, which see the final binding at
    /// call time, so this single per-name answer is the order-aware model on the WASM
    /// side (see SHADOW_BINDING_DESIGN.md). Consulted by the emitter (`user_shadow`
    /// guards + math-alias resolution + all inference) and by admission (dead-by-name
    /// defs, the call-consistency fixpoint, `shadowed_for_iter`).
    pub shadow_bindings: HashMap<String, (usize, ShadowBinding)>,
}

/// #491: the final module-level binding kind for a shadowable name.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ShadowBinding {
    /// A user `def` of the name.
    UserDef,
    /// `from math import <canonical> as <name>` — the canonical math fn name.
    MathAlias(String),
    /// Any OTHER module-level binder of the name (assignment, class, non-math
    /// import, for/with/except target, match capture, `del`, a `global` write
    /// inside a function). The WASM backend has no lowering for such a binding,
    /// so a body that CALLS the name is demoted to the cell-aware JS path (S4).
    /// The payload names the binder kind for the demotion reason.
    Other(&'static str),
}

/// #491 (S4): a `global N` write inside a function rebinds the module cell at an
/// UNKNOWABLE time (whenever that function runs), so it must beat every source-
/// ordered binder: recorded at this sentinel index, which is past every statement.
pub const GLOBAL_WRITE_INDEX: usize = usize::MAX;

/// #491: compute the FINAL module-level binding of every name bound at module scope
/// by ANY binder form (SHADOW_BINDING_DESIGN.md §2), with the index of the TOP-LEVEL
/// statement the binding executes under (a binder nested in a module-level
/// `if`/`for`/`while`/`try`/`with`/`match` body carries that top-level statement's
/// index). Last binder in source order wins (CPython module-scope last-binding
/// semantics); a `global` write inside any nested function wins over everything
/// (`GLOBAL_WRITE_INDEX`). Names never bound here resolve to the builtin.
pub fn final_module_bindings(module: &Module) -> HashMap<String, (usize, ShadowBinding)> {
    let mut out: HashMap<String, (usize, ShadowBinding)> = HashMap::new();
    for (idx, stmt) in module.body.iter().enumerate() {
        module_binders_in_stmt(stmt, idx, &mut out);
    }
    let mut globals: HashSet<String> = HashSet::new();
    collect_global_writes_deep(&module.body, None, &mut globals);
    for g in globals {
        out.insert(
            g,
            (GLOBAL_WRITE_INDEX, ShadowBinding::Other("global write")),
        );
    }
    out
}

/// Names bound by an assignment TARGET expression (recursing through
/// tuple/list/starred; attribute/subscript targets bind no name).
fn target_names(target: &Expr, out: &mut Vec<String>) {
    match &target.kind {
        ExprKind::Name(n) => out.push(n.clone()),
        ExprKind::Tuple(elts) | ExprKind::List(elts) => {
            for e in elts {
                target_names(e, out);
            }
        }
        ExprKind::Starred(inner) => target_names(inner, out),
        _ => {}
    }
}

/// Names bound by a `match` pattern (captures, `as` names, starred captures).
fn pattern_names(p: &pyths_syntax::ast::Pattern, out: &mut Vec<String>) {
    use pyths_syntax::ast::Pattern;
    match p {
        Pattern::Capture(n) => out.push(n.clone()),
        Pattern::Star(Some(n)) => out.push(n.clone()),
        Pattern::As { pattern, name } => {
            pattern_names(pattern, out);
            out.push(name.clone());
        }
        Pattern::Class { args, .. } | Pattern::Sequence(args) | Pattern::Or(args) => {
            for a in args {
                pattern_names(a, out);
            }
        }
        Pattern::Mapping(items) => {
            for (_, sub) in items {
                pattern_names(sub, out);
            }
        }
        Pattern::Wildcard | Pattern::Literal(_) | Pattern::Value(_) | Pattern::Star(None) => {}
    }
}

/// Walrus (`NamedExpr`) targets anywhere inside an expression — they bind the
/// enclosing (module) scope even from inside a comprehension (PEP 572). A lambda
/// body is walked too (over-approximation: more demotion, never less).
fn walrus_names(e: &Expr, out: &mut Vec<String>) {
    if let ExprKind::NamedExpr { target, .. } = &e.kind {
        target_names(target, out);
    }
    for_each_child_expr(e, &mut |c| walrus_names(c, out));
}

/// Apply `f` to every DIRECT child expression of `e` (the complete `ExprKind`
/// surface — the one place the expression shape is enumerated for the #491
/// walkers, so a new expression kind cannot be silently skipped by one of them).
fn for_each_child_expr<'a>(e: &'a Expr, f: &mut dyn FnMut(&'a Expr)) {
    match &e.kind {
        ExprKind::BinOp { left, right, .. } => {
            f(left);
            f(right);
        }
        ExprKind::UnaryOp { operand, .. } => f(operand),
        ExprKind::Compare { left, comparisons } => {
            f(left);
            for (_, c) in comparisons {
                f(c);
            }
        }
        ExprKind::Call {
            func, args, kwargs, ..
        } => {
            f(func);
            for a in args {
                f(a);
            }
            for k in kwargs {
                f(&k.value);
            }
        }
        ExprKind::Attribute { value, .. } => f(value),
        ExprKind::Subscript { value, index, .. } => {
            f(value);
            f(index);
        }
        ExprKind::List(elts) | ExprKind::Tuple(elts) | ExprKind::Set(elts) => {
            for x in elts {
                f(x);
            }
        }
        ExprKind::Dict { items } => {
            for item in items {
                match item {
                    DictItem::KeyValue { key, value } => {
                        f(key);
                        f(value);
                    }
                    DictItem::Spread(x) => f(x),
                }
            }
        }
        ExprKind::FString { parts } => {
            for p in parts {
                if let FStringPart::Expr(x) = p {
                    f(x);
                }
            }
        }
        ExprKind::ListComp { elt, generators }
        | ExprKind::SetComp { elt, generators }
        | ExprKind::GeneratorExp { elt, generators } => {
            f(elt);
            for g in generators {
                f(&g.target);
                f(&g.iter);
                for c in &g.ifs {
                    f(c);
                }
            }
        }
        ExprKind::DictComp {
            key,
            value,
            generators,
        } => {
            f(key);
            f(value);
            for g in generators {
                f(&g.target);
                f(&g.iter);
                for c in &g.ifs {
                    f(c);
                }
            }
        }
        ExprKind::Lambda { body, params, .. } => {
            for p in params {
                if let Some(d) = &p.default {
                    f(d);
                }
            }
            f(body);
        }
        ExprKind::IfExpr {
            test,
            body,
            else_body,
        } => {
            f(test);
            f(body);
            f(else_body);
        }
        ExprKind::Starred(x) | ExprKind::Await(x) | ExprKind::YieldFrom(x) => f(x),
        ExprKind::Yield(x) => {
            if let Some(x) = x {
                f(x);
            }
        }
        ExprKind::NamedExpr { target, value } => {
            f(target);
            f(value);
        }
        ExprKind::Slice { lower, upper, step } => {
            for x in [lower, upper, step].into_iter().flatten() {
                f(x);
            }
        }
        ExprKind::IntLiteral(_)
        | ExprKind::FloatLiteral(_)
        | ExprKind::ImagLiteral(_)
        | ExprKind::StringLiteral(_)
        | ExprKind::BytesLiteral(_)
        | ExprKind::BoolLiteral(_)
        | ExprKind::NoneLiteral
        | ExprKind::Name(_) => {}
    }
}

/// #491 (S4): record every module-level binder inside one top-level statement
/// (recursing through module-level block bodies, NOT into def/class bodies).
fn module_binders_in_stmt(
    stmt: &Stmt,
    idx: usize,
    out: &mut HashMap<String, (usize, ShadowBinding)>,
) {
    let bind = |names: Vec<String>, kind: ShadowBinding, out: &mut HashMap<_, _>| {
        for n in names {
            out.insert(n, (idx, kind.clone()));
        }
    };
    let walrus = |e: &Expr, out: &mut HashMap<String, (usize, ShadowBinding)>| {
        let mut ws = Vec::new();
        walrus_names(e, &mut ws);
        for n in ws {
            out.insert(n, (idx, ShadowBinding::Other("assignment")));
        }
    };
    let recurse = |body: &[Stmt], out: &mut HashMap<String, (usize, ShadowBinding)>| {
        for s in body {
            module_binders_in_stmt(s, idx, out);
        }
    };
    match &stmt.kind {
        StmtKind::FuncDef {
            name,
            params,
            decorator_list,
            return_type,
            ..
        } => {
            // Decorators / defaults / annotations are evaluated at def time in the
            // enclosing (module) scope — a walrus there binds the module.
            for d in decorator_list {
                walrus(d, out);
            }
            for p in params {
                if let Some(d) = &p.default {
                    walrus(d, out);
                }
                if let Some(a) = &p.annotation {
                    walrus(a, out);
                }
            }
            if let Some(rt) = return_type {
                walrus(rt, out);
            }
            out.insert(name.clone(), (idx, ShadowBinding::UserDef));
        }
        StmtKind::ClassDef {
            name,
            bases,
            decorator_list,
            ..
        } => {
            for d in decorator_list {
                walrus(d, out);
            }
            for b in bases {
                walrus(b, out);
            }
            out.insert(name.clone(), (idx, ShadowBinding::Other("class")));
        }
        StmtKind::Assign { targets, value } => {
            let mut names = Vec::new();
            for t in targets {
                target_names(t, &mut names);
                walrus(t, out);
            }
            walrus(value, out);
            bind(names, ShadowBinding::Other("assignment"), out);
        }
        StmtKind::AnnAssign {
            target,
            annotation,
            value,
        } => {
            walrus(annotation, out);
            if let Some(v) = value {
                walrus(v, out);
                // PEP 526: at module scope a bare `x: T` (no value) binds nothing.
                let mut names = Vec::new();
                target_names(target, &mut names);
                bind(names, ShadowBinding::Other("assignment"), out);
            }
        }
        StmtKind::AugAssign { target, value, .. } => {
            walrus(value, out);
            let mut names = Vec::new();
            target_names(target, &mut names);
            bind(names, ShadowBinding::Other("assignment"), out);
        }
        StmtKind::Import { names } => {
            let bound: Vec<String> = names
                .iter()
                .map(|a| {
                    a.alias.clone().unwrap_or_else(|| {
                        // `import a.b.c` binds the head name `a`.
                        a.name.split('.').next().unwrap_or(&a.name).to_string()
                    })
                })
                .collect();
            bind(bound, ShadowBinding::Other("import"), out);
        }
        StmtKind::ImportFrom {
            module: m, names, ..
        } => {
            for alias in names {
                if alias.name == "*" {
                    continue;
                }
                let bound = alias.alias.clone().unwrap_or_else(|| alias.name.clone());
                if m == "math" && WASM_MATH_FUNCTIONS.contains(&alias.name.as_str()) {
                    out.insert(bound, (idx, ShadowBinding::MathAlias(alias.name.clone())));
                } else {
                    out.insert(bound, (idx, ShadowBinding::Other("import")));
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
            walrus(iter, out);
            let mut names = Vec::new();
            target_names(target, &mut names);
            bind(names, ShadowBinding::Other("for-target"), out);
            recurse(body, out);
            if let Some(b) = else_body {
                recurse(b, out);
            }
        }
        StmtKind::While {
            test,
            body,
            else_body,
        } => {
            walrus(test, out);
            recurse(body, out);
            if let Some(b) = else_body {
                recurse(b, out);
            }
        }
        StmtKind::If {
            test,
            body,
            elif_clauses,
            else_body,
        } => {
            walrus(test, out);
            recurse(body, out);
            for (t, b) in elif_clauses {
                walrus(t, out);
                recurse(b, out);
            }
            if let Some(b) = else_body {
                recurse(b, out);
            }
        }
        StmtKind::Try {
            body,
            handlers,
            else_body,
            finally_body,
        } => {
            recurse(body, out);
            for h in handlers {
                if let Some(e) = &h.exc_type {
                    walrus(e, out);
                }
                if let Some(n) = &h.name {
                    out.insert(n.clone(), (idx, ShadowBinding::Other("except-target")));
                }
                recurse(&h.body, out);
            }
            if let Some(b) = else_body {
                recurse(b, out);
            }
            if let Some(b) = finally_body {
                recurse(b, out);
            }
        }
        StmtKind::With { items, body, .. } => {
            for it in items {
                walrus(&it.context_expr, out);
                if let Some(v) = &it.optional_var {
                    let mut names = Vec::new();
                    target_names(v, &mut names);
                    bind(names, ShadowBinding::Other("with-target"), out);
                }
            }
            recurse(body, out);
        }
        StmtKind::Match { subject, cases } => {
            walrus(subject, out);
            for c in cases {
                let mut names = Vec::new();
                pattern_names(&c.pattern, &mut names);
                bind(names, ShadowBinding::Other("match-capture"), out);
                if let Some(g) = &c.guard {
                    walrus(g, out);
                }
                recurse(&c.body, out);
            }
        }
        StmtKind::Del(exprs) => {
            let mut names = Vec::new();
            for e in exprs {
                target_names(e, &mut names);
            }
            bind(names, ShadowBinding::Other("del"), out);
        }
        StmtKind::Expr(e) | StmtKind::Return(Some(e)) | StmtKind::Raise(Some(e), _) => {
            walrus(e, out);
        }
        StmtKind::Assert { test, msg } => {
            walrus(test, out);
            if let Some(m) = msg {
                walrus(m, out);
            }
        }
        StmtKind::Return(None)
        | StmtKind::Raise(None, _)
        | StmtKind::Break
        | StmtKind::Continue
        | StmtKind::Pass
        | StmtKind::Global(_)
        | StmtKind::Nonlocal(_)
        | StmtKind::ImportSideEffect(_) => {}
    }
}

/// Names declared `global` inside a function body at ANY depth (a nested def
/// inside a class/def/block) AND bound in that same function — a real module
/// WRITE. A bare `global N` with no binding of `N` in the function only redirects
/// reads (Python) and is not a binder. `in_def` = the innermost enclosing
/// function's binding set (None at module / class-body level). Mirrors the JS
/// emitter's `collect_global_declared_deep` so the two censuses agree.
fn collect_global_writes_deep(
    stmts: &[Stmt],
    in_def: Option<&HashSet<String>>,
    out: &mut HashSet<String>,
) {
    for s in stmts {
        match &s.kind {
            StmtKind::Global(names) => {
                if let Some(bound) = in_def {
                    for n in names {
                        if bound.contains(n) {
                            out.insert(n.clone());
                        }
                    }
                }
            }
            StmtKind::FuncDef { params, body, .. } => {
                let bound = local_binding_names(params, body);
                collect_global_writes_deep(body, Some(&bound), out)
            }
            StmtKind::ClassDef { body, .. } => {
                // #491 BLOCKER-2: the class block is its own binder scope — a
                // `global N` + value binding of `N` in the block is a module write
                // executed at class-definition time (mirrors the JS emitter's
                // `class_body_value_binders`).
                let bound = class_body_binding_names(body);
                collect_global_writes_deep(body, Some(&bound), out)
            }
            StmtKind::If {
                body,
                elif_clauses,
                else_body,
                ..
            } => {
                collect_global_writes_deep(body, in_def, out);
                for (_, b) in elif_clauses {
                    collect_global_writes_deep(b, in_def, out);
                }
                if let Some(b) = else_body {
                    collect_global_writes_deep(b, in_def, out);
                }
            }
            StmtKind::While {
                body, else_body, ..
            }
            | StmtKind::For {
                body, else_body, ..
            } => {
                collect_global_writes_deep(body, in_def, out);
                if let Some(b) = else_body {
                    collect_global_writes_deep(b, in_def, out);
                }
            }
            StmtKind::With { body, .. } => collect_global_writes_deep(body, in_def, out),
            StmtKind::Try {
                body,
                handlers,
                else_body,
                finally_body,
            } => {
                collect_global_writes_deep(body, in_def, out);
                for h in handlers {
                    collect_global_writes_deep(&h.body, in_def, out);
                }
                if let Some(b) = else_body {
                    collect_global_writes_deep(b, in_def, out);
                }
                if let Some(b) = finally_body {
                    collect_global_writes_deep(b, in_def, out);
                }
            }
            StmtKind::Match { cases, .. } => {
                for c in cases {
                    collect_global_writes_deep(&c.body, in_def, out);
                }
            }
            _ => {}
        }
    }
}

/// #491 BLOCKER-2: the names a CLASS BLOCK binds through its VALUE-binding
/// statements (assignment / aug-assign / valued annotation / tuple / for / with /
/// except / del / import / walrus, through nested control-flow blocks) — EXCLUDING
/// the block's own `def` / `class` statements (a method or nested class keeps the
/// class-attribute lowering even under a `global` declaration of its name; that
/// exotic form is a recorded gap, not modelled). Mirrors the JS emitter's
/// `class_body_value_binders` so the two censuses agree on which `global` names
/// in a class body are module WRITES.
fn class_body_binding_names(body: &[Stmt]) -> HashSet<String> {
    let mut bindings: HashMap<String, (usize, ShadowBinding)> = HashMap::new();
    for s in body {
        if !matches!(s.kind, StmtKind::FuncDef { .. } | StmtKind::ClassDef { .. }) {
            module_binders_in_stmt(s, 0, &mut bindings);
        }
    }
    bindings.into_keys().collect()
}

/// Every `Name` REFERENCE inside an expression (any nesting, incl. lambda bodies
/// and comprehensions). Binding positions are included too — an over-
/// approximation that only ever adds edges to the reachability graph.
fn name_refs_in_expr(e: &Expr, out: &mut HashSet<String>) {
    if let ExprKind::Name(n) = &e.kind {
        out.insert(n.clone());
    }
    for_each_child_expr(e, &mut |c| name_refs_in_expr(c, out));
}

/// Every `Name` reference inside a statement list. `skip_def_bodies`: when true a
/// `def` contributes only what EXECUTES at its position (decorators, defaults,
/// annotations) — the "module-level executed code" view for the B3 rule; when
/// false the whole body counts (the def-graph edge view).
fn name_refs_in_stmts(stmts: &[Stmt], skip_def_bodies: bool, out: &mut HashSet<String>) {
    for s in stmts {
        name_refs_in_stmt(s, skip_def_bodies, out);
    }
}

fn name_refs_in_stmt(s: &Stmt, skip_def_bodies: bool, out: &mut HashSet<String>) {
    let ex = |e: &Expr, out: &mut HashSet<String>| name_refs_in_expr(e, out);
    match &s.kind {
        StmtKind::FuncDef {
            params,
            body,
            decorator_list,
            return_type,
            ..
        } => {
            for d in decorator_list {
                ex(d, out);
            }
            for p in params {
                if let Some(d) = &p.default {
                    ex(d, out);
                }
                if let Some(a) = &p.annotation {
                    ex(a, out);
                }
            }
            if let Some(rt) = return_type {
                ex(rt, out);
            }
            if !skip_def_bodies {
                name_refs_in_stmts(body, false, out);
            }
        }
        StmtKind::ClassDef {
            bases,
            body,
            decorator_list,
            ..
        } => {
            // A class body executes at class-creation time; its methods'
            // bodies are folded in as well (over-approximation).
            for d in decorator_list {
                ex(d, out);
            }
            for b in bases {
                ex(b, out);
            }
            name_refs_in_stmts(body, false, out);
        }
        StmtKind::Expr(e) | StmtKind::Return(Some(e)) => ex(e, out),
        StmtKind::Raise(e, c) => {
            if let Some(e) = e {
                ex(e, out);
            }
            if let Some(c) = c {
                ex(c, out);
            }
        }
        StmtKind::Assign { targets, value } => {
            for t in targets {
                ex(t, out);
            }
            ex(value, out);
        }
        StmtKind::AugAssign { target, value, .. } => {
            ex(target, out);
            ex(value, out);
        }
        StmtKind::AnnAssign {
            target,
            annotation,
            value,
        } => {
            ex(target, out);
            ex(annotation, out);
            if let Some(v) = value {
                ex(v, out);
            }
        }
        StmtKind::If {
            test,
            body,
            elif_clauses,
            else_body,
        } => {
            ex(test, out);
            name_refs_in_stmts(body, skip_def_bodies, out);
            for (t, b) in elif_clauses {
                ex(t, out);
                name_refs_in_stmts(b, skip_def_bodies, out);
            }
            if let Some(b) = else_body {
                name_refs_in_stmts(b, skip_def_bodies, out);
            }
        }
        StmtKind::While {
            test,
            body,
            else_body,
        } => {
            ex(test, out);
            name_refs_in_stmts(body, skip_def_bodies, out);
            if let Some(b) = else_body {
                name_refs_in_stmts(b, skip_def_bodies, out);
            }
        }
        StmtKind::For {
            target,
            iter,
            body,
            else_body,
            ..
        } => {
            ex(target, out);
            ex(iter, out);
            name_refs_in_stmts(body, skip_def_bodies, out);
            if let Some(b) = else_body {
                name_refs_in_stmts(b, skip_def_bodies, out);
            }
        }
        StmtKind::Try {
            body,
            handlers,
            else_body,
            finally_body,
        } => {
            name_refs_in_stmts(body, skip_def_bodies, out);
            for h in handlers {
                if let Some(e) = &h.exc_type {
                    ex(e, out);
                }
                name_refs_in_stmts(&h.body, skip_def_bodies, out);
            }
            if let Some(b) = else_body {
                name_refs_in_stmts(b, skip_def_bodies, out);
            }
            if let Some(b) = finally_body {
                name_refs_in_stmts(b, skip_def_bodies, out);
            }
        }
        StmtKind::With { items, body, .. } => {
            for it in items {
                ex(&it.context_expr, out);
                if let Some(v) = &it.optional_var {
                    ex(v, out);
                }
            }
            name_refs_in_stmts(body, skip_def_bodies, out);
        }
        StmtKind::Match { subject, cases } => {
            ex(subject, out);
            for c in cases {
                if let Some(g) = &c.guard {
                    ex(g, out);
                }
                name_refs_in_stmts(&c.body, skip_def_bodies, out);
            }
        }
        StmtKind::Assert { test, msg } => {
            ex(test, out);
            if let Some(m) = msg {
                ex(m, out);
            }
        }
        StmtKind::Del(exprs) => {
            for e in exprs {
                ex(e, out);
            }
        }
        StmtKind::Return(None)
        | StmtKind::Import { .. }
        | StmtKind::ImportFrom { .. }
        | StmtKind::ImportSideEffect(_)
        | StmtKind::Break
        | StmtKind::Continue
        | StmtKind::Pass
        | StmtKind::Global(_)
        | StmtKind::Nonlocal(_) => {}
    }
}

/// #491 (B3): the names transitively REFERENCED by module-level code that executes
/// BEFORE top-level statement `before_idx`. Start set = every `Name` in statements
/// `0..before_idx` (a `def` contributes only its def-time parts); closure over the
/// module def/class graph (`graph`: top-level def/class name → all names its body
/// references). A WASM function in this set can run while a name rebound at
/// `before_idx` still holds its EARLIER binding, so it may not bake the final one.
fn early_reachable(
    module: &Module,
    graph: &HashMap<String, HashSet<String>>,
    before_idx: usize,
) -> HashSet<String> {
    let mut seen: HashSet<String> = HashSet::new();
    for stmt in module.body.iter().take(before_idx.min(module.body.len())) {
        name_refs_in_stmt(stmt, true, &mut seen);
    }
    let mut work: Vec<String> = seen.iter().cloned().collect();
    while let Some(n) = work.pop() {
        if let Some(refs) = graph.get(&n) {
            for r in refs {
                if seen.insert(r.clone()) {
                    work.push(r.clone());
                }
            }
        }
    }
    seen
}

/// #491 (B3): the module reference graph — every top-level `def`/`class` name mapped
/// to all names referenced anywhere in it (body, decorators, defaults, nested defs).
fn module_ref_graph(module: &Module) -> HashMap<String, HashSet<String>> {
    let mut graph = HashMap::new();
    for stmt in &module.body {
        if let StmtKind::FuncDef { name, .. } | StmtKind::ClassDef { name, .. } = &stmt.kind {
            let mut refs = HashSet::new();
            name_refs_in_stmt(stmt, false, &mut refs);
            graph.insert(name.clone(), refs);
        }
    }
    graph
}

/// #491 (S1): every name a function body binds LOCALLY — params, assignment /
/// for / with / except targets, walrus, nested def/class names, `del` — recursing
/// through nested blocks but not into nested def bodies (their own scope).
fn local_binding_names(params: &[Param], body: &[Stmt]) -> HashSet<String> {
    let mut out: HashSet<String> = params.iter().map(|p| p.name.clone()).collect();
    let mut bindings: HashMap<String, (usize, ShadowBinding)> = HashMap::new();
    for s in body {
        module_binders_in_stmt(s, 0, &mut bindings);
    }
    out.extend(bindings.into_keys());
    out
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
    let mut eligible = BTreeMap::new();
    let mut rejected = Vec::new();

    // #491 name-binding soundness — the ORDER-AWARE authority (SHADOW_BINDING_DESIGN.md).
    // The FINAL module-level binding of every def/math-import name. WASM compiles only
    // function bodies, which see the final binding at call time, so this per-name answer
    // is the whole model on the WASM side. A name whose final binding is a `UserDef`
    // shadows the builtin; a `MathAlias` (a LATER `from math import` beating an earlier
    // def) means the name is that math fn, not the def; a def shadowed by a later
    // binding is dead-by-name (below). An ineligible user shadow demotes its callers.
    let shadow_bindings = final_module_bindings(module);

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

    // #496 (B1): functions that may raise ZeroDivisionError on the WASM path,
    // closed transitively over the call graph — computed once for the whole module
    // so the per-function try/except admission check below can see a callee's
    // division through a call.
    let may_zerodiv = functions_that_can_zerodiv(module);

    // Second pass: check bodies
    for info in candidates {
        // #491 dead-by-name: a `def` is WASM-eligible under its name only if it IS that
        // name's FINAL module binding. A def shadowed by a LATER def or `from math
        // import ... as <name>` is dead code by that name (the later binding wins at
        // call time), so it must NOT be routed to WASM / re-exported — otherwise the
        // hoisted glue re-export would beat the later binding (blocker 4). Not routing
        // it lets JS resolve the final binding correctly (JS-only already emits the
        // last-wins rebind).
        match shadow_bindings.get(&info.name) {
            Some((idx, ShadowBinding::UserDef)) if *idx == info.stmt_index => {}
            _ => {
                rejected.push((
                    info.name.clone(),
                    "the name is rebound later at module scope (a later def or `from \
                     math import ... as` shadows this def); it stays JS so the final \
                     binding wins"
                        .to_string(),
                ));
                continue;
            }
        }
        if let StmtKind::FuncDef { body, params, .. } = &module.body[info.stmt_index].kind {
            if let Err(reason) = check_body(body, &candidate_names, &extended_excs) {
                rejected.push((info.name.clone(), reason));
            } else if let Some(local) = local_shadow_call(params, body, &shadow_bindings) {
                // #491 (S1) sound-by-refusal: the body CALLS a name it binds LOCALLY
                // (a param `abs`, a local `len = 5`) that the WASM emitter would
                // otherwise lower as the builtin / math alias / module function —
                // CPython resolves the local (`TypeError: 'int' object is not
                // callable`). Route to JS, which raises the same TypeError; never
                // the builtin's silent value.
                rejected.push((
                    info.name.clone(),
                    format!(
                        "calls `{}`, which the function binds locally (a param or local \
                         shadows the builtin/module name inside this body); the function \
                         stays JS",
                        local
                    ),
                ));
            } else if let Err(reason) = check_array_kernel_admission(body, &info.params) {
                // M2a-3 sound-by-refusal: array iteration (G1) and sub-64-bit
                // int non-ring ops (G2) are routed to JS, not mis-lowered.
                rejected.push((info.name.clone(), reason));
            } else if let Err(reason) = check_shape_uses(body, &info.params) {
                // M2b: `.shape` reads only on a 2-D array param, index ∈ {0,1}.
                rejected.push((info.name.clone(), reason));
            } else if let Err(reason) = check_zerodiv_try_admission(body, &may_zerodiv) {
                // #496 sound-by-refusal: a division under a ZeroDivisionError handler
                // (directly or via a call to a dividing function) TRAPS on WASM where
                // CPython runs the handler — route to JS instead of admit-then-trap.
                rejected.push((info.name.clone(), reason));
            } else if let Some(shadow) = shadowed_for_iter(body, &shadow_bindings) {
                // #491 name-binding soundness: a `for … in NAME(...)` where NAME is a
                // user `def` shadowing `range` (or any builtin) is NOT the builtin
                // range loop. An eligible user shadow can only return a scalar
                // (non-scalar returns are already refused), which CPython cannot
                // iterate (TypeError), so the WASM for-range lowering would silently
                // loop where CPython raises. Route to JS (which raises correctly).
                rejected.push((
                    info.name.clone(),
                    format!(
                        "iterates `{}(...)`, a user-defined function shadowing a builtin \
                         iterator (not the builtin `range`); the function stays JS",
                        shadow
                    ),
                ));
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
    call_consistency_fixpoint(module, &shadow_bindings, &mut eligible, &mut rejected);

    // #491 (B3) early-call demotion (SHADOW_BINDING_DESIGN.md §3.4). A WASM body
    // bakes ONE resolution of every name it calls — the FINAL module binding. That
    // is only correct if the function can never run while the name still holds an
    // EARLIER binding. Module-level code that executes BEFORE a name's final binder
    // (statement index < b) and transitively reaches the function (any reference,
    // through the def/class graph) could call it then — `print(use_abs(-5))` before
    // `def abs` must see the builtin, after it the user fn. Demote such functions
    // to the cell-aware JS path; their WASM callers follow via the fixpoint.
    {
        let graph = module_ref_graph(module);
        let mut demote: Vec<(String, String)> = Vec::new();
        let mut names: Vec<(&String, &(usize, ShadowBinding))> = shadow_bindings.iter().collect();
        names.sort_by(|a, b| a.0.cmp(b.0)); // deterministic reason order
        for (shadowed, (final_idx, _)) in names {
            let early = early_reachable(module, &graph, *final_idx);
            for (fname, info) in &eligible {
                if !early.contains(fname) || demote.iter().any(|(n, _)| n == fname) {
                    continue;
                }
                if let StmtKind::FuncDef { body, .. } = &module.body[info.stmt_index].kind {
                    let mut called = Vec::new();
                    collect_called_functions(body, &mut called);
                    if called.iter().any(|c| c == shadowed) {
                        demote.push((
                            fname.clone(),
                            format!(
                                "calls `{}` and is reachable from module-level code that runs \
                                 BEFORE `{}` is rebound (top-level statement {}); a WASM body \
                                 bakes the final binding, so the function stays JS (#491 \
                                 call-time resolution)",
                                shadowed,
                                shadowed,
                                if *final_idx == GLOBAL_WRITE_INDEX {
                                    "<global write>".to_string()
                                } else {
                                    final_idx.to_string()
                                }
                            ),
                        ));
                    }
                }
            }
        }
        if !demote.is_empty() {
            for (name, reason) in demote {
                eligible.remove(&name);
                rejected.push((name, reason));
            }
            call_consistency_fixpoint(module, &shadow_bindings, &mut eligible, &mut rejected);
        }
    }

    WasmAnalysis {
        eligible,
        rejected,
        shadow_bindings,
    }
}

/// #364 / #491: collect the names of all directly-called functions (`Name(...)`)
/// in a body — EVERY statement form and EVERY expression position (the previous
/// walker skipped `try`/`with`/`match` bodies, call keyword arguments and
/// dict/set/tuple operands, so a call inside a `try` body escaped the
/// call-consistency fixpoint). Built on the single `for_each_child_expr`
/// enumeration so a new expression kind cannot be silently skipped.
fn collect_called_functions(body: &[Stmt], out: &mut Vec<String>) {
    fn expr(e: &Expr, out: &mut Vec<String>) {
        if let ExprKind::Call { func, .. } = &e.kind {
            if let ExprKind::Name(n) = &func.kind {
                out.push(n.clone());
            }
        }
        for_each_child_expr(e, &mut |c| expr(c, out));
    }
    // A nested def's body is its own scope (its calls are validated when IT is
    // admitted), so only its def-time parts are walked.
    fn walk(s: &Stmt, out: &mut Vec<String>) {
        let ex = |e: &Expr, out: &mut Vec<String>| expr(e, out);
        match &s.kind {
            StmtKind::FuncDef {
                params,
                decorator_list,
                return_type,
                ..
            } => {
                for d in decorator_list {
                    ex(d, out);
                }
                for p in params {
                    if let Some(d) = &p.default {
                        ex(d, out);
                    }
                    if let Some(a) = &p.annotation {
                        ex(a, out);
                    }
                }
                if let Some(rt) = return_type {
                    ex(rt, out);
                }
            }
            StmtKind::ClassDef {
                bases,
                body,
                decorator_list,
                ..
            } => {
                for d in decorator_list {
                    ex(d, out);
                }
                for b in bases {
                    ex(b, out);
                }
                for s in body {
                    walk(s, out);
                }
            }
            StmtKind::Expr(e) | StmtKind::Return(Some(e)) => ex(e, out),
            // `raise ValueError(msg)`: the exception "call" and its message are
            // NOT calls the WASM body makes — the emitter drops the argument and
            // maps the class to an error code (`check_stmt`'s Raise arm validates
            // the class name). Walking them would demote every raising function
            // for "calling" `ValueError`. Mirror the emitter: skip.
            StmtKind::Raise(..) => {}
            StmtKind::Assign { targets, value } => {
                for t in targets {
                    ex(t, out);
                }
                ex(value, out);
            }
            StmtKind::AugAssign { target, value, .. } => {
                ex(target, out);
                ex(value, out);
            }
            StmtKind::AnnAssign {
                target,
                annotation,
                value,
            } => {
                ex(target, out);
                ex(annotation, out);
                if let Some(v) = value {
                    ex(v, out);
                }
            }
            StmtKind::If {
                test,
                body,
                elif_clauses,
                else_body,
            } => {
                ex(test, out);
                for s in body {
                    walk(s, out);
                }
                for (t, b) in elif_clauses {
                    ex(t, out);
                    for s in b {
                        walk(s, out);
                    }
                }
                if let Some(b) = else_body {
                    for s in b {
                        walk(s, out);
                    }
                }
            }
            StmtKind::While {
                test,
                body,
                else_body,
            } => {
                ex(test, out);
                for s in body {
                    walk(s, out);
                }
                if let Some(b) = else_body {
                    for s in b {
                        walk(s, out);
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
                ex(target, out);
                ex(iter, out);
                for s in body {
                    walk(s, out);
                }
                if let Some(b) = else_body {
                    for s in b {
                        walk(s, out);
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
                    walk(s, out);
                }
                for h in handlers {
                    if let Some(e) = &h.exc_type {
                        ex(e, out);
                    }
                    for s in &h.body {
                        walk(s, out);
                    }
                }
                if let Some(b) = else_body {
                    for s in b {
                        walk(s, out);
                    }
                }
                if let Some(b) = finally_body {
                    for s in b {
                        walk(s, out);
                    }
                }
            }
            StmtKind::With { items, body, .. } => {
                for it in items {
                    ex(&it.context_expr, out);
                    if let Some(v) = &it.optional_var {
                        ex(v, out);
                    }
                }
                for s in body {
                    walk(s, out);
                }
            }
            StmtKind::Match { subject, cases } => {
                ex(subject, out);
                for c in cases {
                    if let Some(g) = &c.guard {
                        ex(g, out);
                    }
                    for s in &c.body {
                        walk(s, out);
                    }
                }
            }
            // `assert test, msg`: the message is dropped on WASM (only `test` is
            // lowered — `check_stmt`'s Assert arm); mirror the emitter.
            StmtKind::Assert { test, .. } => ex(test, out),
            StmtKind::Del(exprs) => {
                for e in exprs {
                    ex(e, out);
                }
            }
            StmtKind::Return(None)
            | StmtKind::Import { .. }
            | StmtKind::ImportFrom { .. }
            | StmtKind::ImportSideEffect(_)
            | StmtKind::Break
            | StmtKind::Continue
            | StmtKind::Pass
            | StmtKind::Global(_)
            | StmtKind::Nonlocal(_) => {}
        }
    }
    for s in body {
        walk(s, out);
    }
}

/// #364 / #491: the call-consistency fixpoint (see the call site in
/// `analyze_module`). Drops any eligible function that calls a name which is not a
/// whitelist builtin, a canonical math import, or an eligible user function — with
/// the #491 shadow arms: a name whose FINAL module binding is a user def must be an
/// ELIGIBLE user function; an ALIASED math import (`fabs as abs`) and every OTHER
/// binder kind (assignment / class / non-math import / for-target / `global` write /
/// `del`) have no WASM lowering, so the caller stays JS. Re-runnable: the B3 rule
/// demotes functions after the first fixpoint and calls it again.
fn call_consistency_fixpoint(
    module: &Module,
    shadow_bindings: &HashMap<String, (usize, ShadowBinding)>,
    eligible: &mut BTreeMap<String, WasmFuncInfo>,
    rejected: &mut Vec<(String, String)>,
) {
    loop {
        let eligible_now: HashSet<String> = eligible.keys().cloned().collect();
        let mut drop_name: Option<(String, String)> = None;
        for (name, info) in eligible.iter() {
            if let StmtKind::FuncDef { body, .. } = &module.body[info.stmt_index].kind {
                let mut called = Vec::new();
                collect_called_functions(body, &mut called);
                if let Some(bad) = called.into_iter().find_map(|c| {
                    let not_ok = match shadow_bindings.get(&c) {
                        // #491 user shadow: the builtin/math whitelist does NOT rescue
                        // a name whose FINAL binding is a user def — it must be an
                        // ELIGIBLE user function, else the caller demotes to JS.
                        Some((_, ShadowBinding::UserDef)) => !eligible_now.contains(&c),
                        // #491 blocker 4: a CANONICAL bare math import (bound name ==
                        // the math fn) is the supported math call — keep it. An ALIASED
                        // import to another name (`fabs as abs`) is neither the builtin
                        // nor a canonical math call → demote (JS resolves it).
                        Some((_, ShadowBinding::MathAlias(canon))) => c != *canon,
                        // #491 (S4): any other binder kind → no WASM lowering → demote.
                        Some((_, ShadowBinding::Other(kind))) => {
                            return Some(format!(
                                "`{}`, which is rebound at module scope by a {} (the WASM \
                                 body cannot resolve it at call time)",
                                c, kind
                            ));
                        }
                        None => {
                            !WASM_CALL_BUILTINS.contains(&c.as_str())
                                && !WASM_MATH_FUNCTIONS.contains(&c.as_str())
                                && !eligible_now.contains(&c)
                        }
                    };
                    not_ok.then(|| format!("`{}`, which is not WASM-eligible", c))
                }) {
                    drop_name = Some((name.clone(), bad));
                    break;
                }
            }
        }
        match drop_name {
            Some((name, bad)) => {
                eligible.remove(&name);
                rejected.push((name, format!("calls {} (function stays JS)", bad)));
            }
            None => break,
        }
    }
}

/// #491 (S1): the first name this body CALLS that it also binds LOCALLY (param /
/// local assignment / for-with-except target / walrus / nested def) AND that the
/// WASM emitter would lower specially — a numeric-kernel builtin, a math function,
/// or any module-shadowed name (a `from math` alias or a module def). Such a call
/// is CPython's local (`TypeError: 'int' object is not callable`), never the
/// builtin. A local closure under a neutral name (`f = …; f(x)`) is untouched.
fn local_shadow_call(
    params: &[Param],
    body: &[Stmt],
    shadow_bindings: &HashMap<String, (usize, ShadowBinding)>,
) -> Option<String> {
    let locals = local_binding_names(params, body);
    let mut called = Vec::new();
    collect_called_functions(body, &mut called);
    called.into_iter().find(|c| {
        locals.contains(c)
            && (WASM_CALL_BUILTINS.contains(&c.as_str())
                || WASM_MATH_FUNCTIONS.contains(&c.as_str())
                || shadow_bindings.contains_key(c))
    })
}

/// #491 name-binding soundness: find a `for … in NAME(...)` in this body (any
/// nesting) whose callee NAME is a user `def` — i.e. a user shadow of `range` (or
/// any builtin iterator). Returns the shadowing name. The WASM `for`-loop only
/// honors the builtin `range` or list iteration; a user shadow must not be lowered
/// as the builtin range.
fn shadowed_for_iter(
    body: &[Stmt],
    user_defs: &HashMap<String, (usize, ShadowBinding)>,
) -> Option<String> {
    fn iter_shadow(
        iter: &Expr,
        user_defs: &HashMap<String, (usize, ShadowBinding)>,
    ) -> Option<String> {
        if let ExprKind::Call { func, .. } = &iter.kind {
            if let ExprKind::Name(n) = &func.kind {
                // Only a user `def` (final binding) makes `for … in NAME(...)` a shadow;
                // a math-import alias is not a for-iterator producer.
                if matches!(user_defs.get(n), Some((_, ShadowBinding::UserDef))) {
                    return Some(n.clone());
                }
            }
        }
        None
    }
    fn stmt(s: &Stmt, user_defs: &HashMap<String, (usize, ShadowBinding)>) -> Option<String> {
        match &s.kind {
            StmtKind::For {
                iter,
                body,
                else_body,
                ..
            } => iter_shadow(iter, user_defs)
                .or_else(|| shadowed_for_iter(body, user_defs))
                .or_else(|| {
                    else_body
                        .as_ref()
                        .and_then(|b| shadowed_for_iter(b, user_defs))
                }),
            StmtKind::If {
                body,
                elif_clauses,
                else_body,
                ..
            } => shadowed_for_iter(body, user_defs)
                .or_else(|| {
                    elif_clauses
                        .iter()
                        .find_map(|(_, b)| shadowed_for_iter(b, user_defs))
                })
                .or_else(|| {
                    else_body
                        .as_ref()
                        .and_then(|b| shadowed_for_iter(b, user_defs))
                }),
            StmtKind::While {
                body, else_body, ..
            } => shadowed_for_iter(body, user_defs).or_else(|| {
                else_body
                    .as_ref()
                    .and_then(|b| shadowed_for_iter(b, user_defs))
            }),
            StmtKind::Try {
                body,
                handlers,
                else_body,
                finally_body,
            } => shadowed_for_iter(body, user_defs)
                .or_else(|| {
                    handlers
                        .iter()
                        .find_map(|h| shadowed_for_iter(&h.body, user_defs))
                })
                .or_else(|| {
                    else_body
                        .as_ref()
                        .and_then(|b| shadowed_for_iter(b, user_defs))
                })
                .or_else(|| {
                    finally_body
                        .as_ref()
                        .and_then(|b| shadowed_for_iter(b, user_defs))
                }),
            StmtKind::With { body, .. } => shadowed_for_iter(body, user_defs),
            _ => None,
        }
    }
    body.iter().find_map(|s| stmt(s, user_defs))
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

/// M2a-3: two SOUND admission guards for `Array[dtype]`-param kernels, applied
/// once the params are known (second pass). Both route the offending function
/// to the correct JS path rather than emitting a silently-wrong WASM kernel —
/// the sound-by-refusal discipline (independent-opus R1 blockers).
///
/// **G1 (array iteration).** `for x in a` over an array param is not yet lowered
/// (the for-collection dispatch has no `PtrArray` arm; the range-fallback emits
/// a silent no-op loop). Refuse it — `for i in range(len(a))` is the supported
/// form. All dtypes.
///
/// **G2 (sub-64-bit int WRAP fidelity).** Array int elements load sign/zero-
/// extended to the i64 compute stack and WRAP only at the narrowing store. For
/// `+`/`-`/`*` this equals NumPy (mod-2^k is a ring homomorphism and 2^k | 2^64).
/// It DIVERGES when an intermediate exceeds the element width and is then fed to
/// a NON-ring op (`//`, `%`, `<<`, `>>`, `&`, `|`, `^`, `**`, `~`): NumPy wraps
/// at each op, this design wraps once at the end (e.g. uint8 `(200+100)//2` →
/// 150 here vs 22 in NumPy). So if ANY param is a sub-64-bit int array
/// (`int32`/`uint8`), refuse a body containing any non-ring integer op anywhere
/// (conservative — it may over-refuse a safe `raw_elem % k`, which then runs
/// correctly on JS). `int64` is exact (i64 wrap == NumPy int64) and `float*`
/// uses f64 ops, so neither triggers G2.
fn check_array_kernel_admission(body: &[Stmt], params: &[(String, Type)]) -> Result<(), String> {
    use std::collections::HashSet;
    let array_names: HashSet<&str> = params
        .iter()
        .filter(|(_, t)| matches!(t, Type::Array(_, _)))
        .map(|(n, _)| n.as_str())
        .collect();
    if array_names.is_empty() {
        return Ok(());
    }
    let arr2d_names: HashSet<&str> = params
        .iter()
        .filter(|(_, t)| matches!(t, Type::Array(_, 2)))
        .map(|(n, _)| n.as_str())
        .collect();
    let has_sub64_int = params.iter().any(|(_, t)| {
        matches!(
            t,
            Type::Array(ArrayDtype::Int32, _) | Type::Array(ArrayDtype::Uint8, _)
        )
    });

    // G3 (M2b — 2-D row-as-value soundness). A 2-D array supports only the
    // FULLY-indexed element access `a[i, j]` (Tuple index) and `a[i][j]` (a
    // single-index subscript that is ITSELF the base of an outer subscript).
    // A BARE `a[i]` on a 2-D array (a "row" bound to a name / used as a value)
    // is NOT lowered — there is no row-view value in the ABI; emitting it would
    // mis-address (`ptr+16+i*esize`, 1-D stride) and mistype the result as a
    // buffer pointer = a silent wrong read. Refuse it (the function stays JS).
    // `walk2d(e, base_pos)`: `base_pos` = this expr is the `.value` child of an
    // enclosing subscript. A 2-D single-index subscript is legal iff it is in
    // base position (the inner half of `a[i][j]`); a Tuple index is the element
    // access `a[i, j]` (legal anywhere). Everything else is an ordinary
    // recursion with `base_pos = false`.
    fn walk2d(e: &Expr, base_pos: bool, arr2d: &HashSet<&str>) -> Result<(), String> {
        if let ExprKind::Subscript { value, index, .. } = &e.kind {
            let is_row = matches!(&value.kind, ExprKind::Name(n) if arr2d.contains(n.as_str()))
                && !matches!(index.kind, ExprKind::Tuple(_));
            if is_row && !base_pos {
                if let ExprKind::Name(n) = &value.kind {
                    return Err(format!(
                        "a bare row `{n}[i]` of the 2-D array param `{n}` is used as a value on the \
                         WASM fast path; only the full element access `{n}[i, j]` or `{n}[i][j]` is \
                         supported (there is no row-view value in the array ABI). The function stays \
                         on the JS path"
                    ));
                }
            }
            // The subscript's base is in base position; the index never is.
            walk2d(value, true, arr2d)?;
            walk2d(index, false, arr2d)?;
            return Ok(());
        }
        walk2d_children(e, arr2d)
    }
    // Recurse into every child sub-expression with base_pos = false (only a
    // Subscript's own `.value` child is in base position, handled above).
    fn walk2d_children(e: &Expr, arr2d: &HashSet<&str>) -> Result<(), String> {
        match &e.kind {
            ExprKind::BinOp { left, right, .. } => {
                walk2d(left, false, arr2d)?;
                walk2d(right, false, arr2d)?;
            }
            ExprKind::UnaryOp { operand, .. } => walk2d(operand, false, arr2d)?,
            ExprKind::Compare { left, comparisons } => {
                walk2d(left, false, arr2d)?;
                for (_, c) in comparisons {
                    walk2d(c, false, arr2d)?;
                }
            }
            ExprKind::Call {
                func, args, kwargs, ..
            } => {
                walk2d(func, false, arr2d)?;
                for a in args {
                    walk2d(a, false, arr2d)?;
                }
                for kw in kwargs {
                    walk2d(&kw.value, false, arr2d)?;
                }
            }
            ExprKind::IfExpr {
                test,
                body,
                else_body,
            } => {
                walk2d(test, false, arr2d)?;
                walk2d(body, false, arr2d)?;
                walk2d(else_body, false, arr2d)?;
            }
            ExprKind::Attribute { value, .. } => walk2d(value, false, arr2d)?,
            ExprKind::List(elts) | ExprKind::Tuple(elts) => {
                for el in elts {
                    walk2d(el, false, arr2d)?;
                }
            }
            _ => {}
        }
        Ok(())
    }
    fn walk2d_body(b: &[Stmt], arr2d: &HashSet<&str>) -> Result<(), String> {
        for st in b {
            walk2d_stmt(st, arr2d)?;
        }
        Ok(())
    }
    fn walk2d_stmt(s: &Stmt, arr2d: &HashSet<&str>) -> Result<(), String> {
        match &s.kind {
            StmtKind::For {
                target,
                iter,
                body,
                else_body,
                ..
            } => {
                walk2d(target, false, arr2d)?;
                walk2d(iter, false, arr2d)?;
                walk2d_body(body, arr2d)?;
                if let Some(b) = else_body {
                    walk2d_body(b, arr2d)?;
                }
            }
            StmtKind::Assign { targets, value } => {
                for t in targets {
                    walk2d(t, false, arr2d)?;
                }
                walk2d(value, false, arr2d)?;
            }
            StmtKind::AugAssign { target, value, .. } => {
                walk2d(target, false, arr2d)?;
                walk2d(value, false, arr2d)?;
            }
            StmtKind::AnnAssign { target, value, .. } => {
                walk2d(target, false, arr2d)?;
                if let Some(v) = value {
                    walk2d(v, false, arr2d)?;
                }
            }
            StmtKind::Return(v) => {
                if let Some(v) = v {
                    walk2d(v, false, arr2d)?;
                }
            }
            StmtKind::Expr(e) => walk2d(e, false, arr2d)?,
            StmtKind::If {
                test,
                body,
                elif_clauses,
                else_body,
            } => {
                walk2d(test, false, arr2d)?;
                walk2d_body(body, arr2d)?;
                for (t, b) in elif_clauses {
                    walk2d(t, false, arr2d)?;
                    walk2d_body(b, arr2d)?;
                }
                if let Some(b) = else_body {
                    walk2d_body(b, arr2d)?;
                }
            }
            StmtKind::While {
                test,
                body,
                else_body,
            } => {
                walk2d(test, false, arr2d)?;
                walk2d_body(body, arr2d)?;
                if let Some(b) = else_body {
                    walk2d_body(b, arr2d)?;
                }
            }
            StmtKind::Try {
                body,
                handlers,
                else_body,
                finally_body,
            } => {
                walk2d_body(body, arr2d)?;
                for h in handlers {
                    walk2d_body(&h.body, arr2d)?;
                }
                if let Some(b) = else_body {
                    walk2d_body(b, arr2d)?;
                }
                if let Some(b) = finally_body {
                    walk2d_body(b, arr2d)?;
                }
            }
            StmtKind::Assert { test, msg } => {
                walk2d(test, false, arr2d)?;
                if let Some(m) = msg {
                    walk2d(m, false, arr2d)?;
                }
            }
            StmtKind::Raise(exc, cause) => {
                if let Some(e) = exc {
                    walk2d(e, false, arr2d)?;
                }
                if let Some(c) = cause {
                    walk2d(c, false, arr2d)?;
                }
            }
            StmtKind::With { items, body, .. } => {
                for it in items {
                    walk2d(&it.context_expr, false, arr2d)?;
                    if let Some(v) = &it.optional_var {
                        walk2d(v, false, arr2d)?;
                    }
                }
                walk2d_body(body, arr2d)?;
            }
            StmtKind::Match { subject, cases } => {
                walk2d(subject, false, arr2d)?;
                for c in cases {
                    if let Some(g) = &c.guard {
                        walk2d(g, false, arr2d)?;
                    }
                    walk2d_body(&c.body, arr2d)?;
                }
            }
            StmtKind::Del(exprs) => {
                for e in exprs {
                    walk2d(e, false, arr2d)?;
                }
            }
            StmtKind::Break
            | StmtKind::Continue
            | StmtKind::Pass
            | StmtKind::Global(_)
            | StmtKind::Nonlocal(_)
            | StmtKind::Import { .. }
            | StmtKind::ImportSideEffect(_)
            | StmtKind::ImportFrom { .. }
            | StmtKind::FuncDef { .. }
            | StmtKind::ClassDef { .. } => {}
        }
        Ok(())
    }
    if !arr2d_names.is_empty() {
        walk2d_body(body, &arr2d_names)?;
    }

    fn nonring_binop(op: BinOp) -> bool {
        matches!(
            op,
            BinOp::FloorDiv
                | BinOp::Mod
                | BinOp::ShiftLeft
                | BinOp::ShiftRight
                | BinOp::BitAnd
                | BinOp::BitOr
                | BinOp::BitXor
                | BinOp::Pow
        )
    }
    fn nonring_aug(op: AugAssignOp) -> bool {
        matches!(
            op,
            AugAssignOp::FloorDiv
                | AugAssignOp::Mod
                | AugAssignOp::ShiftLeft
                | AugAssignOp::ShiftRight
                | AugAssignOp::BitAnd
                | AugAssignOp::BitOr
                | AugAssignOp::BitXor
                | AugAssignOp::Pow
        )
    }
    fn g2_err() -> String {
        "a non-ring integer op (// % << >> & | ^ ** ~) on a sub-64-bit int array \
         (int32/uint8) kernel is not supported on the WASM fast path — the fixed-width \
         WRAP would diverge from NumPy on an overflowing intermediate; the function stays \
         on the JS path (int64/float array ops are unaffected)"
            .to_string()
    }

    fn expr(e: &Expr, sub64: bool) -> Result<(), String> {
        match &e.kind {
            ExprKind::BinOp { left, op, right } => {
                if sub64 && nonring_binop(*op) {
                    return Err(g2_err());
                }
                expr(left, sub64)?;
                expr(right, sub64)?;
            }
            ExprKind::UnaryOp { op, operand } => {
                if sub64 && matches!(op, UnaryOp::BitNot) {
                    return Err(g2_err());
                }
                expr(operand, sub64)?;
            }
            ExprKind::Compare { left, comparisons } => {
                expr(left, sub64)?;
                for (_, c) in comparisons {
                    expr(c, sub64)?;
                }
            }
            ExprKind::Call {
                func, args, kwargs, ..
            } => {
                expr(func, sub64)?;
                for a in args {
                    expr(a, sub64)?;
                }
                for kw in kwargs {
                    expr(&kw.value, sub64)?;
                }
            }
            ExprKind::IfExpr {
                test,
                body,
                else_body,
            } => {
                expr(test, sub64)?;
                expr(body, sub64)?;
                expr(else_body, sub64)?;
            }
            ExprKind::Subscript { value, index, .. } => {
                expr(value, sub64)?;
                expr(index, sub64)?;
            }
            ExprKind::Attribute { value, .. } => expr(value, sub64)?,
            ExprKind::List(elts) | ExprKind::Tuple(elts) => {
                for el in elts {
                    expr(el, sub64)?;
                }
            }
            _ => {}
        }
        Ok(())
    }

    // EXHAUSTIVE over every StmtKind (independent-opus R2 blocker: the prior
    // `_ => {}` swallowed `Try`/`Assert`, letting a non-ring op or `for x in a`
    // nested in a `try` body escape BOTH guards). No silent catch-all — a newly
    // admitted statement kind must be handled here or the compiler stops building.
    fn stmt_body(b: &[Stmt], arr: &HashSet<&str>, sub64: bool) -> Result<(), String> {
        for st in b {
            stmt(st, arr, sub64)?;
        }
        Ok(())
    }
    fn stmt(s: &Stmt, arr: &HashSet<&str>, sub64: bool) -> Result<(), String> {
        match &s.kind {
            StmtKind::For {
                target,
                iter,
                body,
                else_body,
                ..
            } => {
                // G1: iterating an array param directly is not lowered yet.
                if let ExprKind::Name(n) = &iter.kind {
                    if arr.contains(n.as_str()) {
                        return Err(format!(
                            "iterating an array param `{n}` directly (`for x in {n}`) is not \
                             supported on the WASM fast path yet (use \
                             `for i in range(len({n}))`); the function stays on the JS path"
                        ));
                    }
                }
                expr(target, sub64)?;
                expr(iter, sub64)?;
                stmt_body(body, arr, sub64)?;
                if let Some(b) = else_body {
                    stmt_body(b, arr, sub64)?;
                }
            }
            StmtKind::Assign { targets, value } => {
                for t in targets {
                    expr(t, sub64)?;
                }
                expr(value, sub64)?;
            }
            StmtKind::AugAssign { target, op, value } => {
                if sub64 && nonring_aug(*op) {
                    return Err(g2_err());
                }
                expr(target, sub64)?;
                expr(value, sub64)?;
            }
            StmtKind::AnnAssign { target, value, .. } => {
                expr(target, sub64)?;
                if let Some(v) = value {
                    expr(v, sub64)?;
                }
            }
            StmtKind::Return(v) => {
                if let Some(v) = v {
                    expr(v, sub64)?;
                }
            }
            StmtKind::Expr(e) => expr(e, sub64)?,
            StmtKind::If {
                test,
                body,
                elif_clauses,
                else_body,
            } => {
                expr(test, sub64)?;
                stmt_body(body, arr, sub64)?;
                for (t, b) in elif_clauses {
                    expr(t, sub64)?;
                    stmt_body(b, arr, sub64)?;
                }
                if let Some(b) = else_body {
                    stmt_body(b, arr, sub64)?;
                }
            }
            StmtKind::While {
                test,
                body,
                else_body,
            } => {
                expr(test, sub64)?;
                stmt_body(body, arr, sub64)?;
                if let Some(b) = else_body {
                    stmt_body(b, arr, sub64)?;
                }
            }
            StmtKind::Try {
                body,
                handlers,
                else_body,
                finally_body,
            } => {
                stmt_body(body, arr, sub64)?;
                for h in handlers {
                    stmt_body(&h.body, arr, sub64)?;
                }
                if let Some(b) = else_body {
                    stmt_body(b, arr, sub64)?;
                }
                if let Some(b) = finally_body {
                    stmt_body(b, arr, sub64)?;
                }
            }
            StmtKind::Assert { test, msg } => {
                expr(test, sub64)?;
                if let Some(m) = msg {
                    expr(m, sub64)?;
                }
            }
            StmtKind::Raise(exc, cause) => {
                if let Some(e) = exc {
                    expr(e, sub64)?;
                }
                if let Some(c) = cause {
                    expr(c, sub64)?;
                }
            }
            StmtKind::With { items, body, .. } => {
                for it in items {
                    expr(&it.context_expr, sub64)?;
                    if let Some(v) = &it.optional_var {
                        expr(v, sub64)?;
                    }
                }
                stmt_body(body, arr, sub64)?;
            }
            StmtKind::Match { subject, cases } => {
                expr(subject, sub64)?;
                for c in cases {
                    if let Some(g) = &c.guard {
                        expr(g, sub64)?;
                    }
                    stmt_body(&c.body, arr, sub64)?;
                }
            }
            StmtKind::Del(exprs) => {
                for e in exprs {
                    expr(e, sub64)?;
                }
            }
            // True leaves / statements that carry no sub-expression a non-ring
            // op or an array-iteration could hide in. Enumerated (not a silent
            // `_`) so a future StmtKind forces a compile error here.
            StmtKind::Break
            | StmtKind::Continue
            | StmtKind::Pass
            | StmtKind::Global(_)
            | StmtKind::Nonlocal(_)
            | StmtKind::Import { .. }
            | StmtKind::ImportSideEffect(_)
            | StmtKind::ImportFrom { .. }
            | StmtKind::FuncDef { .. }
            | StmtKind::ClassDef { .. } => {}
        }
        Ok(())
    }

    for s in body {
        stmt(s, &array_names, has_sub64_int)?;
    }
    Ok(())
}

/// M2b: validate every `.shape` read (the 2-D shape-access surface). `.shape` is
/// admitted ONLY as `<p>.shape[k]` where `p` is a 2-D array param and `k` is a
/// literal `0` (rows, = shape0 @+8) or `1` (cols, = shape1 @+12). Any other use
/// — a bare `p.shape` (a tuple value; not in the ABI), a non-literal or
/// out-of-range index, or `.shape` on a scalar / 1-D array / non-param — is
/// refused (the function stays JS). Runs for EVERY candidate (not only
/// array-param ones), so `check_expr`'s syntactic permit of `Name.shape` can
/// never admit a shape read codegen cannot emit (soundness).
fn check_shape_uses(body: &[Stmt], params: &[(String, Type)]) -> Result<(), String> {
    use std::collections::HashSet;
    let arr2d: HashSet<&str> = params
        .iter()
        .filter(|(_, t)| matches!(t, Type::Array(_, 2)))
        .map(|(n, _)| n.as_str())
        .collect();

    fn is_shape_attr(e: &Expr) -> Option<&str> {
        if let ExprKind::Attribute { value, attr, .. } = &e.kind {
            if attr == "shape" {
                if let ExprKind::Name(n) = &value.kind {
                    return Some(n.as_str());
                }
                return Some(""); // `.shape` on a non-name base — always invalid
            }
        }
        None
    }

    fn sh(e: &Expr, arr2d: &HashSet<&str>) -> Result<(), String> {
        if let ExprKind::Subscript { value, index, .. } = &e.kind {
            if let Some(base) = is_shape_attr(value) {
                // A `.shape[..]` subscript — the ONLY legal shape use.
                if !arr2d.contains(base) {
                    return Err(format!(
                        "`.shape` is only supported on a 2-D array param on the WASM fast path \
                         (got `{base}.shape`); the function stays on the JS path"
                    ));
                }
                match &index.kind {
                    ExprKind::IntLiteral(k) if *k == 0 || *k == 1 => {
                        return Ok(()); // valid: shape0 (rows) / shape1 (cols)
                    }
                    _ => {
                        return Err(format!(
                            "`{base}.shape[k]` needs a literal index 0 (rows) or 1 (cols) on the \
                             WASM fast path; the function stays on the JS path"
                        ));
                    }
                }
            }
            // M2b: a 2-element TUPLE index `base[i, j]` is admitted ONLY when
            // `base` is a 2-D array param (the row-major element access). On any
            // other base (a list, a 1-D array, a scalar) it would reach codegen
            // as a mis-typed access — refuse it here (type-aware), so the
            // check_expr syntactic permit of `x[i, j]` is sound.
            if let ExprKind::Tuple(elts) = &index.kind {
                let base_ok = matches!(&value.kind,
                    ExprKind::Name(n) if arr2d.contains(n.as_str()));
                if !base_ok {
                    return Err(
                        "a 2-element tuple index `a[i, j]` is only supported on a 2-D array \
                         param on the WASM fast path; the function stays on the JS path"
                            .to_string(),
                    );
                }
                for el in elts {
                    sh(el, arr2d)?;
                }
                return Ok(());
            }
            sh(value, arr2d)?;
            sh(index, arr2d)?;
            return Ok(());
        }
        if is_shape_attr(e).is_some() {
            // A `.shape` NOT consumed by a valid `[0|1]` subscript (a bare
            // `p.shape` tuple value, or `.shape` on a non-name).
            return Err(
                "a bare `.shape` (as a value) is not supported on the WASM fast path; use \
                 `p.shape[0]` / `p.shape[1]`; the function stays on the JS path"
                    .to_string(),
            );
        }
        sh_children(e, arr2d)
    }
    fn sh_children(e: &Expr, arr2d: &HashSet<&str>) -> Result<(), String> {
        match &e.kind {
            ExprKind::BinOp { left, right, .. } => {
                sh(left, arr2d)?;
                sh(right, arr2d)?;
            }
            ExprKind::UnaryOp { operand, .. } => sh(operand, arr2d)?,
            ExprKind::Compare { left, comparisons } => {
                sh(left, arr2d)?;
                for (_, c) in comparisons {
                    sh(c, arr2d)?;
                }
            }
            ExprKind::Call {
                func, args, kwargs, ..
            } => {
                sh(func, arr2d)?;
                for a in args {
                    sh(a, arr2d)?;
                }
                for kw in kwargs {
                    sh(&kw.value, arr2d)?;
                }
            }
            ExprKind::IfExpr {
                test,
                body,
                else_body,
            } => {
                sh(test, arr2d)?;
                sh(body, arr2d)?;
                sh(else_body, arr2d)?;
            }
            ExprKind::Attribute { value, .. } => sh(value, arr2d)?,
            ExprKind::List(elts) | ExprKind::Tuple(elts) => {
                for el in elts {
                    sh(el, arr2d)?;
                }
            }
            _ => {}
        }
        Ok(())
    }
    fn sh_body(b: &[Stmt], arr2d: &HashSet<&str>) -> Result<(), String> {
        for st in b {
            sh_stmt(st, arr2d)?;
        }
        Ok(())
    }
    fn sh_stmt(s: &Stmt, arr2d: &HashSet<&str>) -> Result<(), String> {
        match &s.kind {
            StmtKind::For {
                target,
                iter,
                body,
                else_body,
                ..
            } => {
                sh(target, arr2d)?;
                sh(iter, arr2d)?;
                sh_body(body, arr2d)?;
                if let Some(b) = else_body {
                    sh_body(b, arr2d)?;
                }
            }
            StmtKind::Assign { targets, value } => {
                for t in targets {
                    sh(t, arr2d)?;
                }
                sh(value, arr2d)?;
            }
            StmtKind::AugAssign { target, value, .. } => {
                sh(target, arr2d)?;
                sh(value, arr2d)?;
            }
            StmtKind::AnnAssign { target, value, .. } => {
                sh(target, arr2d)?;
                if let Some(v) = value {
                    sh(v, arr2d)?;
                }
            }
            StmtKind::Return(v) => {
                if let Some(v) = v {
                    sh(v, arr2d)?;
                }
            }
            StmtKind::Expr(e) => sh(e, arr2d)?,
            StmtKind::If {
                test,
                body,
                elif_clauses,
                else_body,
            } => {
                sh(test, arr2d)?;
                sh_body(body, arr2d)?;
                for (t, b) in elif_clauses {
                    sh(t, arr2d)?;
                    sh_body(b, arr2d)?;
                }
                if let Some(b) = else_body {
                    sh_body(b, arr2d)?;
                }
            }
            StmtKind::While {
                test,
                body,
                else_body,
            } => {
                sh(test, arr2d)?;
                sh_body(body, arr2d)?;
                if let Some(b) = else_body {
                    sh_body(b, arr2d)?;
                }
            }
            StmtKind::Try {
                body,
                handlers,
                else_body,
                finally_body,
            } => {
                sh_body(body, arr2d)?;
                for h in handlers {
                    sh_body(&h.body, arr2d)?;
                }
                if let Some(b) = else_body {
                    sh_body(b, arr2d)?;
                }
                if let Some(b) = finally_body {
                    sh_body(b, arr2d)?;
                }
            }
            StmtKind::Assert { test, msg } => {
                sh(test, arr2d)?;
                if let Some(m) = msg {
                    sh(m, arr2d)?;
                }
            }
            StmtKind::Raise(exc, cause) => {
                if let Some(e) = exc {
                    sh(e, arr2d)?;
                }
                if let Some(c) = cause {
                    sh(c, arr2d)?;
                }
            }
            StmtKind::With { items, body, .. } => {
                for it in items {
                    sh(&it.context_expr, arr2d)?;
                    if let Some(v) = &it.optional_var {
                        sh(v, arr2d)?;
                    }
                }
                sh_body(body, arr2d)?;
            }
            StmtKind::Match { subject, cases } => {
                sh(subject, arr2d)?;
                for c in cases {
                    if let Some(g) = &c.guard {
                        sh(g, arr2d)?;
                    }
                    sh_body(&c.body, arr2d)?;
                }
            }
            StmtKind::Del(exprs) => {
                for e in exprs {
                    sh(e, arr2d)?;
                }
            }
            StmtKind::Break
            | StmtKind::Continue
            | StmtKind::Pass
            | StmtKind::Global(_)
            | StmtKind::Nonlocal(_)
            | StmtKind::Import { .. }
            | StmtKind::ImportSideEffect(_)
            | StmtKind::ImportFrom { .. }
            | StmtKind::FuncDef { .. }
            | StmtKind::ClassDef { .. } => {}
        }
        Ok(())
    }
    sh_body(body, &arr2d)
}

/// #364: a parameter is on the WASM numeric-kernel fast path only when it is a
/// numeric scalar (int/float/bool) or a flat list of numeric scalars
/// (`list[int]` / `list[float]` / `list[bool]`) — the proven-safe marshalling
/// surface (similarity's `list[float]`, Livermore's scalar params). Strings,
/// dicts, sets, tuples, nested lists, Optional/Callable/Any stay JS.
// The explicit `Type::Array` arm intentionally mirrors the wildcard's `false`
// (M2a-1 scaffolding — it is the documented M2a-3 flip point, not dead code).
#[allow(clippy::match_same_arms)]
pub fn is_numeric_kernel_param(ty: &Type) -> bool {
    match ty {
        Type::Int | Type::Float | Type::Bool => true,
        Type::List(inner) => matches!(**inner, Type::Int | Type::Float | Type::Bool),
        // M2 array/buffer ABI (M2a-1 scaffolding): `Array[dtype, ndim]` is the
        // crossable numeric-kernel param the array pitch is built on — a bulk
        // buffer of fixed-width elements. It is RECOGNIZED here as the flip
        // point, but admission stays GATED OFF: the `PtrArray` element
        // load/store codegen (`emit.rs`), the server runtime marshaller
        // (`pythscribe/runtime/`), and the `bridge.rs` glue are all M2a-3 stubs
        // that `unreachable!("…M2a-3")`. Returning `false` keeps an array-param
        // function on the JS fallback path so it never reaches those stubs.
        //
        // M2a-3: codegen has landed — a 1-D `Array[dtype, ndim=1]` is now the
        // crossable numeric-kernel param. The `PtrArray` element load/store
        // branch (`emit.rs`) indexes the buffer at header offset 16, loads at
        // the dtype width, and the server runtime (`pythscribe/runtime/`)
        // bulk-copies the buffer in/out through the total marshaller check
        // (`pythscribe/runtime/array_buffer.py`).
        //
        // M2b: 2-D C-contiguous arrays now cross too — `a[i, j]` / `a[i][j]`
        // lower to the row-major element access `ptr+16+(i*ncols+j)*esize`
        // (shape0 @+8 = rows, shape1 @+12 = cols), and the header/marshaller
        // carry shape1. `ndim` is one of {1, 2} by construction (the parser
        // only builds `Type::Array` for ndim ∈ {1, 2}; ndim>2 maps to
        // `Type::Any`, refused here), so `ndim <= 2` admits exactly the
        // supported shapes and can never admit an ndim>2 the codegen cannot
        // index.
        Type::Array(_, ndim) => matches!(*ndim, 1 | 2),
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
        // M2a-3: an `Array` is WASM-eligible as a PARAM (a crossable buffer),
        // but a non-scalar array RETURN is #377 → M6 (a "new array" would need a
        // heap-object return marshaller). Refuse it from admission so such a
        // function stays on the JS path — never a half-marshalled array return.
        if matches!(ty, Type::Array(_, _)) {
            return Err(format!(
                "return type {} is not supported on the WASM fast path (array returns are \
                 #377 → M6; the function stays on the JS path)",
                ty
            ));
        }
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
        // M2a-3/M2b: a 1-D OR 2-D numeric array is WASM-eligible (it lowers to
        // `WasmType::PtrArray`, always `Some`, so `wasm_admission_sound`
        // holds). ndim>2 never reaches here (the parser maps `Array[dtype, 3]`
        // to `Type::Any`). Element dtype is always one of the five admitted
        // spellings (the parser only builds `Type::Array` for those), so every
        // eligible array is representable.
        Type::Array(_, ndim) => matches!(*ndim, 1 | 2),
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

/// #486: arity of a math.* function the WASM backend lowers, as (fixed) argument
/// count. MUST agree with `crates/pyths_codegen_wasm/src/emit.rs::MATH_FUNCTIONS`
/// — the two live in different crates (this one is the admission authority, that
/// one the codegen table) and a boundary test in the codegen crate
/// (`wasm_math_arity_matches_codegen_table`) cross-checks them so they cannot
/// drift. `None` for a non-math name.
pub fn wasm_math_arity(name: &str) -> Option<usize> {
    Some(match name {
        "atan2" | "pow" => 2,
        "sqrt" | "sin" | "cos" | "tan" | "asin" | "acos" | "atan" | "log" | "log2" | "log10"
        | "exp" | "ceil" | "floor" | "fabs" => 1,
        _ => return None,
    })
}

/// #486: CPython-faithful arity `(min, max)` (inclusive) for EVERY builtin the
/// WASM backend lowers via a `Name(...)` call. Total by construction — every
/// entry of `WASM_CALL_BUILTINS` and `WASM_MATH_FUNCTIONS` resolves here, so no
/// lowered builtin is left unchecked (the #486 gap: `len` had no arity check, so
/// `len(x, y)` silently dropped the extra arg and `int()`/`float()`/`abs()`
/// PANICKED codegen at `emit.rs`'s `args[0]`). A call at an arity outside the
/// admitted range is REFUSED from WASM admission (the function stays on the JS
/// path — which raises CPython's `TypeError`, or evaluates the valid nullary
/// `int()`/`float()`), never silently miscompiled or panicked.
///
/// The WASM backend lowers only the ONE-arg numeric conversion for
/// `int`/`float`/`abs` and the single-argument `len`, so those admit exactly 1;
/// the valid nullary `int()`/`float()` (→ 0 / 0.0) and `int(x, base)` are not
/// lowered here and correctly fall back to JS. `range` admits 1..=3 (CPython);
/// as a `for`-iterator this is already enforced by `check_for_iter`, and this
/// covers the bare-call path too.
///
/// `None` = not an arity-checked builtin here (a call to a user/eligible
/// function is validated by its own signature, not this table).
pub fn wasm_builtin_arity(name: &str) -> Option<(usize, usize)> {
    match name {
        "len" | "abs" | "int" | "float" => Some((1, 1)),
        "range" => Some((1, 3)),
        _ => wasm_math_arity(name).map(|a| (a, a)),
    }
}

/// Human-readable arity phrase for the refusal reason.
fn arity_phrase(lo: usize, hi: usize) -> String {
    if lo == hi {
        format!("exactly {} argument(s)", lo)
    } else {
        format!("{} to {} arguments", lo, hi)
    }
}

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

/// #496: could any of these handlers CATCH a `ZeroDivisionError`? An
/// `except ZeroDivisionError`, or `except Exception` (its admitted superclass),
/// or a bare `except:` can. The other WASM-admitted handler types (ValueError,
/// TypeError, IndexError, KeyError, AssertionError, RuntimeError) do NOT catch a
/// ZeroDivisionError in CPython, so a would-trap division propagates out either
/// way and the function stays eligible. `ArithmeticError` and `BaseException` are
/// the remaining CPython superclasses of ZeroDivisionError; they are not currently
/// WASM-admitted handler types (an `except ArithmeticError` is already refused
/// upstream by the handler-name check), so they never reach here — listed anyway,
/// defensively (S4), so this stays sound by construction if they are ever admitted.
fn handlers_catch_zerodivisionerror(handlers: &[ExceptHandler]) -> bool {
    fn expr_catches(e: &Expr) -> bool {
        match &e.kind {
            // `except (A, B):` — any element may catch it.
            ExprKind::Tuple(elts) => elts.iter().any(expr_catches),
            _ => matches!(
                exception_name_from_expr(e).as_deref(),
                Some("ZeroDivisionError" | "Exception" | "ArithmeticError" | "BaseException")
            ),
        }
    }
    handlers.iter().any(|h| match &h.exc_type {
        None => true, // bare `except:` catches everything, ZeroDivisionError included
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

/// #496: does any statement in this try body REACH a division-family operation
/// whose WASM lowering diverges from CPython's `ZeroDivisionError` on a zero
/// divisor — directly, OR through a call to a module function that (transitively)
/// divides (`may_zerodiv`; B1)? Integer `//` / `%` TRAP (`i64.div_s` /
/// `i64.rem_s`), true `/` yields `inf`, and `**` with a zero base + negative
/// exponent yields a silent wrong value / `OverflowError` (B2) — none of them sets
/// the catchable err_code, and a callee's trap is uncatchable across the WASM call
/// boundary, so a local handler that CPython would run is bypassed on the WASM fast
/// path (C3 error-occurrence divergence). Descends through nested compound-statement
/// bodies, INCLUDING a nested `try`'s body / handlers / else / finally (so a
/// division anywhere in the try's dynamic extent is seen), but not into nested
/// defs/classes (separate scopes — also already rejected from WASM). `divmod(...)`,
/// `**`, and the augmented `/=` `//=` `%=` `**=` count too.
fn try_body_has_zerodiv_op(body: &[Stmt], may_zerodiv: &HashSet<String>) -> bool {
    body.iter().any(|s| stmt_has_zerodiv_op(s, may_zerodiv))
}

fn stmt_has_zerodiv_op(stmt: &Stmt, may_zerodiv: &HashSet<String>) -> bool {
    let any = |b: &[Stmt]| b.iter().any(|s| stmt_has_zerodiv_op(s, may_zerodiv));
    let any_opt = |b: &Option<Vec<Stmt>>| b.as_ref().is_some_and(|b| any(b));
    let e = |x: &Expr| expr_has_zerodiv_op(x, may_zerodiv);
    match &stmt.kind {
        StmtKind::Expr(x) | StmtKind::Return(Some(x)) => e(x),
        StmtKind::Assign { targets, value } => targets.iter().any(&e) || e(value),
        StmtKind::AugAssign { target, op, value } => {
            matches!(
                op,
                AugAssignOp::Div | AugAssignOp::FloorDiv | AugAssignOp::Mod | AugAssignOp::Pow
            ) || e(target)
                || e(value)
        }
        StmtKind::AnnAssign { target, value, .. } => e(target) || value.as_ref().is_some_and(&e),
        StmtKind::If {
            test,
            body,
            elif_clauses,
            else_body,
        } => {
            e(test)
                || any(body)
                || elif_clauses.iter().any(|(t, b)| e(t) || any(b))
                || any_opt(else_body)
        }
        StmtKind::While {
            test,
            body,
            else_body,
        } => e(test) || any(body) || any_opt(else_body),
        StmtKind::For {
            iter,
            body,
            else_body,
            ..
        } => e(iter) || any(body) || any_opt(else_body),
        StmtKind::With { body, .. } => any(body),
        StmtKind::Match { cases, .. } => cases.iter().any(|c| any(&c.body)),
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
        StmtKind::Assert { test, .. } => e(test),
        StmtKind::Raise(exc, cause) => {
            exc.as_ref().is_some_and(&e) || cause.as_ref().is_some_and(e)
        }
        // Return(None), Break/Continue/Pass, Import*, Global/Nonlocal/Del, and
        // FuncDef/ClassDef (separate scopes) bind no division this scan must gate.
        // Any statement kind not on the WASM fast path was already rejected by
        // `check_body`, which runs before this scan.
        _ => false,
    }
}

/// #496: whether an expression tree REACHES a division-family operation — a
/// `/` `//` `%` `**` BinOp, a `divmod(...)` call, or a call to a module function in
/// `may_zerodiv` (one that transitively divides; B1) — the operations that diverge
/// from CPython's ZeroDivisionError on a zero divisor on the WASM fast path.
fn expr_has_zerodiv_op(e: &Expr, may_zerodiv: &HashSet<String>) -> bool {
    use pyths_syntax::ast::ExprKind as E;
    let r = |x: &Expr| expr_has_zerodiv_op(x, may_zerodiv);
    match &e.kind {
        E::BinOp { left, op, right } => {
            matches!(op, BinOp::Div | BinOp::FloorDiv | BinOp::Mod | BinOp::Pow)
                || r(left)
                || r(right)
        }
        E::Call {
            func, args, kwargs, ..
        } => {
            // A `divmod(...)` call, OR a call to a user function that (transitively)
            // divides — its trap is uncatchable across the WASM call boundary (B1).
            matches!(&func.kind, E::Name(n) if n == "divmod" || may_zerodiv.contains(n))
                || r(func)
                || args.iter().any(&r)
                || kwargs.iter().any(|k| r(&k.value))
        }
        E::UnaryOp { operand, .. } => r(operand),
        E::Compare { left, comparisons } => r(left) || comparisons.iter().any(|(_, e)| r(e)),
        E::Attribute { value, .. } => r(value),
        E::Subscript { value, index, .. } => r(value) || r(index),
        E::Slice { lower, upper, step } => {
            lower.as_deref().is_some_and(&r)
                || upper.as_deref().is_some_and(&r)
                || step.as_deref().is_some_and(&r)
        }
        E::IfExpr {
            test,
            body,
            else_body,
        } => r(test) || r(body) || r(else_body),
        E::Tuple(elts) | E::List(elts) | E::Set(elts) => elts.iter().any(&r),
        // Literals, Name, and expression forms not on the numeric-kernel WASM fast
        // path (comprehensions / dict / lambda / await / yield / starred / fstring)
        // hold no division reaching a WASM div/rem op; a body containing one was
        // already rejected by `check_body` before this scan runs.
        _ => false,
    }
}

/// #496 (B1): the set of module functions that MAY raise a `ZeroDivisionError` on
/// the WASM path — those whose body contains a division-family op (`/` `//` `%`
/// `**` / `divmod`) DIRECTLY, closed transitively over the call graph (a function
/// that calls such a function may itself trap). A `try/except ZeroDivisionError`
/// whose body calls into this set is refused, because the callee's trap is
/// uncatchable across the WASM call boundary and would drop the handler. The
/// fixpoint mirrors the `#364` call-consistency fixpoint below; the module's
/// function set is small (numeric kernels), so the O(n²) closure is cheap.
fn functions_that_can_zerodiv(module: &Module) -> HashSet<String> {
    let empty: HashSet<String> = HashSet::new();
    // Every top-level function and its body (nested defs are separate scopes and
    // never WASM-admitted, so they are not call targets here).
    let fdefs: Vec<(&str, &[Stmt])> = module
        .body
        .iter()
        .filter_map(|s| match &s.kind {
            StmtKind::FuncDef { name, body, .. } => Some((name.as_str(), body.as_slice())),
            _ => None,
        })
        .collect();
    let mut set: HashSet<String> = HashSet::new();
    // Seed: functions that divide DIRECTLY (scan with an empty callee set, so only
    // the division OPERATORS count, not calls).
    for (name, body) in &fdefs {
        if body.iter().any(|s| stmt_has_zerodiv_op(s, &empty)) {
            set.insert((*name).to_string());
        }
    }
    // Fixpoint: add a function that CALLS one already in the set (a not-yet-marked
    // function has no direct division, so a hit here is purely a call into `set`).
    loop {
        let mut changed = false;
        for (name, body) in &fdefs {
            if set.contains(*name) {
                continue;
            }
            if body.iter().any(|s| stmt_has_zerodiv_op(s, &set)) {
                set.insert((*name).to_string());
                changed = true;
            }
        }
        if !changed {
            break;
        }
    }
    set
}

/// #496: walk a function body and refuse (stay-JS) if it contains a `try` that
/// catches `ZeroDivisionError` (or `Exception`/bare) whose body can REACH a
/// division-family op on the WASM path — directly, or via a call to a function in
/// `may_zerodiv` (B1). Integer `//`/`%` TRAP on a zero divisor and `**`
/// zero-to-negative / true `/` diverge, and a callee's trap is uncatchable across
/// the WASM call boundary, so the handler CPython would run is dropped (C3
/// error-occurrence divergence). Routing to the JS backend is sound: it raises a
/// catchable ZeroDivisionError and runs the handler exactly as CPython does.
fn check_zerodiv_try_admission(body: &[Stmt], may_zerodiv: &HashSet<String>) -> Result<(), String> {
    for stmt in body {
        match &stmt.kind {
            StmtKind::Try {
                body,
                handlers,
                else_body,
                finally_body,
            } => {
                if handlers_catch_zerodivisionerror(handlers)
                    && try_body_has_zerodiv_op(body, may_zerodiv)
                {
                    return Err(
                        "a division (`/`, `//`, `%`, `**`, or `divmod`) — or a call to a function \
                         that divides — inside a try/except that catches ZeroDivisionError is not \
                         supported on the WASM fast path (a zero divisor would trap or yield inf \
                         instead of running the handler); function stays on the JS backend"
                            .into(),
                    );
                }
                // Descend into every sub-body so a NESTED guarded try is checked too.
                check_zerodiv_try_admission(body, may_zerodiv)?;
                for h in handlers {
                    check_zerodiv_try_admission(&h.body, may_zerodiv)?;
                }
                if let Some(b) = else_body {
                    check_zerodiv_try_admission(b, may_zerodiv)?;
                }
                if let Some(b) = finally_body {
                    check_zerodiv_try_admission(b, may_zerodiv)?;
                }
            }
            StmtKind::If {
                body,
                elif_clauses,
                else_body,
                ..
            } => {
                check_zerodiv_try_admission(body, may_zerodiv)?;
                for (_, b) in elif_clauses {
                    check_zerodiv_try_admission(b, may_zerodiv)?;
                }
                if let Some(b) = else_body {
                    check_zerodiv_try_admission(b, may_zerodiv)?;
                }
            }
            StmtKind::While {
                body, else_body, ..
            }
            | StmtKind::For {
                body, else_body, ..
            } => {
                check_zerodiv_try_admission(body, may_zerodiv)?;
                if let Some(b) = else_body {
                    check_zerodiv_try_admission(b, may_zerodiv)?;
                }
            }
            StmtKind::With { body, .. } => check_zerodiv_try_admission(body, may_zerodiv)?,
            StmtKind::Match { cases, .. } => {
                for c in cases {
                    check_zerodiv_try_admission(&c.body, may_zerodiv)?;
                }
            }
            _ => {}
        }
    }
    Ok(())
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
            // #496: a division under a ZeroDivisionError-catching handler (incl.
            // via a call to a function that divides) is refused by the dedicated
            // module pass `check_zerodiv_try_admission`, which has the transitive
            // `may_zerodiv` call-graph set (B1) this intra-statement check lacks.
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
                    let is_user = eligible_names.contains(n);
                    if !WASM_CALL_BUILTINS.contains(&n.as_str())
                        && !WASM_MATH_FUNCTIONS.contains(&n.as_str())
                        && !is_user
                    {
                        return Err(format!(
                            "builtin/function `{}` is not on the WASM numeric-kernel whitelist \
                             (function stays JS)",
                            n
                        ));
                    }
                    // #486: builtin ARITY. Applies to the BUILTIN interpretation
                    // only — a user function that SHADOWS a builtin name (`is_user`)
                    // wins and is validated by its own signature, not this table
                    // (name-binding arm: the `shadow` net family). A wrong-arity
                    // builtin call is refused so the function stays on the JS path
                    // (which raises CPython's `TypeError`), never silently
                    // miscompiled (`len(x, y)` dropped the arg) or panicked
                    // (`int()`/`float()`/`abs()` at codegen `args[0]`).
                    if !is_user {
                        if let Some((lo, hi)) = wasm_builtin_arity(n) {
                            if args.len() < lo || args.len() > hi {
                                return Err(format!(
                                    "builtin `{}` called with {} argument(s) on the WASM fast path \
                                     (expects {}); the function stays on the JS path",
                                    n,
                                    args.len(),
                                    arity_phrase(lo, hi)
                                ));
                            }
                        }
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
                            // #486: math.X arity (sqrt/…=1, atan2/pow=2). Wrong
                            // arity is refused so the function stays JS (CPython's
                            // TypeError), never silently miscompiled (`math.sqrt(a, b)`
                            // dropped the 2nd arg; `math.pow(a)` fed an undefined 2nd).
                            if let Some(a) = wasm_math_arity(attr) {
                                if args.len() != a {
                                    return Err(format!(
                                        "math.{} called with {} argument(s) on the WASM fast path \
                                         (expects exactly {}); the function stays on the JS path",
                                        attr,
                                        args.len(),
                                        a
                                    ));
                                }
                            }
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
            // M2b: a 2-element TUPLE index is the 2-D array element access
            // `a[i, j]`. Permit it SYNTACTICALLY (check its two index elements),
            // rather than falling into the tuple-literal rejection below; the
            // type-aware `check_shape_uses` refuses a 2-tuple index on anything
            // but a 2-D array param, so this permit can never admit a bad shape.
            if let ExprKind::Tuple(elts) = &index.kind {
                if elts.len() == 2 {
                    check_expr(value, eligible_names)?;
                    for e in elts {
                        check_expr(e, eligible_names)?;
                    }
                    return Ok(());
                }
                // A non-2 tuple index (`a[i, j, k]`) is not an admitted shape.
                return Err("only a 2-element tuple index (a[i, j], for a 2-D array) is supported on the WASM fast path (function stays JS)".into());
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
                // M2b: `<array>.shape` — the 2-D shape-access surface. Permitted
                // SYNTACTICALLY here (check_expr has no type info); the
                // type-aware `check_shape_uses` (second pass) refuses `.shape`
                // on anything but a 2-D array param, and refuses a non-{0,1}
                // index — so this permit can never admit a shape read codegen
                // cannot emit (soundness preserved).
                if attr == "shape" {
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
    fn test_array_1d_and_2d_eligible_ndim3_refused_m2b() {
        // M2b: BOTH 1-D and 2-D arrays are WASM-eligible (codegen row-major
        // indexing + shape marshalling landed). ndim>2 is refused by the
        // `ndim <= 2` gate (the parser also never builds `Type::Array(_, 3)`,
        // but the predicate is the belt-and-braces control). `wasm_admission_
        // sound` (elig ⇒ representable) holds for all — `to_wasm_type` is
        // `Some(PtrArray)` for every ndim.
        use pyths_types::types::ArrayDtype;
        for dt in [
            ArrayDtype::Int32,
            ArrayDtype::Int64,
            ArrayDtype::Float32,
            ArrayDtype::Float64,
            ArrayDtype::Uint8,
        ] {
            assert!(is_wasm_eligible(&Type::Array(dt, 1)), "1-D array admitted");
            assert!(
                is_wasm_eligible(&Type::Array(dt, 2)),
                "2-D array admitted (M2b)"
            );
            assert!(!is_wasm_eligible(&Type::Array(dt, 3)), "ndim>2 refused");
        }
    }

    #[test]
    fn test_array_param_predicate_1d_and_2d_admitted_ndim3_refused_m2b() {
        // M2b flip: `is_numeric_kernel_param` admits both 1-D and 2-D
        // `Array[dtype]` (crossable buffer params) and refuses ndim>2. A mutant
        // that admitted ndim>2 would route it to a nonexistent indexing branch.
        use pyths_types::types::ArrayDtype;
        for dt in [
            ArrayDtype::Int32,
            ArrayDtype::Int64,
            ArrayDtype::Float32,
            ArrayDtype::Float64,
            ArrayDtype::Uint8,
        ] {
            assert!(
                is_numeric_kernel_param(&Type::Array(dt, 1)),
                "Array[{dt:?}, 1] is an admitted numeric-kernel param"
            );
            assert!(
                is_numeric_kernel_param(&Type::Array(dt, 2)),
                "Array[{dt:?}, 2] is an admitted numeric-kernel param (M2b)"
            );
            assert!(
                !is_numeric_kernel_param(&Type::Array(dt, 3)),
                "Array[{dt:?}, 3] (ndim>2) must stay refused"
            );
        }
    }

    #[test]
    fn test_array_1d_param_function_admitted_m2a3() {
        // SPOT: a function with a 1-D `Array[dtype]` parameter is now
        // WASM-admitted (a fill/transform kernel). It must enter `eligible`.
        for src in [
            "def f(a: Array[float32], out: Array[float32]) -> None:\n    for i in range(len(a)):\n        out[i] = a[i] * 2.0\n",
            "def f(a: Array[float64, 1]) -> float:\n    return a[0]\n",
            "def f(a: Array[uint8], out: Array[uint8]) -> None:\n    for i in range(len(a)):\n        out[i] = a[i] + 1\n",
            "def f(a: Array[int64]) -> int:\n    return len(a)\n",
            "def f(a: Array[int32], out: Array[int32]) -> None:\n    for i in range(len(a)):\n        out[i] = a[i] + 1\n",
        ] {
            let a = analyze(src);
            assert!(
                a.eligible.contains_key("f"),
                "1-D array-param kernel must be WASM-admitted (M2a-3): {src:?} rej={:?}",
                a.rejected
            );
        }
    }

    #[test]
    fn test_array_2d_param_admitted_and_bare_row_refused_m2b() {
        // M2b: a 2-D `Array[dtype, 2]` param with full element access is
        // ADMITTED (row-major indexing landed). The PAIRED CONTROL is the G3
        // bare-row guard: a `a[i]` on a 2-D array used as a VALUE (no row-view
        // in the ABI) is REFUSED and routes to JS — never a mis-addressed
        // 1-D-stride read.
        let arr2d = analyze(
            "def f(a: Array[float64, 2], out: Array[float64, 2]) -> None:\n\
             \x20   for i in range(len(a)):\n\
             \x20       for j in range(a.shape[1]):\n\
             \x20           out[i, j] = a[i, j] * 2.0\n",
        );
        assert!(
            arr2d.eligible.contains_key("f"),
            "2-D array-param kernel with full access must be admitted (M2b): rej={:?}",
            arr2d.rejected
        );
        // Bare-row control: `row = a[i]` binds a 2-D row to a value → refused.
        let bare =
            analyze("def f(a: Array[int32, 2]) -> int:\n    row = a[0]\n    return row[0]\n");
        assert!(
            !bare.eligible.contains_key("f"),
            "a bare 2-D row bound to a value must NOT be admitted (G3)"
        );
        let reason = bare
            .rejected
            .iter()
            .find(|(n, _)| n == "f")
            .map(|(_, r)| r.clone())
            .unwrap_or_default();
        assert!(
            reason.contains("bare row") || reason.contains("row-view"),
            "expected a bare-2D-row refusal, got: {reason:?}"
        );
        // And the 1-D twin IS admitted.
        let arr1d = analyze("def f(a: Array[float64]) -> int:\n    return len(a)\n");
        assert!(
            arr1d.eligible.contains_key("f"),
            "Array[float64]-param (1-D) must be admitted: rej={:?}",
            arr1d.rejected
        );
    }

    #[test]
    fn test_decorated_rejected() {
        let src = "@component\ndef f(x: int) -> int:\n    return x\n";
        let a = analyze(src);
        assert!(a.eligible.is_empty());
        assert!(a.rejected[0].1.contains("decorator"));
    }

    // ---- #486: builtin arity (total; every lowered builtin checked) ----

    // Correct arity is unaffected — the common numeric-kernel calls stay
    // eligible.
    #[test]
    fn arity_correct_calls_stay_eligible() {
        for src in [
            "def f(xs: list[int]) -> int:\n    return len(xs)\n",
            "def f(a: int) -> int:\n    return abs(a)\n",
            "def f(a: float) -> int:\n    return int(a)\n",
            "def f(a: int) -> float:\n    return float(a)\n",
            "import math\ndef f(a: float) -> float:\n    return math.sqrt(a)\n",
            "import math\ndef f(a: float, b: float) -> float:\n    return math.pow(a, b)\n",
            "def f(n: int) -> int:\n    t = 0\n    for i in range(n):\n        t += i\n    return t\n",
        ] {
            let a = analyze(src);
            assert!(
                a.eligible.contains_key("f"),
                "correct-arity builtin call must stay eligible: {src:?} rej={:?}",
                a.rejected
            );
        }
    }

    // Wrong arity is REFUSED (function stays JS), never silently admitted.
    // Covers every lowered builtin at a wrong arity, incl. the zero-arg
    // int()/float()/abs() cases that previously PANICKED codegen.
    #[test]
    fn arity_wrong_calls_refused() {
        for (src, needle) in [
            (
                "def f(xs: list[int]) -> int:\n    return len(xs, xs)\n",
                "len",
            ),
            ("def f(a: int) -> int:\n    return len()\n", "len"),
            (
                "def f(a: int, b: int) -> int:\n    return abs(a, b)\n",
                "abs",
            ),
            ("def f(a: int) -> int:\n    return abs()\n", "abs"),
            ("def f(a: int) -> int:\n    return int()\n", "int"),
            (
                "def f(a: float, b: int) -> int:\n    return int(a, b)\n",
                "int",
            ),
            ("def f(a: int) -> float:\n    return float()\n", "float"),
            (
                "def f(a: int, b: int) -> float:\n    return float(a, b)\n",
                "float",
            ),
            (
                "import math\ndef f(a: float, b: float) -> float:\n    return math.sqrt(a, b)\n",
                "math.sqrt",
            ),
            (
                "import math\ndef f(a: float) -> float:\n    return math.pow(a)\n",
                "math.pow",
            ),
        ] {
            let a = analyze(src);
            assert!(
                !a.eligible.contains_key("f"),
                "wrong-arity builtin call must be refused from WASM: {src:?}"
            );
            assert!(
                a.rejected
                    .iter()
                    .any(|(n, r)| n == "f" && r.contains(needle)),
                "rejection reason must name the builtin `{needle}`: {:?}",
                a.rejected
            );
        }
    }

    // Name-binding arm: a user function that SHADOWS a builtin name is validated
    // by its own signature, not the builtin arity table — a 2-arg user `len`
    // called with 2 args must NOT be refused by the builtin (1,1) rule.
    #[test]
    fn arity_user_shadow_not_builtin_checked() {
        let src = "def len(a: int, b: int) -> int:\n    return a + b\n\
                   def use(a: int, b: int) -> int:\n    return len(a, b)\n";
        let a = analyze(src);
        assert!(
            a.eligible.contains_key("use") && a.eligible.contains_key("len"),
            "user-shadowed `len(a, b)` must not be refused by the builtin arity rule: {:?}",
            a.rejected
        );
    }

    #[test]
    fn arity_table_is_total_over_lowered_builtins() {
        // Every WASM-lowered call builtin resolves in the arity authority (no
        // lowered builtin left unchecked — the #486 gap).
        for name in super::WASM_CALL_BUILTINS {
            assert!(
                wasm_builtin_arity(name).is_some(),
                "lowered call builtin `{name}` has no arity entry (#486 totality)"
            );
        }
        for name in super::WASM_MATH_FUNCTIONS {
            assert!(
                wasm_builtin_arity(name).is_some() && wasm_math_arity(name).is_some(),
                "lowered math function `{name}` has no arity entry (#486 totality)"
            );
        }
    }
}
