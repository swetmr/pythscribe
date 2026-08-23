//! Filesystem-aware relative-import normalization: wildcard expansion
//! (`from .mod import *`) and `from . import X` submodule-vs-symbol
//! disambiguation.
//!
//! ROOT FIX for the silent-drop miscompile: the emitter used to emit NOTHING
//! for a relative star import — a clean compile whose names later exploded as
//! bare `ReferenceError`s at runtime with no hint of the cause.
//!
//! This module is the ONE owner of the behavior. Every CLI entry point that
//! parses a source file for codegen (compile / run / bundle / test / check)
//! calls [`normalize_relative_imports`] immediately after parse, so the
//! emitter never sees a relative star. Unlike bare/npm stars, the RELATIVE
//! form's target is locatable on disk (relative to the importing file), so
//! the public name set is knowable at compile time: we parse the sibling,
//! collect its star-visible names (respecting `__all__`, else all
//! non-underscore top-level `def`/`class`/assignment names, chasing nested
//! relative-import chains), and rewrite the star into the equivalent explicit
//! named import(s) the emitter already lowers correctly.
//!
//! The emitter itself carries a HARD-ERROR backstop for any context that
//! bypasses this pass (direct library consumers), so the silent drop is
//! impossible on every path: CLI paths get full support, everything else
//! fails loud at compile time.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

use pyths_syntax::ast::{Expr, ExprKind, ImportAlias, Module, Stmt, StmtKind};

/// Cheap textual pre-check used by `compile` to disable the single-file
/// incremental cache: BOTH normalizations read SIBLING filesystem state the
/// cache key (this file's source + target + compiler version) does not cover
/// — a relative star reads the sibling's source, and the `from . import X`
/// disambiguation depends on whether a submodule file exists — so a cache
/// hit could serve a stale artifact after only the sibling changed.
/// Conservative over-approximation (a false positive merely skips the cache).
pub fn may_depend_on_siblings(source: &str) -> bool {
    // ANY relative import (`from .`, `from ..pkg`, `from .pkg import name`)
    // now reads sibling filesystem state during normalization: the
    // submodule-vs-index-symbol disambiguation probes for submodule files AND
    // parses the package `__init__` to model CPython's precedence. So a cache
    // key covering only this file's own source could serve a stale artifact
    // after only a sibling changed — disable the incremental cache whenever a
    // relative from-import is present. Conservative over-approximation (a false
    // positive merely skips the cache).
    let bytes = source.as_bytes();
    let mut i = 0usize;
    while let Some(pos) = source[i..].find("from ") {
        let start = i + pos + 5;
        // Skip any leading whitespace between `from` and the module ref.
        let mut j = start;
        while j < source.len() && (bytes[j] == b' ' || bytes[j] == b'\t') {
            j += 1;
        }
        // A leading dot marks a relative import (`from .`, `from ..a`, …).
        if j < source.len() && bytes[j] == b'.' {
            return true;
        }
        i = start;
    }
    false
}

/// True iff the statement is a top-level relative star import.
fn is_relative_star(stmt: &Stmt) -> bool {
    matches!(
        &stmt.kind,
        StmtKind::ImportFrom { names, level, .. }
            if *level > 0 && names.len() == 1 && names[0].name == "*"
    )
}

/// True iff the statement is a leading-dot-only import (`from . import x`,
/// `from .. import y`) — the AMBIGUOUS form: each name is either a sibling
/// SUBMODULE (needs a namespace import of the submodule file) or a SYMBOL
/// defined in the package `__init__` (needs a named import from the index).
/// Only the filesystem can tell them apart, so the CLI pre-pass does.
fn is_dot_only_import(stmt: &Stmt) -> bool {
    matches!(
        &stmt.kind,
        StmtKind::ImportFrom { module, names, level }
            if *level > 0
                && module.is_empty()
                && !(names.len() == 1 && names[0].name == "*")
    )
}

/// True iff the statement is a NAMED relative import with a non-empty module
/// (`from .pkg import name`, `from ..a.b import x`) — the form where an
/// imported name may itself be a SUBMODULE of the package (needs a namespace
/// import of the submodule file) rather than a symbol of the package index
/// (needs a named import). Only the filesystem can tell them apart.
fn is_named_relative_import(stmt: &Stmt) -> bool {
    matches!(
        &stmt.kind,
        StmtKind::ImportFrom { module, names, level }
            if *level > 0
                && !module.is_empty()
                && !(names.len() == 1 && names[0].name == "*")
    )
}

/// Normalize the relative imports of `module` (whose source file is
/// `source_path`) against the filesystem — the ONE CLI-owned pre-pass every
/// parse-for-codegen entry point runs:
///
/// 1. `from .mod import *` expands into explicit named imports (see the
///    module doc above).
/// 2. `from . import X` is DISAMBIGUATED per name: if a submodule file
///    `./X.ps|.py|X/__init__` exists it stays in the leading-dot-only form
///    (the emitter lowers it to `import * as X from "./X"`); otherwise `X`
///    is a symbol of the package `__init__` and the statement is rewritten
///    with the module sentinel `"."`, which the emitter lowers to a NAMED
///    import from the index (`import { X } from "./"`). The sentinel is
///    unreachable from the parser (leading dots parse into `level`), so it
///    is exclusively this pass's contract with the emitter.
///
/// No-op when the module has neither form. Errors are loud, human-readable
/// strings.
pub fn normalize_relative_imports(module: &mut Module, source_path: &Path) -> Result<(), String> {
    if !module
        .body
        .iter()
        .any(|s| is_relative_star(s) || is_dot_only_import(s) || is_named_relative_import(s))
    {
        return Ok(());
    }
    let source_dir = source_path
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .map(Path::to_path_buf)
        .unwrap_or_else(|| PathBuf::from("."));
    let source_dir = source_dir.canonicalize().map_err(|e| {
        format!(
            "cannot resolve directory of {}: {}",
            source_path.display(),
            e
        )
    })?;

    let body = std::mem::take(&mut module.body);
    let mut new_body: Vec<Stmt> = Vec::with_capacity(body.len());
    for stmt in body {
        // FIX 2 — `from . import X` disambiguation (submodule vs
        // package-index symbol), by filesystem probe.
        if is_dot_only_import(&stmt) {
            let (names, level, span) = match &stmt.kind {
                StmtKind::ImportFrom { names, level, .. } => (names.clone(), *level, stmt.span),
                _ => unreachable!(),
            };
            let mut submodules: Vec<ImportAlias> = Vec::new();
            let mut symbols: Vec<ImportAlias> = Vec::new();
            for a in names {
                // CPython precedence: a name DEFINED in the package `__init__`
                // WINS over a same-named submodule file (getattr(pkg, name)
                // returns the __init__ attribute, so the submodule is never
                // imported). Only when __init__ does NOT bind the name is it a
                // submodule.
                if package_defines_symbol(&source_dir, level, "", &a.name) {
                    symbols.push(a);
                } else if resolve_relative_module(&source_dir, level, &a.name).is_some() {
                    submodules.push(a);
                } else {
                    symbols.push(a);
                }
            }
            if !submodules.is_empty() {
                new_body.push(Stmt::new(
                    StmtKind::ImportFrom {
                        module: String::new(),
                        names: submodules,
                        level,
                    },
                    span,
                ));
            }
            if !symbols.is_empty() {
                new_body.push(Stmt::new(
                    StmtKind::ImportFrom {
                        module: ".".to_string(),
                        names: symbols,
                        level,
                    },
                    span,
                ));
            }
            continue;
        }
        // FIX (Issue B) — `from .pkg import name` where `name` is a SUBMODULE of
        // `pkg` (a level or more deep). The emitter lowers a non-empty-module
        // named relative import to `import { name } from "./pkg"`, asking pkg's
        // index for a named export it does not provide (the submodule file is
        // never re-exported) → ESM link error. The emitter has no filesystem
        // access to tell a submodule from an index symbol; this CLI pre-pass
        // does. Split the imported names: a genuine submodule becomes a
        // MODULE-NAMESPACE import of the submodule FILE (the leading-dot-only
        // form the emitter lowers to `import * as name from "./pkg/name"`),
        // while an index symbol stays a named import from pkg. CPython
        // precedence again applies: an index-defined name wins over a submodule.
        if is_named_relative_import(&stmt) {
            let (module, names, level, span) = match &stmt.kind {
                StmtKind::ImportFrom {
                    module,
                    names,
                    level,
                } => (module.clone(), names.clone(), *level, stmt.span),
                _ => unreachable!(),
            };
            let mut submodule_ns: Vec<ImportAlias> = Vec::new();
            let mut kept: Vec<ImportAlias> = Vec::new();
            for a in names {
                let dotted = format!("{}.{}", module, a.name);
                if package_defines_symbol(&source_dir, level, &module, &a.name) {
                    // A symbol of pkg's index → keep the working named import.
                    kept.push(a);
                } else if resolve_relative_module(&source_dir, level, &dotted).is_some() {
                    // A submodule FILE → namespace import of the file itself.
                    // Encode the full relative path in the alias name; the
                    // emitter's leading-dot-only branch builds the specifier as
                    // `./<prefix><name>`, giving `import * as <local> from
                    // "./pkg/name"`.
                    let local = a.alias.clone().unwrap_or_else(|| a.name.clone());
                    submodule_ns.push(ImportAlias {
                        name: format!("{}/{}", module.replace('.', "/"), a.name),
                        alias: Some(local),
                    });
                } else {
                    // Neither an index symbol nor a submodule file — keep it a
                    // named import; a genuinely missing name fails loud at ESM
                    // link time (same as before).
                    kept.push(a);
                }
            }
            if !submodule_ns.is_empty() {
                new_body.push(Stmt::new(
                    StmtKind::ImportFrom {
                        module: String::new(),
                        names: submodule_ns,
                        level,
                    },
                    span,
                ));
            }
            if !kept.is_empty() {
                new_body.push(Stmt::new(
                    StmtKind::ImportFrom {
                        module,
                        names: kept,
                        level,
                    },
                    span,
                ));
            }
            continue;
        }
        if !is_relative_star(&stmt) {
            new_body.push(stmt);
            continue;
        }
        let (mod_name, level, span) = match &stmt.kind {
            StmtKind::ImportFrom { module, level, .. } => (module.clone(), *level, stmt.span),
            _ => unreachable!(),
        };
        let py_form = format!("from {}{} import *", ".".repeat(level as usize), mod_name);
        let target = resolve_relative_module(&source_dir, level, &mod_name).ok_or_else(|| {
            format!(
                "{}: cannot expand `{}` — no module file found for `{}{}` \
                 (looked for .ps/.py and a package __init__ relative to {})",
                source_path.display(),
                py_form,
                ".".repeat(level as usize),
                mod_name,
                source_dir.display()
            )
        })?;

        let mut visited: HashSet<PathBuf> = HashSet::new();
        let publics = collect_star_visible(&target, &mut visited).map_err(|e| {
            format!(
                "{}: cannot expand `{}`: {}",
                source_path.display(),
                py_form,
                e
            )
        })?;

        if !publics.unsupported.is_empty() {
            eprintln!(
                "warning: `{}` ({}): the following star-visible names are bound by \
                 absolute imports or submodule bindings inside {} and cannot be \
                 re-imported through the star — import them explicitly if needed: {}",
                py_form,
                source_path.display(),
                target.display(),
                publics.unsupported.join(", ")
            );
        }

        // The star target itself must still EXECUTE (Python runs the module
        // even when it contributes no names, and side effects must fire in
        // import order). Emit a side-effect import of the target first; ESM
        // module caching keeps it single-execution.
        let target_spec = js_relative_spec(level, &mod_name);
        new_body.push(Stmt::new(StmtKind::ImportSideEffect(target_spec), span));

        // Group the names by DEFINING module (a name pulled through a chained
        // relative import must be imported from the module that actually
        // exports it — compiled ESM does not re-export imported bindings),
        // preserving first-appearance order.
        let mut group_order: Vec<(u32, String)> = Vec::new();
        let mut groups: HashMap<(u32, String), Vec<ImportAlias>> = HashMap::new();
        for entry in &publics.names {
            let (glevel, gmodule) = relativize_module(&source_dir, &entry.defining_file)
                .ok_or_else(|| {
                    format!(
                        "{}: cannot expand `{}` — `{}` is defined in {}, which is not \
                         reachable via a relative import from {}; import it explicitly",
                        source_path.display(),
                        py_form,
                        entry.public,
                        entry.defining_file.display(),
                        source_dir.display()
                    )
                })?;
            if gmodule.is_empty() {
                // The defining module is THIS file's own package index —
                // `from . import <symbol>` lowers as a submodule namespace
                // import, so a symbol import cannot be spelled. Rare
                // (star from the importer's own package); fail loud.
                return Err(format!(
                    "{}: cannot expand `{}` — `{}` is defined in this package's own \
                     __init__ ({}); importing a package's own symbols via a star is \
                     not supported — import them explicitly",
                    source_path.display(),
                    py_form,
                    entry.public,
                    entry.defining_file.display()
                ));
            }
            let key = (glevel, gmodule);
            if !groups.contains_key(&key) {
                group_order.push(key.clone());
            }
            groups.entry(key).or_default().push(ImportAlias {
                name: entry.original.clone(),
                alias: if entry.original == entry.public {
                    None
                } else {
                    Some(entry.public.clone())
                },
            });
        }
        for key in group_order {
            let names = groups.remove(&key).expect("group recorded in order list");
            let (glevel, gmodule) = key;
            new_body.push(Stmt::new(
                StmtKind::ImportFrom {
                    module: gmodule,
                    names,
                    level: glevel,
                },
                span,
            ));
        }
    }
    module.body = new_body;
    Ok(())
}

/// The extensionless JS specifier the emitter's relative-import convention
/// produces for a `(level, module)` pair — used for the side-effect import.
fn js_relative_spec(level: u32, module: &str) -> String {
    let prefix = "../".repeat((level - 1) as usize);
    if module.is_empty() {
        // package index of the level-th parent
        let spec = format!("./{}", prefix);
        spec.trim_end_matches('/').to_string() + "/"
    } else {
        format!("./{}{}", prefix, module.replace('.', "/"))
    }
}

/// One star-visible name: public name in the importing scope, the file that
/// actually defines (and therefore exports) it, and its name in that file.
struct StarName {
    public: String,
    defining_file: PathBuf,
    original: String,
}

struct StarVisible {
    /// Insertion-ordered star-visible names (first binding keeps its slot,
    /// later rebinds update in place — module namespaces are dicts).
    names: Vec<StarName>,
    /// Star-visible in Python but not expandable (bound by absolute imports
    /// or submodule namespace bindings in the target).
    unsupported: Vec<String>,
}

/// The top-level bound names of a module file — every name that
/// `getattr(module, name)` would find after the module runs: `def`/`class`
/// definitions, assignment targets, and names bound by imports. Used to model
/// CPython's `from pkg import X` precedence: a name DEFINED in a package's
/// `__init__` shadows a same-named submodule (`getattr(pkg, X)` returns the
/// `__init__` attribute, so the submodule file is never imported). Best-effort:
/// a file that cannot be read/parsed contributes nothing (the caller then
/// falls back to the submodule probe, i.e. the prior behavior).
fn top_level_binds(file: &Path) -> HashSet<String> {
    let mut out = HashSet::new();
    let Ok(src) = std::fs::read_to_string(file) else {
        return out;
    };
    let Ok(parsed) = pyths_parser::parse(&src) else {
        return out;
    };
    for stmt in &parsed.body {
        match &stmt.kind {
            StmtKind::FuncDef { name, .. } | StmtKind::ClassDef { name, .. } => {
                out.insert(name.clone());
            }
            StmtKind::Assign { targets, .. } => {
                for t in targets {
                    let mut names = Vec::new();
                    collect_target_names(t, &mut names);
                    out.extend(names);
                }
            }
            StmtKind::AnnAssign { target, .. } | StmtKind::AugAssign { target, .. } => {
                let mut names = Vec::new();
                collect_target_names(target, &mut names);
                out.extend(names);
            }
            StmtKind::ImportFrom {
                module,
                names,
                level,
            } => {
                // A leading-dot-only relative import (`from . import a`) binds a
                // SUBMODULE — it does NOT create an index symbol that shadows a
                // same-named submodule for `from pkg import a` precedence: it IS
                // that submodule. Skip it, so a package `__init__` that itself
                // does `from . import a` is not mistaken for defining `a` as a
                // shadowing symbol (which would wrongly reroute `a` to the index
                // instead of the submodule file).
                if *level > 0 && module.is_empty() {
                    continue;
                }
                for a in names {
                    if a.name != "*" {
                        out.insert(a.alias.clone().unwrap_or_else(|| a.name.clone()));
                    }
                }
            }
            StmtKind::Import { names } => {
                for a in names {
                    // `import pkg.sub` binds the top name `pkg`; `import x as y`
                    // binds `y`.
                    let bound = match &a.alias {
                        Some(alias) => alias.clone(),
                        None => a.name.split('.').next().unwrap_or(&a.name).to_string(),
                    };
                    out.insert(bound);
                }
            }
            _ => {}
        }
    }
    out
}

/// CPython precedence probe: does `from {level dots}{module} import {name}`
/// bind a SYMBOL defined in that package's index (`__init__`) rather than a
/// same-named submodule file? `module` empty → the current package's own
/// `__init__`. When true, the index binding wins over any submodule of the
/// same name (the submodule file must NOT be imported), matching CPython's
/// `getattr(pkg, name)` resolution order.
fn package_defines_symbol(dir: &Path, level: u32, module: &str, name: &str) -> bool {
    match resolve_relative_module(dir, level, module) {
        Some(index_file) => top_level_binds(&index_file).contains(name),
        None => false,
    }
}

/// Resolve `(level, dotted-module)` relative to `dir` to a module file:
/// `<mod>.ps` / `<mod>.py` / `<mod>/__init__.ps` / `<mod>/__init__.py`
/// (empty module → the climbed directory's own `__init__`).
fn resolve_relative_module(dir: &Path, level: u32, module: &str) -> Option<PathBuf> {
    let mut base = dir.to_path_buf();
    for _ in 1..level {
        base = base.parent()?.to_path_buf();
    }
    let candidates: Vec<PathBuf> = if module.is_empty() {
        vec![base.join("__init__.ps"), base.join("__init__.py")]
    } else {
        let rel = module.replace('.', "/");
        vec![
            base.join(format!("{}.ps", rel)),
            base.join(format!("{}.py", rel)),
            base.join(&rel).join("__init__.ps"),
            base.join(&rel).join("__init__.py"),
        ]
    };
    for c in candidates {
        if c.is_file() {
            return c.canonicalize().ok();
        }
    }
    None
}

/// Express `target` (a module file) as a `(level, dotted-module)` relative
/// import from `from_dir`. A package `__init__` is addressed by its package
/// directory. Returns `None` when the target is not under any ancestor of
/// `from_dir` (not spellable as a relative import).
fn relativize_module(from_dir: &Path, target: &Path) -> Option<(u32, String)> {
    let is_init = target.file_stem().is_some_and(|s| s == "__init__");
    let logical: PathBuf = if is_init {
        target.parent()?.to_path_buf()
    } else {
        target.with_extension("")
    };
    let mut base = from_dir.to_path_buf();
    let mut level: u32 = 1;
    loop {
        if let Ok(rest) = logical.strip_prefix(&base) {
            let parts: Vec<String> = rest
                .components()
                .map(|c| c.as_os_str().to_string_lossy().into_owned())
                .collect();
            return Some((level, parts.join(".")));
        }
        base = base.parent()?.to_path_buf();
        level += 1;
    }
}

/// Collect target names of an assignment-target expression (Name / nested
/// Tuple / List / Starred).
fn collect_target_names(e: &Expr, out: &mut Vec<String>) {
    match &e.kind {
        ExprKind::Name(n) => out.push(n.clone()),
        ExprKind::Tuple(elts) | ExprKind::List(elts) => {
            for x in elts {
                collect_target_names(x, out);
            }
        }
        ExprKind::Starred(inner) => collect_target_names(inner, out),
        _ => {}
    }
}

/// Parse a literal `__all__ = ["a", "b"]` list/tuple of string literals.
fn parse_all_list(value: &Expr) -> Option<Vec<String>> {
    let elts = match &value.kind {
        ExprKind::List(elts) | ExprKind::Tuple(elts) => elts,
        _ => return None,
    };
    let mut out = Vec::with_capacity(elts.len());
    for e in elts {
        match &e.kind {
            ExprKind::StringLiteral(s) => out.push(s.clone()),
            _ => return None,
        }
    }
    Some(out)
}

/// Collect the star-visible names of the module file at `file`:
/// - `__all__` (literal list of strings) wins when present;
/// - otherwise all non-underscore top-level `def` / `class` / assignment
///   names, plus names pulled in through the target's own RELATIVE imports
///   (named ones attribute to their defining module; star ones recurse).
///
/// Names bound by absolute imports / submodule bindings are reported as
/// `unsupported` (compiled ESM cannot re-import them through the sibling).
fn collect_star_visible(
    file: &Path,
    visited: &mut HashSet<PathBuf>,
) -> Result<StarVisible, String> {
    let canonical = file
        .canonicalize()
        .map_err(|e| format!("cannot open {}: {}", file.display(), e))?;
    if !visited.insert(canonical.clone()) {
        // Import cycle through star chains — contribute nothing further
        // (mirrors CPython's partially-initialized-module behavior).
        return Ok(StarVisible {
            names: Vec::new(),
            unsupported: Vec::new(),
        });
    }
    let source = std::fs::read_to_string(&canonical)
        .map_err(|e| format!("cannot read {}: {}", canonical.display(), e))?;
    let parsed = pyths_parser::parse(&source).map_err(|errors| {
        let msgs: Vec<String> = errors.iter().map(|e| e.message.clone()).collect();
        format!(
            "parse error in {}: {}",
            canonical.display(),
            msgs.join(", ")
        )
    })?;
    let dir = canonical
        .parent()
        .map(Path::to_path_buf)
        .unwrap_or_else(|| PathBuf::from("."));

    // Insertion-ordered name map: name → index into `names` (rebind updates
    // in place; a tombstone marks a name later shadowed by an unsupported
    // binding).
    let mut names: Vec<Option<StarName>> = Vec::new();
    let mut index: HashMap<String, usize> = HashMap::new();
    let mut unsupported: Vec<String> = Vec::new();
    let mut all_list: Option<Vec<String>> = None;

    let bind = |names: &mut Vec<Option<StarName>>,
                index: &mut HashMap<String, usize>,
                public: String,
                defining_file: PathBuf,
                original: String| {
        let entry = StarName {
            public: public.clone(),
            defining_file,
            original,
        };
        match index.get(&public) {
            Some(&i) => names[i] = Some(entry),
            None => {
                index.insert(public, names.len());
                names.push(Some(entry));
            }
        }
    };
    let bind_unsupported = |names: &mut Vec<Option<StarName>>,
                            index: &mut HashMap<String, usize>,
                            unsupported: &mut Vec<String>,
                            public: &str| {
        if let Some(&i) = index.get(public) {
            names[i] = None; // shadowed by an unexpandable binding
        }
        if !unsupported.iter().any(|n| n == public) {
            unsupported.push(public.to_string());
        }
    };

    for stmt in &parsed.body {
        match &stmt.kind {
            StmtKind::FuncDef { name, .. } | StmtKind::ClassDef { name, .. } => {
                bind(
                    &mut names,
                    &mut index,
                    name.clone(),
                    canonical.clone(),
                    name.clone(),
                );
            }
            StmtKind::Assign { targets, value } => {
                // `__all__ = [...]` — capture; also handle rebinds (last wins).
                if let [t] = targets.as_slice() {
                    if matches!(&t.kind, ExprKind::Name(n) if n == "__all__") {
                        all_list = parse_all_list(value);
                    }
                }
                let mut bound = Vec::new();
                for t in targets {
                    collect_target_names(t, &mut bound);
                }
                for n in bound {
                    bind(&mut names, &mut index, n.clone(), canonical.clone(), n);
                }
            }
            StmtKind::AnnAssign { target, .. } | StmtKind::AugAssign { target, .. } => {
                let mut bound = Vec::new();
                collect_target_names(target, &mut bound);
                for n in bound {
                    bind(&mut names, &mut index, n.clone(), canonical.clone(), n);
                }
            }
            StmtKind::ImportFrom {
                module,
                names: import_names,
                level,
            } => {
                if *level == 0 {
                    // Absolute imports bind names we cannot re-route.
                    for a in import_names {
                        if a.name == "*" {
                            continue;
                        }
                        let local = a.alias.as_deref().unwrap_or(&a.name);
                        bind_unsupported(&mut names, &mut index, &mut unsupported, local);
                    }
                    continue;
                }
                if import_names.len() == 1 && import_names[0].name == "*" {
                    // Chained relative star — recurse; the deeper module's
                    // star-visible set flows through with its own defining
                    // files (and its own __all__ filter already applied).
                    let deeper =
                        resolve_relative_module(&dir, *level, module).ok_or_else(|| {
                            format!(
                                "{}: no module file found for `from {}{} import *`",
                                canonical.display(),
                                ".".repeat(*level as usize),
                                module
                            )
                        })?;
                    let sub = collect_star_visible(&deeper, visited)?;
                    for s in sub.names {
                        bind(
                            &mut names,
                            &mut index,
                            s.public,
                            s.defining_file,
                            s.original,
                        );
                    }
                    for u in sub.unsupported {
                        bind_unsupported(&mut names, &mut index, &mut unsupported, &u);
                    }
                    continue;
                }
                if module.is_empty() {
                    // `from . import X` — same FS disambiguation as the
                    // normalize pass, with the SAME CPython precedence: a name
                    // defined in the package `__init__` is a SYMBOL of the index
                    // (a real named export → attributable) and WINS over a
                    // same-named submodule; only a name NOT defined in the index
                    // is a submodule namespace binding (compiled ESM does not
                    // re-export those → unsupported).
                    for a in import_names {
                        let local = a.alias.as_deref().unwrap_or(&a.name);
                        let idx_file = resolve_relative_module(&dir, *level, "");
                        let init_defines = idx_file
                            .as_ref()
                            .is_some_and(|f| top_level_binds(f).contains(&a.name));
                        if init_defines {
                            bind(
                                &mut names,
                                &mut index,
                                local.to_string(),
                                idx_file.expect("init_defines implies Some"),
                                a.name.clone(),
                            );
                        } else if resolve_relative_module(&dir, *level, &a.name).is_some() {
                            bind_unsupported(&mut names, &mut index, &mut unsupported, local);
                        } else if let Some(idx_file) = idx_file {
                            bind(
                                &mut names,
                                &mut index,
                                local.to_string(),
                                idx_file,
                                a.name.clone(),
                            );
                        } else {
                            bind_unsupported(&mut names, &mut index, &mut unsupported, local);
                        }
                    }
                    continue;
                }
                // Named relative import — attribute each name to ITS
                // defining module (the compiled sibling does not re-export
                // imported bindings, so the expansion must import from the
                // module that actually exports the symbol).
                match resolve_relative_module(&dir, *level, module) {
                    Some(target) => {
                        for a in import_names {
                            let local = a.alias.as_deref().unwrap_or(&a.name);
                            bind(
                                &mut names,
                                &mut index,
                                local.to_string(),
                                target.clone(),
                                a.name.clone(),
                            );
                        }
                    }
                    None => {
                        for a in import_names {
                            let local = a.alias.as_deref().unwrap_or(&a.name);
                            bind_unsupported(&mut names, &mut index, &mut unsupported, local);
                        }
                    }
                }
            }
            StmtKind::Import {
                names: import_names,
            } => {
                for a in import_names {
                    let local = match &a.alias {
                        Some(alias) => alias.clone(),
                        // `import pkg.sub` binds the top name `pkg`.
                        None => a.name.split('.').next().unwrap_or(&a.name).to_string(),
                    };
                    bind_unsupported(&mut names, &mut index, &mut unsupported, &local);
                }
            }
            _ => {}
        }
    }

    let flat: Vec<StarName> = names.into_iter().flatten().collect();
    if let Some(all) = all_list {
        // `__all__` is authoritative: exact set, in `__all__` order.
        let by_name: HashMap<&str, &StarName> =
            flat.iter().map(|s| (s.public.as_str(), s)).collect();
        let mut out = Vec::with_capacity(all.len());
        for n in &all {
            match by_name.get(n.as_str()) {
                Some(s) => out.push(StarName {
                    public: s.public.clone(),
                    defining_file: s.defining_file.clone(),
                    original: s.original.clone(),
                }),
                None if unsupported.iter().any(|u| u == n) => {
                    return Err(format!(
                        "__all__ in {} lists `{}`, which is bound by an absolute \
                         import or submodule binding — not star-expandable; import \
                         it explicitly",
                        canonical.display(),
                        n
                    ));
                }
                None => {
                    return Err(format!(
                        "__all__ in {} lists `{}` but no top-level definition of \
                         that name was found",
                        canonical.display(),
                        n
                    ));
                }
            }
        }
        return Ok(StarVisible {
            names: out,
            unsupported: Vec::new(),
        });
    }
    // No __all__: CPython's star skips underscore-prefixed names.
    let out: Vec<StarName> = flat
        .into_iter()
        .filter(|s| !s.public.starts_with('_'))
        .collect();
    let unsupported = unsupported
        .into_iter()
        .filter(|n| !n.starts_with('_'))
        .collect();
    Ok(StarVisible {
        names: out,
        unsupported,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("pyths_relstar_{}", name));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn expand(dir: &Path, entry: &str) -> Result<Module, String> {
        let src = std::fs::read_to_string(dir.join(entry)).unwrap();
        let mut module = pyths_parser::parse(&src).map_err(|_| "parse".to_string())?;
        normalize_relative_imports(&mut module, &dir.join(entry))?;
        Ok(module)
    }

    fn import_stmts(m: &Module) -> Vec<String> {
        m.body
            .iter()
            .filter_map(|s| match &s.kind {
                StmtKind::ImportFrom {
                    module,
                    names,
                    level,
                } => Some(format!(
                    "from {}{} import {}",
                    ".".repeat(*level as usize),
                    module,
                    names
                        .iter()
                        .map(|a| match &a.alias {
                            Some(al) => format!("{} as {}", a.name, al),
                            None => a.name.clone(),
                        })
                        .collect::<Vec<_>>()
                        .join(", ")
                )),
                StmtKind::ImportSideEffect(p) => Some(format!("sideeffect {}", p)),
                _ => None,
            })
            .collect()
    }

    #[test]
    fn expands_public_names_skips_underscore() {
        let dir = scratch("basic");
        std::fs::write(
            dir.join("impl.ps"),
            "Y = 5\nZ = 6\n_hidden = 1\ndef work():\n    return Y\nclass Thing:\n    pass\n",
        )
        .unwrap();
        std::fs::write(dir.join("main.ps"), "from .impl import *\nprint(Y + Z)\n").unwrap();
        let m = expand(&dir, "main.ps").unwrap();
        let imports = import_stmts(&m);
        assert_eq!(imports[0], "sideeffect ./impl");
        assert_eq!(imports[1], "from .impl import Y, Z, work, Thing");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn respects_all_list_order_and_filter() {
        let dir = scratch("all");
        std::fs::write(
            dir.join("impl.ps"),
            "__all__ = [\"work\", \"_kept\"]\n_kept = 3\ndef work():\n    return 1\ndef extra():\n    return 2\n",
        )
        .unwrap();
        std::fs::write(dir.join("main.ps"), "from .impl import *\n").unwrap();
        let m = expand(&dir, "main.ps").unwrap();
        let imports = import_stmts(&m);
        // __all__ keeps _kept (explicit) and drops extra (not listed).
        assert_eq!(imports[1], "from .impl import work, _kept");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn chained_star_attributes_to_defining_module() {
        let dir = scratch("chain");
        std::fs::write(dir.join("base.ps"), "Q = 7\n").unwrap();
        std::fs::write(dir.join("hub.ps"), "from .base import *\nH = 1\n").unwrap();
        std::fs::write(dir.join("main.ps"), "from .hub import *\nprint(Q + H)\n").unwrap();
        let m = expand(&dir, "main.ps").unwrap();
        let imports = import_stmts(&m);
        assert_eq!(imports[0], "sideeffect ./hub");
        // Q must come from base (its defining module), H from hub.
        assert!(
            imports.contains(&"from .base import Q".to_string()),
            "{imports:?}"
        );
        assert!(
            imports.contains(&"from .hub import H".to_string()),
            "{imports:?}"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn named_relative_rebind_attributes_to_origin() {
        let dir = scratch("origin");
        std::fs::write(dir.join("base.ps"), "def q():\n    return 1\n").unwrap();
        std::fs::write(dir.join("impl.ps"), "from .base import q as r\nW = 2\n").unwrap();
        std::fs::write(dir.join("main.ps"), "from .impl import *\n").unwrap();
        let m = expand(&dir, "main.ps").unwrap();
        let imports = import_stmts(&m);
        assert!(
            imports.contains(&"from .base import q as r".to_string()),
            "{imports:?}"
        );
        assert!(
            imports.contains(&"from .impl import W".to_string()),
            "{imports:?}"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn missing_sibling_is_loud() {
        let dir = scratch("missing");
        std::fs::write(dir.join("main.ps"), "from .nosuch import *\n").unwrap();
        let err = expand(&dir, "main.ps").unwrap_err();
        assert!(err.contains("no module file found"), "{err}");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn all_listing_absolute_bound_name_is_loud() {
        let dir = scratch("allabs");
        std::fs::write(
            dir.join("impl.ps"),
            "from math import sqrt\n__all__ = [\"sqrt\"]\n",
        )
        .unwrap();
        std::fs::write(dir.join("main.ps"), "from .impl import *\n").unwrap();
        let err = expand(&dir, "main.ps").unwrap_err();
        assert!(err.contains("absolute import"), "{err}");
        let _ = std::fs::remove_dir_all(&dir);
    }

    // FIX 2 — `from . import X` where X is a SYMBOL of the package __init__
    // (no submodule file) must be rewritten to the index sentinel (module
    // "."), which the emitter lowers to `import { X } from "./"`.
    #[test]
    fn dot_only_symbol_rewrites_to_index_sentinel() {
        let dir = scratch("dotsym");
        std::fs::write(dir.join("__init__.ps"), "CONST = 200\n").unwrap();
        std::fs::write(dir.join("mod.ps"), "from . import CONST\nprint(CONST)\n").unwrap();
        let m = expand(&dir, "mod.ps").unwrap();
        match &m.body[0].kind {
            StmtKind::ImportFrom {
                module,
                names,
                level,
            } => {
                assert_eq!(module, ".", "symbol import must carry the index sentinel");
                assert_eq!(*level, 1);
                assert_eq!(names.len(), 1);
                assert_eq!(names[0].name, "CONST");
            }
            other => panic!("expected ImportFrom, got {other:?}"),
        }
        let _ = std::fs::remove_dir_all(&dir);
    }

    // FIX 2 — mixed `from . import a, CONST` splits: submodule names stay in
    // the leading-dot-only form (namespace lowering), index symbols move to
    // the sentinel statement. Submodule names must NOT be re-routed.
    #[test]
    fn dot_only_mixed_splits_submodule_and_symbol() {
        let dir = scratch("dotmix");
        std::fs::write(dir.join("a.ps"), "X = 42\n").unwrap();
        std::fs::write(dir.join("__init__.ps"), "CONST = 200\n").unwrap();
        std::fs::write(
            dir.join("mod.ps"),
            "from . import a, CONST\nprint(a.X, CONST)\n",
        )
        .unwrap();
        let m = expand(&dir, "mod.ps").unwrap();
        let (sub, sym) = (&m.body[0].kind, &m.body[1].kind);
        match sub {
            StmtKind::ImportFrom { module, names, .. } => {
                assert_eq!(module, "", "submodule stays leading-dot-only");
                assert_eq!(names[0].name, "a");
            }
            other => panic!("expected submodule ImportFrom, got {other:?}"),
        }
        match sym {
            StmtKind::ImportFrom { module, names, .. } => {
                assert_eq!(module, ".", "symbol moves to the index sentinel");
                assert_eq!(names[0].name, "CONST");
            }
            other => panic!("expected sentinel ImportFrom, got {other:?}"),
        }
        let _ = std::fs::remove_dir_all(&dir);
    }

    // The cache pre-check must fire for BOTH sibling-dependent forms.
    #[test]
    fn sibling_dependency_precheck_covers_relative_forms() {
        assert!(may_depend_on_siblings("from .impl import *\n"));
        assert!(may_depend_on_siblings("from . import a\n"));
        assert!(may_depend_on_siblings("from .. import util\n"));
        // A NAMED relative import now also reads sibling FS (submodule-vs-index
        // disambiguation + __init__ precedence), so the cache must invalidate.
        assert!(may_depend_on_siblings("from .impl import work\n"));
        assert!(may_depend_on_siblings("from .pkg.sub import mod\n"));
        // Absolute imports never touch siblings.
        assert!(!may_depend_on_siblings("import math\nfrom x import y\n"));
        assert!(!may_depend_on_siblings("from os import path\n"));
    }

    // Issue A — `from . import X` where the package `__init__` DEFINES X and a
    // same-named submodule X.ps ALSO exists: CPython binds the __init__ symbol
    // (getattr precedence), NOT the submodule. Must rewrite to the index
    // sentinel `.`, never stay a submodule namespace import.
    #[test]
    fn dot_only_init_symbol_wins_over_same_named_submodule() {
        let dir = scratch("initwins");
        std::fs::write(dir.join("__init__.ps"), "X = \"from init\"\n").unwrap();
        std::fs::write(dir.join("X.ps"), "Y = 1\n").unwrap(); // same-named submodule
        std::fs::write(dir.join("mod.ps"), "from . import X\nprint(X)\n").unwrap();
        let m = expand(&dir, "mod.ps").unwrap();
        match &m.body[0].kind {
            StmtKind::ImportFrom { module, names, .. } => {
                assert_eq!(
                    module, ".",
                    "init-defined X must be the index symbol, not the submodule"
                );
                assert_eq!(names[0].name, "X");
            }
            other => panic!("expected sentinel ImportFrom, got {other:?}"),
        }
        let _ = std::fs::remove_dir_all(&dir);
    }

    // Issue A control — no __init__ binding: X stays a submodule (leading-dot
    // namespace form), unchanged from the prior behavior.
    #[test]
    fn dot_only_submodule_without_init_binding_stays_submodule() {
        let dir = scratch("initabsent");
        std::fs::write(dir.join("__init__.ps"), "OTHER = 1\n").unwrap();
        std::fs::write(dir.join("X.ps"), "Y = 1\n").unwrap();
        std::fs::write(dir.join("mod.ps"), "from . import X\n").unwrap();
        let m = expand(&dir, "mod.ps").unwrap();
        match &m.body[0].kind {
            StmtKind::ImportFrom { module, names, .. } => {
                assert_eq!(module, "", "submodule stays leading-dot-only");
                assert_eq!(names[0].name, "X");
            }
            other => panic!("expected submodule ImportFrom, got {other:?}"),
        }
        let _ = std::fs::remove_dir_all(&dir);
    }

    // Issue B — `from .sub import mod` where `mod` is a SUBMODULE of subpackage
    // `sub` (sub/mod.ps). The emitter would emit `import { mod } from "./sub"`
    // (ESM link error). Must rewrite to a namespace import of the submodule
    // FILE: leading-dot form with the encoded path `sub/mod`, aliased to `mod`,
    // which the emitter lowers to `import * as mod from "./sub/mod"`.
    #[test]
    fn named_relative_submodule_rewrites_to_namespace_of_file() {
        let dir = scratch("subofpkg");
        std::fs::create_dir_all(dir.join("sub")).unwrap();
        std::fs::write(dir.join("sub").join("__init__.ps"), "PKGV = 1\n").unwrap();
        std::fs::write(dir.join("sub").join("mod.ps"), "Z = 9\n").unwrap();
        std::fs::write(dir.join("main.ps"), "from .sub import mod\nprint(mod.Z)\n").unwrap();
        let m = expand(&dir, "main.ps").unwrap();
        match &m.body[0].kind {
            StmtKind::ImportFrom {
                module,
                names,
                level,
            } => {
                assert_eq!(
                    module, "",
                    "submodule import must become the leading-dot namespace form"
                );
                assert_eq!(*level, 1);
                assert_eq!(
                    names[0].name, "sub/mod",
                    "path is encoded for the emitter's ./<name> specifier"
                );
                assert_eq!(
                    names[0].alias.as_deref(),
                    Some("mod"),
                    "local binding is mod"
                );
            }
            other => panic!("expected namespace ImportFrom, got {other:?}"),
        }
        let _ = std::fs::remove_dir_all(&dir);
    }

    // Regression — a package `__init__` that itself does `from . import a`
    // (a submodule) must NOT be read as DEFINING a shadowing symbol `a`. The
    // `from . import a` binding IS the submodule; `a` stays a namespace import.
    #[test]
    fn init_self_dot_import_of_submodule_is_not_a_shadowing_symbol() {
        let dir = scratch("initself");
        std::fs::write(dir.join("a.ps"), "X = 42\n").unwrap();
        std::fs::write(dir.join("__init__.ps"), "from . import a\nprint(a.X)\n").unwrap();
        let m = expand(&dir, "__init__.ps").unwrap();
        match &m.body[0].kind {
            StmtKind::ImportFrom { module, names, .. } => {
                assert_eq!(module, "", "a must stay a submodule namespace import");
                assert_eq!(names[0].name, "a");
            }
            other => panic!("expected submodule ImportFrom, got {other:?}"),
        }
        let _ = std::fs::remove_dir_all(&dir);
    }

    // Issue B control — `from .sub import sym` where sym IS a symbol of
    // sub/__init__: stays a NAMED import from sub (the working form).
    #[test]
    fn named_relative_index_symbol_stays_named_import() {
        let dir = scratch("symofpkg");
        std::fs::create_dir_all(dir.join("sub")).unwrap();
        std::fs::write(dir.join("sub").join("__init__.ps"), "sym = 42\n").unwrap();
        std::fs::write(dir.join("main.ps"), "from .sub import sym\nprint(sym)\n").unwrap();
        let m = expand(&dir, "main.ps").unwrap();
        match &m.body[0].kind {
            StmtKind::ImportFrom { module, names, .. } => {
                assert_eq!(module, "sub", "index symbol stays a named import from sub");
                assert_eq!(names[0].name, "sym");
                assert!(names[0].alias.is_none());
            }
            other => panic!("expected named ImportFrom, got {other:?}"),
        }
        let _ = std::fs::remove_dir_all(&dir);
    }

    // Issue B — mixed `from .sub import mod, sym`: the submodule splits into a
    // namespace import, the index symbol stays a named import.
    #[test]
    fn named_relative_mixed_splits_submodule_and_symbol() {
        let dir = scratch("mixedsub");
        std::fs::create_dir_all(dir.join("sub")).unwrap();
        std::fs::write(dir.join("sub").join("__init__.ps"), "sym = 42\n").unwrap();
        std::fs::write(dir.join("sub").join("mod.ps"), "Z = 9\n").unwrap();
        std::fs::write(dir.join("main.ps"), "from .sub import mod, sym\n").unwrap();
        let m = expand(&dir, "main.ps").unwrap();
        let imports = import_stmts(&m);
        // submodule → namespace (leading-dot, encoded path); symbol → named.
        assert!(
            imports.contains(&"from . import sub/mod as mod".to_string()),
            "{imports:?}"
        );
        assert!(
            imports.contains(&"from .sub import sym".to_string()),
            "{imports:?}"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    // Issue B — nested subpackage path `from .pkg.sub import mod`.
    #[test]
    fn named_relative_nested_subpackage_submodule() {
        let dir = scratch("nestedsub");
        std::fs::create_dir_all(dir.join("pkg").join("sub")).unwrap();
        std::fs::write(dir.join("pkg").join("__init__.ps"), "\n").unwrap();
        std::fs::write(dir.join("pkg").join("sub").join("__init__.ps"), "\n").unwrap();
        std::fs::write(dir.join("pkg").join("sub").join("mod.ps"), "Z = 9\n").unwrap();
        std::fs::write(dir.join("main.ps"), "from .pkg.sub import mod\n").unwrap();
        let m = expand(&dir, "main.ps").unwrap();
        match &m.body[0].kind {
            StmtKind::ImportFrom { module, names, .. } => {
                assert_eq!(module, "");
                assert_eq!(
                    names[0].name, "pkg/sub/mod",
                    "nested path encoded with slashes"
                );
                assert_eq!(names[0].alias.as_deref(), Some("mod"));
            }
            other => panic!("expected namespace ImportFrom, got {other:?}"),
        }
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn star_from_package_init_via_dot_pkg() {
        let dir = scratch("pkginit");
        std::fs::create_dir_all(dir.join("pkg")).unwrap();
        std::fs::write(dir.join("pkg").join("__init__.ps"), "V = 9\n").unwrap();
        std::fs::write(dir.join("main.ps"), "from .pkg import *\nprint(V)\n").unwrap();
        let m = expand(&dir, "main.ps").unwrap();
        let imports = import_stmts(&m);
        assert!(
            imports.contains(&"from .pkg import V".to_string()),
            "{imports:?}"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }
}
