//! #491 BLOCKER-3 (root): a nested scope's `global N` must never be captured by
//! an ENCLOSING function's lexical local of the same name.
//!
//! CPython resolves `global N` at MODULE scope no matter how many enclosing
//! function locals named `N` sit between the declaring scope and the module.
//! Emitted JS is lexically scoped: `export let abs = pyAbs; function outer() {
//! let abs = 1; function inner() { abs = 7; } }` — `inner`'s bare `abs` resolves
//! to OUTER's `let abs`, so the write corrupts the enclosing local AND never
//! reaches the module cell (silent wrong values on BOTH sides: `outer()` returned
//! 7 instead of 1, `print(abs)` printed the builtin instead of 7). The same
//! capture hits a `global N` READ (`return abs(-5)` called outer's int), a class
//! body's `global N` write nested in a function that binds `N`, and every
//! nesting depth (each intervening function local captures in turn).
//!
//! THE ONE AUTHORITY (deep fix, "replace the model, not the symptom"): an AST
//! pre-pass run at the top of `emit_module` (the single seam every codegen entry
//! point and the WASM glue twin pass through). For every function scope `F`, a
//! local binding `N` of `F` (a param or a body binder, not `global`/`nonlocal`-
//! declared in `F`) is a COLLISION iff some scope nested anywhere inside `F`
//! declares `global N`. Each colliding local is renamed to a fresh identifier
//! `N$l<k>` throughout `F`'s lexical region under CPython's symtable rules —
//! every read, write, aug-assign, tuple element, `del`, walrus, for/with/except/
//! import/match binder, nested def/class NAME, `nonlocal N` in a nested scope,
//! free references from nested defs / lambdas / comprehensions / class bodies —
//! and NOT the occurrences that CPython resolves elsewhere: a nested scope that
//! itself binds `N`, a nested scope that declares `global N` (and its whole
//! subtree), a class body that binds `N` or declares it `global` (its direct
//! statements; its methods still see `F`'s local), a comprehension whose target
//! is `N` (its leftmost iterable still sees `F`'s local), a param default /
//! annotation / decorator of `F` itself (evaluated in `F`'s ENCLOSING scope).
//! The emitter then sees a function whose local can no longer collide, so the
//! nested `global N` access is a bare module-cell access by construction — no
//! emission site had to learn anything (there is no single identifier chokepoint
//! in the emitter; a per-site fix would be the whack-a-mole this pass replaces).
//!
//! EXPORT SAFETY: only FUNCTION-LOCAL bindings are renamed. A module cell that
//! is a user module-level `def abs():` / assignment keeps its user name and its
//! `export let abs` / `export { abs }` surface; the `.d.ts` is generated from the
//! ORIGINAL module. `$` can never occur in a Python identifier, so `N$l<k>` is
//! fresh by construction (the same argument as `fresh_temp`), and the Python
//! name is RECOVERABLE by parsing (`python_name`) at the few sites where a PARAM
//! name becomes runtime metadata rather than a JS identifier: `__pyparams__`
//! (keyword binding), the keyword-only prologue (`__pyKwPop` / `__pyNoExtraKw`),
//! the `@component` props destructure (prop KEY), and the sentinel-read error
//! messages (`__pyChkLocal` / `__pyChkFree`). Lambda params are never renamed (a
//! lambda body cannot contain a `global` statement).
//!
//! OVER-FIX GUARD: a local with NO colliding nested `global` keeps its name (the
//! common path returns `None` and emits byte-identical JS).
//!
//! REFUSED SHAPE (#491 SF2, loud by construction): a function-local DOTTED
//! import without an alias (`import a.b`) binds the top package `a` as a local
//! of `F` and CANNOT be renamed (aliasing it would bind the SUBMODULE), so when
//! `a` collides with a nested `global a` the emitted `let a = {}` graft object
//! would still capture the nested write — a silent wrong value. The pass
//! REFUSES that exact shape with a compile diagnostic (`refused`, surfaced by
//! `emit_module` through `emit_import_error` → `pyths compile` fails) instead
//! of shipping it; alias the import or rename the local.
//!
//! Controls: `tests/differential/shadow_491/cases/28_*`–`31_*` (3-way vs
//! CPython), `behavioral_differential.rs::test_nested_global_*`, mutation M11
//! (`mutations.py`: the pass returns `None` → every new case RED again); the
//! refusal: unit test `dotted_import_head_collision_is_refused` + mutation M14.

use crate::emit::JsCodegen;
use pyths_syntax::ast::*;
use std::collections::{HashMap, HashSet};

/// old name → fresh JS-only name, for the region currently being walked.
type Renames = HashMap<String, String>;

const TAG: &str = "$l";

/// The Python-source name behind a (possibly renamed) local identifier:
/// `abs$l3` → `abs`; anything else is returned unchanged. Pure string parsing —
/// `$` never occurs in a Python identifier, so a `$l<digits>` tail can only
/// have been minted here.
pub(crate) fn python_name(ident: &str) -> &str {
    if let Some(i) = ident.rfind(TAG) {
        let (head, tail) = (&ident[..i], &ident[i + TAG.len()..]);
        if !head.is_empty() && !tail.is_empty() && tail.bytes().all(|b| b.is_ascii_digit()) {
            return head;
        }
    }
    ident
}

/// The pre-pass result: the renamed module (`None` = no collision, emit the
/// original byte-identically) plus the REFUSED shapes (#491 SF2) — each a
/// compile diagnostic the caller must surface; the module is still returned so
/// the refusal is loud at compile time rather than a silent miscompile.
pub(crate) struct CapturedGlobalRename {
    pub(crate) module: Option<Module>,
    pub(crate) refused: Vec<String>,
}

/// Run the pre-pass. `module: None` when no function local collides with a
/// nested `global` declaration (the common case: the caller keeps emitting from
/// the original module, byte-identically). `Some(renamed_clone)` otherwise.
pub(crate) fn rename_captured_globals(module: &Module) -> CapturedGlobalRename {
    if !has_global_inside_a_function(&module.body, false) {
        return CapturedGlobalRename {
            module: None,
            refused: Vec::new(),
        };
    }
    let mut out = module.clone();
    let mut st = State {
        counter: 0,
        changed: false,
        refused: Vec::new(),
    };
    process_nested_functions(&mut out.body, &mut st);
    CapturedGlobalRename {
        module: if st.changed { Some(out) } else { None },
        refused: st.refused,
    }
}

struct State {
    counter: u32,
    changed: bool,
    /// #491 SF2: the unrenamable shapes found (compile diagnostics).
    refused: Vec<String>,
}

/// Cheap admission scan: is there ANY `global` statement inside a function body
/// (at any depth — through control flow and class bodies)? Only then can a
/// collision exist.
fn has_global_inside_a_function(stmts: &[Stmt], in_fn: bool) -> bool {
    stmts.iter().any(|s| match &s.kind {
        StmtKind::Global(_) => in_fn,
        StmtKind::FuncDef { body, .. } => has_global_inside_a_function(body, true),
        StmtKind::ClassDef { body, .. } => has_global_inside_a_function(body, in_fn),
        _ => child_blocks(s)
            .iter()
            .any(|b| has_global_inside_a_function(b, in_fn)),
    })
}

/// Every `global`-declared name anywhere in `stmts`' subtree (nested def /
/// class bodies included) — the trigger set for the enclosing function.
fn collect_deep_globals(stmts: &[Stmt], out: &mut HashSet<String>) {
    for s in stmts {
        match &s.kind {
            StmtKind::Global(names) => out.extend(names.iter().cloned()),
            StmtKind::FuncDef { body, .. } | StmtKind::ClassDef { body, .. } => {
                collect_deep_globals(body, out)
            }
            _ => {
                for b in child_blocks(s) {
                    collect_deep_globals(b, out);
                }
            }
        }
    }
}

/// The statement lists nested in a control-flow statement (not def/class).
fn child_blocks(s: &Stmt) -> Vec<&[Stmt]> {
    let mut v: Vec<&[Stmt]> = Vec::new();
    match &s.kind {
        StmtKind::If {
            body,
            elif_clauses,
            else_body,
            ..
        } => {
            v.push(body);
            for (_, b) in elif_clauses {
                v.push(b);
            }
            if let Some(b) = else_body {
                v.push(b);
            }
        }
        StmtKind::While {
            body, else_body, ..
        }
        | StmtKind::For {
            body, else_body, ..
        } => {
            v.push(body);
            if let Some(b) = else_body {
                v.push(b);
            }
        }
        StmtKind::With { body, .. } => v.push(body),
        StmtKind::Try {
            body,
            handlers,
            else_body,
            finally_body,
        } => {
            v.push(body);
            for h in handlers {
                v.push(&h.body);
            }
            if let Some(b) = else_body {
                v.push(b);
            }
            if let Some(b) = finally_body {
                v.push(b);
            }
        }
        StmtKind::Match { cases, .. } => {
            for c in cases {
                v.push(&c.body);
            }
        }
        _ => {}
    }
    v
}

/// Visit every function definition in `stmts` (any depth, through control flow
/// and class bodies) and resolve ITS collisions. Order-independent: a parent's
/// rename only rewrites references that resolve to the parent's local, which
/// never changes a nested function's own binding set or the `global` names.
fn process_nested_functions(stmts: &mut [Stmt], st: &mut State) {
    for s in stmts.iter_mut() {
        match &mut s.kind {
            StmtKind::FuncDef {
                name, params, body, ..
            } => {
                process_function(name, params, body, st);
                process_nested_functions(body, st);
            }
            StmtKind::ClassDef { body, .. } => process_nested_functions(body, st),
            StmtKind::If {
                body,
                elif_clauses,
                else_body,
                ..
            } => {
                process_nested_functions(body, st);
                for (_, b) in elif_clauses.iter_mut() {
                    process_nested_functions(b, st);
                }
                if let Some(b) = else_body {
                    process_nested_functions(b, st);
                }
            }
            StmtKind::While {
                body, else_body, ..
            }
            | StmtKind::For {
                body, else_body, ..
            } => {
                process_nested_functions(body, st);
                if let Some(b) = else_body {
                    process_nested_functions(b, st);
                }
            }
            StmtKind::With { body, .. } => process_nested_functions(body, st),
            StmtKind::Try {
                body,
                handlers,
                else_body,
                finally_body,
            } => {
                process_nested_functions(body, st);
                for h in handlers.iter_mut() {
                    process_nested_functions(&mut h.body, st);
                }
                if let Some(b) = else_body {
                    process_nested_functions(b, st);
                }
                if let Some(b) = finally_body {
                    process_nested_functions(b, st);
                }
            }
            StmtKind::Match { cases, .. } => {
                for c in cases.iter_mut() {
                    process_nested_functions(&mut c.body, st);
                }
            }
            _ => {}
        }
    }
}

/// ONE function scope `F`: locals(F) ∩ deep_globals(F.body) are the collisions.
fn process_function(fname: &str, params: &mut [Param], body: &mut [Stmt], st: &mut State) {
    let param_names: Vec<String> = params.iter().map(|p| p.name.clone()).collect();
    // The emitter's own #438 local census — the same authority that decides
    // what `F` binds, so "is a local" agrees with `scope_bindings` by construction.
    let locals = JsCodegen::collect_local_bindings(body, &param_names);
    let mut deep = HashSet::new();
    collect_deep_globals(body, &mut deep);
    // #491 SF2: the ONE shape the rename cannot express — a dotted import
    // without an alias in `F`'s own region (through control flow; a nested
    // def / class body binds its own `a`) whose HEAD is declared `global`
    // by a nested scope. Keyed on the deep `global` set DIRECTLY, not on
    // `locals ∩ deep`: the #438 census does not count a dotted import's head
    // as a local, so `def outer(): import os.path; def inner(): global os; …`
    // has no census collision at all — yet the emitted `let os = {}` graft
    // would still capture the nested write. Refuse it loudly here;
    // `walk_stmt`'s Import arm leaves the statement as is, and the caller
    // fails the compile on `refused`.
    if !deep.is_empty() {
        collect_refused_dotted_imports(fname, body, &deep, &mut st.refused);
    }
    let mut colliding: Vec<&String> = locals.iter().filter(|n| deep.contains(*n)).collect();
    if colliding.is_empty() {
        return;
    }
    colliding.sort(); // deterministic numbering
    let mut r: Renames = HashMap::new();
    for n in colliding {
        r.insert(n.clone(), format!("{n}{TAG}{}", st.counter));
        st.counter += 1;
    }
    st.changed = true;
    for p in params.iter_mut() {
        rename_str(&mut p.name, &r);
    }
    walk_stmts(body, &r, &r);
}

/// #491 SF2: every `import a.b` (no alias) in `F`'s own statements — through
/// control-flow blocks, NOT into nested def / class bodies (their `a` is their
/// own binding, dropped from the renames) — whose head `a` is a colliding local.
fn collect_refused_dotted_imports(
    fname: &str,
    stmts: &[Stmt],
    colliding: &HashSet<String>,
    out: &mut Vec<String>,
) {
    for s in stmts {
        match &s.kind {
            StmtKind::Import { names } => {
                for a in names {
                    if a.alias.is_none() && a.name.contains('.') {
                        let head = a.name.split('.').next().unwrap_or("").to_string();
                        if colliding.contains(&head) {
                            out.push(format!(
                                "`import {dotted}` inside `{fname}`: the package head `{head}` is a \
                                 local of `{fname}` that a nested scope declares `global {head}`. \
                                 A dotted import's head cannot be renamed (aliasing it would bind \
                                 the submodule), so the nested `global {head}` write would silently \
                                 land on the local package object instead of the module binding. \
                                 Alias the import (`import {dotted} as <name>`) or rename the local.",
                                dotted = a.name
                            ));
                        }
                    }
                }
            }
            StmtKind::FuncDef { .. } | StmtKind::ClassDef { .. } => {}
            _ => {
                for b in child_blocks(s) {
                    collect_refused_dotted_imports(fname, b, colliding, out);
                }
            }
        }
    }
}

fn rename_str(s: &mut String, r: &Renames) {
    if let Some(n) = r.get(s.as_str()) {
        *s = n.clone();
    }
}

fn minus(r: &Renames, drop: impl Fn(&str) -> bool) -> Renames {
    r.iter()
        .filter(|(k, _)| !drop(k))
        .map(|(k, v)| (k.clone(), v.clone()))
        .collect()
}

/// The renames in force for a NESTED FUNCTION scope: a name the nested function
/// binds itself (param / body binder) or declares `global` is its own (or the
/// module's) — dropped; a `nonlocal` one refers to the enclosing local — kept.
fn nested_function_renames(nested: &Renames, params: &[Param], body: &[Stmt]) -> Renames {
    let param_names: Vec<String> = params.iter().map(|p| p.name.clone()).collect();
    let locals = JsCodegen::collect_local_bindings(body, &param_names);
    let globals = JsCodegen::collect_global_declared(body);
    minus(nested, |n| locals.contains(n) || globals.contains(n))
}

/// The renames in force for the DIRECT statements of a class body: a name the
/// class block binds (any value binder, or a def / class of that name) is a
/// class attribute (`LOAD_NAME`: class dict → globals → builtins, never the
/// enclosing local); a `global`-declared one is the module's. Nested scopes of
/// the class body are NOT affected (class namespaces are skipped by CPython's
/// symtable) — they keep using the enclosing `nested` map.
fn class_body_renames(nested: &Renames, body: &[Stmt]) -> Renames {
    let mut bound = JsCodegen::class_body_value_binders(body);
    for s in body {
        if let StmtKind::FuncDef { name, .. } | StmtKind::ClassDef { name, .. } = &s.kind {
            bound.insert(name.clone());
        }
    }
    let globals = JsCodegen::collect_global_declared(body);
    minus(nested, |n| bound.contains(n) || globals.contains(n))
}

fn walk_stmts(stmts: &mut [Stmt], here: &Renames, nested: &Renames) {
    for s in stmts.iter_mut() {
        walk_stmt(s, here, nested);
    }
}

fn walk_opt_stmts(b: &mut Option<Vec<Stmt>>, here: &Renames, nested: &Renames) {
    if let Some(b) = b {
        walk_stmts(b, here, nested);
    }
}

fn walk_stmt(s: &mut Stmt, here: &Renames, nested: &Renames) {
    match &mut s.kind {
        StmtKind::Expr(e) => walk_expr(e, here, nested),
        StmtKind::Assign { targets, value } => {
            for t in targets.iter_mut() {
                walk_expr(t, here, nested);
            }
            walk_expr(value, here, nested);
        }
        StmtKind::AugAssign { target, value, .. } => {
            walk_expr(target, here, nested);
            walk_expr(value, here, nested);
        }
        StmtKind::AnnAssign {
            target,
            annotation,
            value,
        } => {
            walk_expr(target, here, nested);
            walk_expr(annotation, here, nested);
            if let Some(v) = value {
                walk_expr(v, here, nested);
            }
        }
        StmtKind::FuncDef {
            name,
            params,
            body,
            decorator_list,
            return_type,
            ..
        } => {
            // The def NAME binds in the current scope; decorators, defaults,
            // annotations and the return type are evaluated in it too.
            rename_str(name, here);
            for d in decorator_list.iter_mut() {
                walk_expr(d, here, nested);
            }
            if let Some(rt) = return_type {
                walk_expr(rt, here, nested);
            }
            for p in params.iter_mut() {
                if let Some(a) = &mut p.annotation {
                    walk_expr(a, here, nested);
                }
                if let Some(d) = &mut p.default {
                    walk_expr(d, here, nested);
                }
            }
            let inner = nested_function_renames(nested, params, body);
            if !inner.is_empty() {
                // A param named N was dropped from `inner`, so params never
                // need renaming here.
                walk_stmts(body, &inner, &inner);
            }
        }
        StmtKind::ClassDef {
            name,
            bases,
            body,
            decorator_list,
        } => {
            rename_str(name, here);
            for d in decorator_list.iter_mut() {
                walk_expr(d, here, nested);
            }
            for b in bases.iter_mut() {
                walk_expr(b, here, nested);
            }
            let body_r = class_body_renames(nested, body);
            walk_stmts(body, &body_r, nested);
        }
        StmtKind::Return(e) => {
            if let Some(e) = e {
                walk_expr(e, here, nested);
            }
        }
        StmtKind::If {
            test,
            body,
            elif_clauses,
            else_body,
        } => {
            walk_expr(test, here, nested);
            walk_stmts(body, here, nested);
            for (t, b) in elif_clauses.iter_mut() {
                walk_expr(t, here, nested);
                walk_stmts(b, here, nested);
            }
            walk_opt_stmts(else_body, here, nested);
        }
        StmtKind::While {
            test,
            body,
            else_body,
        } => {
            walk_expr(test, here, nested);
            walk_stmts(body, here, nested);
            walk_opt_stmts(else_body, here, nested);
        }
        StmtKind::For {
            target,
            iter,
            body,
            else_body,
            ..
        } => {
            walk_expr(target, here, nested);
            walk_expr(iter, here, nested);
            walk_stmts(body, here, nested);
            walk_opt_stmts(else_body, here, nested);
        }
        StmtKind::Import { names } => {
            for a in names.iter_mut() {
                let bound = a
                    .alias
                    .clone()
                    .unwrap_or_else(|| a.name.split('.').next().unwrap_or("").to_string());
                if let Some(new) = here.get(&bound) {
                    if a.alias.is_some() || !a.name.contains('.') {
                        a.alias = Some(new.clone());
                    }
                    // `import a.b` with no alias binds the top package `a`;
                    // aliasing would bind the SUBMODULE — left as is (documented).
                }
            }
        }
        StmtKind::ImportFrom { names, .. } => {
            for a in names.iter_mut() {
                let bound = a.alias.clone().unwrap_or_else(|| a.name.clone());
                if let Some(new) = here.get(&bound) {
                    a.alias = Some(new.clone());
                }
            }
        }
        StmtKind::Try {
            body,
            handlers,
            else_body,
            finally_body,
        } => {
            walk_stmts(body, here, nested);
            for h in handlers.iter_mut() {
                if let Some(t) = &mut h.exc_type {
                    walk_expr(t, here, nested);
                }
                if let Some(n) = &mut h.name {
                    rename_str(n, here);
                }
                walk_stmts(&mut h.body, here, nested);
            }
            walk_opt_stmts(else_body, here, nested);
            walk_opt_stmts(finally_body, here, nested);
        }
        StmtKind::Raise(a, b) => {
            if let Some(a) = a {
                walk_expr(a, here, nested);
            }
            if let Some(b) = b {
                walk_expr(b, here, nested);
            }
        }
        StmtKind::Assert { test, msg } => {
            walk_expr(test, here, nested);
            if let Some(m) = msg {
                walk_expr(m, here, nested);
            }
        }
        StmtKind::Del(targets) => {
            for t in targets.iter_mut() {
                walk_expr(t, here, nested);
            }
        }
        StmtKind::With { items, body, .. } => {
            for it in items.iter_mut() {
                walk_expr(&mut it.context_expr, here, nested);
                if let Some(v) = &mut it.optional_var {
                    walk_expr(v, here, nested);
                }
            }
            walk_stmts(body, here, nested);
        }
        StmtKind::Match { subject, cases } => {
            walk_expr(subject, here, nested);
            for c in cases.iter_mut() {
                walk_pattern(&mut c.pattern, here, nested);
                if let Some(g) = &mut c.guard {
                    walk_expr(g, here, nested);
                }
                walk_stmts(&mut c.body, here, nested);
            }
        }
        // `global N` is the trigger — never rewritten. `nonlocal N` names the
        // enclosing binding, which is exactly what got renamed.
        StmtKind::Global(_) => {}
        StmtKind::Nonlocal(names) => {
            for n in names.iter_mut() {
                rename_str(n, here);
            }
        }
        StmtKind::Break | StmtKind::Continue | StmtKind::Pass | StmtKind::ImportSideEffect(_) => {}
    }
}

fn walk_pattern(p: &mut Pattern, here: &Renames, nested: &Renames) {
    match p {
        Pattern::Wildcard => {}
        Pattern::Capture(n) => rename_str(n, here),
        Pattern::Literal(e) | Pattern::Value(e) => walk_expr(e, here, nested),
        Pattern::Class { cls, args } => {
            // `case abs(...)`: the head of the (possibly dotted) class name is
            // a reference.
            if let Some((head, rest)) = cls.split_once('.') {
                if let Some(new) = here.get(head) {
                    *cls = format!("{new}.{rest}");
                }
            } else {
                rename_str(cls, here);
            }
            for a in args.iter_mut() {
                walk_pattern(a, here, nested);
            }
        }
        Pattern::Sequence(ps) | Pattern::Or(ps) => {
            for q in ps.iter_mut() {
                walk_pattern(q, here, nested);
            }
        }
        Pattern::Mapping(items) => {
            for (k, q) in items.iter_mut() {
                walk_expr(k, here, nested);
                walk_pattern(q, here, nested);
            }
        }
        Pattern::As { pattern, name } => {
            walk_pattern(pattern, here, nested);
            rename_str(name, here);
        }
        Pattern::Star(n) => {
            if let Some(n) = n {
                rename_str(n, here);
            }
        }
    }
}

fn target_names(e: &Expr, out: &mut HashSet<String>) {
    match &e.kind {
        ExprKind::Name(n) => {
            out.insert(n.clone());
        }
        ExprKind::Tuple(elts) | ExprKind::List(elts) => {
            for x in elts {
                target_names(x, out);
            }
        }
        ExprKind::Starred(inner) => target_names(inner, out),
        _ => {}
    }
}

/// A comprehension is its own function scope: the LEFTMOST iterable is
/// evaluated in the enclosing scope; every other part sees the comprehension's
/// targets first, then the enclosing function's locals.
fn walk_comprehension(
    generators: &mut [Comprehension],
    elts: &mut [&mut Expr],
    here: &Renames,
    nested: &Renames,
) {
    if let Some(first) = generators.first_mut() {
        walk_expr(&mut first.iter, here, nested);
    }
    let mut targets = HashSet::new();
    for g in generators.iter() {
        target_names(&g.target, &mut targets);
    }
    let inner = minus(nested, |n| targets.contains(n));
    for (i, g) in generators.iter_mut().enumerate() {
        if i > 0 {
            walk_expr(&mut g.iter, &inner, &inner);
        }
        walk_expr(&mut g.target, &inner, &inner);
        for c in g.ifs.iter_mut() {
            walk_expr(c, &inner, &inner);
        }
    }
    for e in elts.iter_mut() {
        walk_expr(e, &inner, &inner);
    }
}

fn walk_expr(e: &mut Expr, here: &Renames, nested: &Renames) {
    match &mut e.kind {
        ExprKind::Name(n) => rename_str(n, here),
        ExprKind::IntLiteral(_)
        | ExprKind::FloatLiteral(_)
        | ExprKind::ImagLiteral(_)
        | ExprKind::StringLiteral(_)
        | ExprKind::BytesLiteral(_)
        | ExprKind::BoolLiteral(_)
        | ExprKind::NoneLiteral => {}
        ExprKind::FString { parts } => {
            for p in parts.iter_mut() {
                if let FStringPart::Expr(x) = p {
                    walk_expr(x, here, nested);
                }
            }
        }
        ExprKind::BinOp { left, right, .. } => {
            walk_expr(left, here, nested);
            walk_expr(right, here, nested);
        }
        ExprKind::UnaryOp { operand, .. } => walk_expr(operand, here, nested),
        ExprKind::Compare { left, comparisons } => {
            walk_expr(left, here, nested);
            for (_, r) in comparisons.iter_mut() {
                walk_expr(r, here, nested);
            }
        }
        ExprKind::Call {
            func, args, kwargs, ..
        } => {
            walk_expr(func, here, nested);
            for a in args.iter_mut() {
                walk_expr(a, here, nested);
            }
            for k in kwargs.iter_mut() {
                // `k.name` is the CALLEE's parameter name — never a variable.
                walk_expr(&mut k.value, here, nested);
            }
        }
        ExprKind::Attribute { value, .. } => walk_expr(value, here, nested),
        ExprKind::Subscript { value, index, .. } => {
            walk_expr(value, here, nested);
            walk_expr(index, here, nested);
        }
        ExprKind::Slice { lower, upper, step } => {
            for x in [lower, upper, step].into_iter().flatten() {
                walk_expr(x, here, nested);
            }
        }
        ExprKind::List(elts) | ExprKind::Tuple(elts) | ExprKind::Set(elts) => {
            for x in elts.iter_mut() {
                walk_expr(x, here, nested);
            }
        }
        ExprKind::Dict { items } => {
            for it in items.iter_mut() {
                match it {
                    DictItem::KeyValue { key, value } => {
                        walk_expr(key, here, nested);
                        walk_expr(value, here, nested);
                    }
                    DictItem::Spread(x) => walk_expr(x, here, nested),
                }
            }
        }
        ExprKind::ListComp { elt, generators }
        | ExprKind::SetComp { elt, generators }
        | ExprKind::GeneratorExp { elt, generators } => {
            walk_comprehension(generators, &mut [elt.as_mut()], here, nested);
        }
        ExprKind::DictComp {
            key,
            value,
            generators,
        } => {
            walk_comprehension(
                generators,
                &mut [key.as_mut(), value.as_mut()],
                here,
                nested,
            );
        }
        ExprKind::Lambda { params, body } => {
            for p in params.iter_mut() {
                if let Some(d) = &mut p.default {
                    walk_expr(d, here, nested);
                }
            }
            let names: HashSet<&str> = params.iter().map(|p| p.name.as_str()).collect();
            let inner = minus(nested, |n| names.contains(n));
            walk_expr(body, &inner, &inner);
        }
        ExprKind::IfExpr {
            test,
            body,
            else_body,
        } => {
            walk_expr(test, here, nested);
            walk_expr(body, here, nested);
            walk_expr(else_body, here, nested);
        }
        ExprKind::Starred(x) | ExprKind::Await(x) | ExprKind::YieldFrom(x) => {
            walk_expr(x, here, nested)
        }
        ExprKind::Yield(x) => {
            if let Some(x) = x {
                walk_expr(x, here, nested);
            }
        }
        ExprKind::NamedExpr { target, value } => {
            walk_expr(target, here, nested);
            walk_expr(value, here, nested);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rename(src: &str) -> Option<Module> {
        let m = pyths_parser::parse(src).expect("parse");
        rename_captured_globals(&m).module
    }

    /// #491 SF2: the exact unrenamable shape — a function-local `import a.b`
    /// (no alias) whose head collides with a nested `global a` — is REFUSED
    /// with a diagnostic naming the import, the function, and the fix; the
    /// aliased twin (`import os.path as p`) and a dotted import in a function
    /// with no nested `global` are NOT refused (over-fix guard).
    #[test]
    fn dotted_import_head_collision_is_refused() {
        let src = "def f():\n    os = 1\n    import os.path\n    def g():\n        global os\n        os = 7\n    g()\n    return os\n";
        let m = pyths_parser::parse(src).expect("parse");
        let r = rename_captured_globals(&m);
        assert_eq!(r.refused.len(), 1, "exactly one refusal: {:?}", r.refused);
        let d = &r.refused[0];
        assert!(
            d.contains("`import os.path` inside `f`")
                && d.contains("global os")
                && d.contains("as <name>"),
            "diagnostic must name the import, the function and the fix: {d}"
        );
        // Aliased twin: renamable, not refused.
        let src = "def f():\n    os = 1\n    import os.path as p\n    def g():\n        global os\n        os = 7\n    g()\n    return (os, p)\n";
        let m = pyths_parser::parse(src).expect("parse");
        let r = rename_captured_globals(&m);
        assert!(
            r.refused.is_empty(),
            "aliased dotted import must not be refused: {:?}",
            r.refused
        );
        assert!(r.module.is_some(), "the local `os` still renames");
        // No nested `global` at all: nothing to refuse, nothing to rename.
        let src = "def f():\n    import os.path\n    return os\n";
        let m = pyths_parser::parse(src).expect("parse");
        let r = rename_captured_globals(&m);
        assert!(r.refused.is_empty() && r.module.is_none());
        // The head is bound ONLY by the dotted import (no census local, so no
        // rename collision at all) — still the silent shape, still refused.
        let src = "def outer():\n    import os.path\n    def inner():\n        global os\n        os = 7\n    inner()\n    return type(os).__name__\n";
        let m = pyths_parser::parse(src).expect("parse");
        let r = rename_captured_globals(&m);
        assert_eq!(
            r.refused.len(),
            1,
            "import-only head must be refused too: {:?}",
            r.refused
        );
        assert!(r.refused[0].contains("`import os.path` inside `outer`"));
        // The collision lives in a NESTED def that binds its own `os`: not F's shape.
        let src = "def f():\n    os = 1\n    def h():\n        import os.path\n        def g():\n            global os\n            os = 7\n        return os\n    return os\n";
        let m = pyths_parser::parse(src).expect("parse");
        let r = rename_captured_globals(&m);
        assert_eq!(
            r.refused.len(),
            1,
            "the refusal is attributed to `h`, the importing function: {:?}",
            r.refused
        );
        assert!(r.refused[0].contains("inside `h`"));
    }

    fn names(m: &Module) -> Vec<String> {
        // every identifier that carries the tag, in source order
        let mut out = Vec::new();
        fn stmts(ss: &[Stmt], out: &mut Vec<String>) {
            for s in ss {
                match &s.kind {
                    StmtKind::FuncDef {
                        name, params, body, ..
                    } => {
                        if name.contains(TAG) {
                            out.push(name.clone());
                        }
                        for p in params {
                            if p.name.contains(TAG) {
                                out.push(p.name.clone());
                            }
                        }
                        stmts(body, out);
                    }
                    StmtKind::ClassDef { body, .. } => stmts(body, out),
                    StmtKind::Assign { targets, value } => {
                        for t in targets {
                            expr(t, out);
                        }
                        expr(value, out);
                    }
                    StmtKind::Return(Some(e)) | StmtKind::Expr(e) => expr(e, out),
                    StmtKind::Nonlocal(ns) | StmtKind::Global(ns) => {
                        for n in ns {
                            if n.contains(TAG) {
                                out.push(n.clone());
                            }
                        }
                    }
                    _ => {}
                }
            }
        }
        fn expr(e: &Expr, out: &mut Vec<String>) {
            match &e.kind {
                ExprKind::Name(n) => {
                    if n.contains(TAG) {
                        out.push(n.clone());
                    }
                }
                ExprKind::Call { func, args, .. } => {
                    expr(func, out);
                    for a in args {
                        expr(a, out);
                    }
                }
                ExprKind::Tuple(xs) => {
                    for x in xs {
                        expr(x, out);
                    }
                }
                ExprKind::BinOp { left, right, .. } => {
                    expr(left, out);
                    expr(right, out);
                }
                _ => {}
            }
        }
        stmts(&m.body, &mut out);
        out
    }

    #[test]
    fn python_name_round_trip() {
        assert_eq!(python_name("abs$l0"), "abs");
        assert_eq!(python_name("abs$l12"), "abs");
        assert_eq!(python_name("abs"), "abs");
        assert_eq!(python_name("default$"), "default$");
        assert_eq!(python_name("__result$"), "__result$");
        assert_eq!(python_name("$l1"), "$l1");
    }

    #[test]
    fn no_collision_is_a_no_op() {
        // OVER-FIX GUARD: a genuine enclosing local with no nested `global`
        // keeps its name — the pass returns None (byte-identical emission).
        assert!(rename(
            "def outer():\n    abs = 1\n    def inner():\n        return abs\n    return inner()\n"
        )
        .is_none());
        // a `global` only in the enclosing function itself is not a collision
        assert!(rename("def f():\n    global abs\n    abs = 7\n").is_none());
        // module-level `global` is not inside a function
        assert!(rename("global abs\nabs = 1\n").is_none());
    }

    #[test]
    fn item3_renames_only_outer_local() {
        let m = rename(
            "def outer():\n    abs = 1\n    def inner():\n        global abs\n        abs = 7\n    inner()\n    return abs\nprint(outer())\nprint(abs)\n",
        )
        .expect("collision");
        // outer's binding + outer's read renamed; inner's write NOT; module untouched
        assert_eq!(names(&m), vec!["abs$l0", "abs$l0"]);
    }

    #[test]
    fn param_and_nonlocal_and_two_level() {
        let m = rename(
            "def outer(abs):\n    def mid():\n        abs = 2\n        def inner():\n            global abs\n            abs = 7\n        def sib():\n            nonlocal abs\n            abs = abs + 1\n        return abs\n    return abs\n",
        )
        .expect("collision");
        // outer's param → $l0 (its `return abs` too); mid's local → $l1 (its
        // return, sib's nonlocal + read/write); inner's global write untouched.
        assert_eq!(
            names(&m),
            vec!["abs$l0", "abs$l1", "abs$l1", "abs$l1", "abs$l1", "abs$l1", "abs$l0"]
        );
    }

    #[test]
    fn class_body_rules() {
        // class body binds `abs` → its direct statements are class-attribute /
        // LOAD_NAME accesses (untouched); the METHOD still sees outer's local.
        let m = rename(
            "def outer():\n    abs = 1\n    class C:\n        y = abs\n        abs = 2\n        def m(self):\n            return abs\n    def inner():\n        global abs\n        abs = 7\n    return abs\n",
        )
        .expect("collision");
        assert_eq!(names(&m), vec!["abs$l0", "abs$l0", "abs$l0"]);
        // class body declares `global abs` → its write is the module's; the
        // method still resolves to outer's local (CPython symtable: class
        // namespace has no effect on nested scopes).
        let m = rename(
            "def outer():\n    abs = 1\n    class C:\n        global abs\n        abs = 7\n        def m(self):\n            return abs\n    return abs\n",
        )
        .expect("collision");
        assert_eq!(names(&m), vec!["abs$l0", "abs$l0", "abs$l0"]);
    }
}
