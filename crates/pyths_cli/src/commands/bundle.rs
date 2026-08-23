use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

use pyths_codegen_js::sourcemap::{encode_bundle_map, BundleMapping, Mapping};

use super::output::{self, Verbosity};

/// One compiled module of the bundle, with the raw per-module source mappings
/// (final module-JS positions) when `--sourcemap` is on.
struct ModuleUnit {
    js: String,
    mappings: Vec<Mapping>,
    /// Forward-slash path recorded in the bundle map's `sources`.
    src_path: String,
    src_content: String,
    /// The source file's directory — relative import specifiers in the
    /// emitted JS resolve against THIS, not the entry's dir.
    dir: PathBuf,
    /// Emitted-JS relative specifier → canonical path of the bundled module
    /// it resolves to (built during traversal; the rewrite phase consults it
    /// so traversal and linking can never disagree).
    dep_by_spec: HashMap<String, PathBuf>,
    /// Unique JS namespace variable this module's exports are exposed under
    /// (non-entry modules only).
    ns_var: String,
}

/// A parsed emitted-JS import line. The emitter writes every import as a
/// single line at column 0 (function-local imports are hoisted), so
/// line-based parsing is exact for generated code.
enum ImportLine {
    /// `import "spec";`
    SideEffect { spec: String },
    /// `import * as X from "spec";`
    Namespace { binding: String, spec: String },
    /// `import { a, b as c } from "spec";` — (exported, local) pairs.
    Named {
        specs: Vec<(String, String)>,
        spec: String,
    },
    /// `import X from "spec";`
    Default { binding: String, spec: String },
}

fn unquote(s: &str) -> Option<String> {
    let s = s.trim();
    s.strip_prefix('"')?.strip_suffix('"').map(str::to_string)
}

fn parse_import_line(line: &str) -> Option<ImportLine> {
    let rest = line.strip_prefix("import ")?.trim_end();
    let rest = rest.strip_suffix(';').unwrap_or(rest);
    if rest.starts_with('"') {
        return Some(ImportLine::SideEffect {
            spec: unquote(rest)?,
        });
    }
    let (head, spec) = rest.rsplit_once(" from ")?;
    let spec = unquote(spec)?;
    let head = head.trim();
    if let Some(ns) = head.strip_prefix("* as ") {
        return Some(ImportLine::Namespace {
            binding: ns.trim().to_string(),
            spec,
        });
    }
    if let Some(inner) = head.strip_prefix('{') {
        let inner = inner.strip_suffix('}')?;
        let specs = inner
            .split(',')
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(|s| match s.split_once(" as ") {
                Some((e, l)) => (e.trim().to_string(), l.trim().to_string()),
                None => (s.to_string(), s.to_string()),
            })
            .collect();
        return Some(ImportLine::Named { specs, spec });
    }
    Some(ImportLine::Default {
        binding: head.to_string(),
        spec,
    })
}

/// Join a forward-slash specifier onto `dir`, normalizing `.`/`..` segments
/// eagerly — the traversal works with CANONICAL directories, which on
/// Windows carry the verbatim `\\?\` prefix where the OS does NOT collapse
/// `./` / `../` (a raw join of `"./../x"` would never resolve).
fn normalized_join(dir: &Path, rel: &str) -> PathBuf {
    let mut out = dir.to_path_buf();
    for part in rel.split('/') {
        match part {
            "" | "." => {}
            ".." => {
                out.pop();
            }
            p => out.push(p),
        }
    }
    out
}

/// Resolve an emitted-JS RELATIVE specifier (extensionless, forward-slash)
/// against the importing module's directory: `<spec>.ps`, `<spec>.py`,
/// `<spec>/__init__.ps`, `<spec>/__init__.py` (a bare `.`/`./` is the
/// package index itself).
fn resolve_relative_spec(dir: &Path, spec: &str) -> Option<PathBuf> {
    let trimmed = spec.trim_end_matches('/');
    let candidates: Vec<PathBuf> = if trimmed.is_empty() || trimmed == "." {
        vec![dir.join("__init__.ps"), dir.join("__init__.py")]
    } else {
        let joined = normalized_join(dir, trimmed);
        vec![
            PathBuf::from(format!("{}.ps", joined.display())),
            PathBuf::from(format!("{}.py", joined.display())),
            joined.join("__init__.ps"),
            joined.join("__init__.py"),
        ]
    };
    for c in candidates {
        if c.is_file() {
            return c.canonicalize().ok();
        }
    }
    None
}

/// A BARE specifier (`import helper` → `import * as helper from "helper"`)
/// resolves to an npm package BY DESIGN in per-module compiles — but `pyths
/// bundle` has always inlined a same-directory sibling of that name (the
/// self-contained single-file story). Treat the bare spec as bundle-local
/// only when the sibling file actually exists (also probing the kebab→snake
/// inverse, since emitted npm specs kebab-case Python module names);
/// otherwise it stays external.
fn resolve_bare_local(dir: &Path, spec: &str) -> Option<PathBuf> {
    let mut names: Vec<String> = vec![spec.to_string()];
    if spec.contains('-') {
        names.push(spec.replace('-', "_"));
    }
    for name in names {
        if let Some(found) = resolve_relative_spec(dir, &name) {
            return Some(found);
        }
    }
    None
}

/// Collect the binding names of an emitted `export` DECLARATION pattern —
/// a bare identifier or an array/nested-array destructuring pattern
/// (module-level tuple unpack). Stops at a top-level `=`.
fn collect_pattern_names(pattern: &str, out: &mut Vec<String>) {
    let mut ident = String::new();
    let mut depth = 0usize;
    for ch in pattern.chars() {
        match ch {
            '[' | '{' | '(' => depth += 1,
            ']' | '}' | ')' => depth = depth.saturating_sub(1),
            '=' if depth == 0 => break,
            _ => {}
        }
        if ch.is_ascii_alphanumeric() || ch == '_' || ch == '$' {
            ident.push(ch);
        } else {
            if !ident.is_empty() && !ident.chars().next().unwrap().is_ascii_digit() {
                out.push(std::mem::take(&mut ident));
            }
            ident.clear();
            if ch == '=' && depth == 0 {
                break;
            }
        }
    }
    if !ident.is_empty() && !ident.chars().next().unwrap().is_ascii_digit() {
        out.push(ident);
    }
}

/// One transformed output line: text plus the module-JS line it came from
/// (None for glue lines the bundle map must not attribute to user source).
struct XLine {
    text: String,
    orig_line: Option<u32>,
}

/// A module body rewritten for inlining: imports of bundled modules become
/// destructures/aliases of their namespace objects, `export` markers are
/// stripped (columns preserved), and the export surface is collected so the
/// wrapper can `return` a live (getter-based) namespace object.
struct TransformedModule {
    lines: Vec<XLine>,
    /// (public name, local expression) — the module's export surface.
    exports: Vec<(String, String)>,
}

/// BUG #3 root fix (`pyths bundle` emitted invalid JS for any relative
/// import): rewrite one module's emitted JS for inlining. ALL import/export
/// syntax is handled in this ONE place, for entry and non-entry modules
/// alike:
///   - relative imports of bundled modules → `const {…} = <ns_var>;` /
///     `const X = <ns_var>;` (dep initialized earlier — emission is in
///     dependency order);
///   - unresolvable RELATIVE imports are a HARD error (never silently
///     dropped); non-relative (npm/runtime) import lines are dropped as
///     before (the bundle inlines the runtime; npm deps stay external);
///   - non-entry `export <decl>` → the `export ` marker is replaced by
///     7 spaces (same columns, so source-map column shifts stay exact) and
///     the declared names are recorded for the namespace `return`;
///     `export default` / `export { … };` are rewritten and recorded too.
///     The entry keeps its `export`s verbatim (top-level ESM).
fn transform_unit(
    unit_js: &str,
    unit_dir: &Path,
    dep_by_spec: &HashMap<String, PathBuf>,
    ns_by_path: &HashMap<PathBuf, String>,
    exports_by_path: &HashMap<PathBuf, HashSet<String>>,
    is_entry: bool,
    src_label: &str,
) -> Result<TransformedModule, String> {
    let mut lines: Vec<XLine> = Vec::new();
    let mut exports: Vec<(String, String)> = Vec::new();
    let mut export_seen: HashSet<String> = HashSet::new();
    let record_export = |exports: &mut Vec<(String, String)>,
                         export_seen: &mut HashSet<String>,
                         public: String,
                         local: String| {
        if export_seen.insert(public.clone()) {
            exports.push((public, local));
        }
    };
    let mut default_counter = 0usize;

    for (mod_line, line) in unit_js.lines().enumerate() {
        if line.starts_with("import ") || line == "import" {
            let parsed = parse_import_line(line).ok_or_else(|| {
                format!(
                    "bundle: unrecognized import statement in generated JS for {}: {}",
                    src_label, line
                )
            })?;
            let spec = match &parsed {
                ImportLine::SideEffect { spec }
                | ImportLine::Namespace { spec, .. }
                | ImportLine::Named { spec, .. }
                | ImportLine::Default { spec, .. } => spec.clone(),
            };
            let is_relative = spec.starts_with("./") || spec.starts_with("../") || spec == ".";
            let target = match dep_by_spec.get(&spec).cloned() {
                Some(t) => t,
                None if is_relative => {
                    // Fallback: resolve here (covers a spec the traversal saw
                    // under a normalized form). A RELATIVE spec must resolve.
                    resolve_relative_spec(unit_dir, &spec).ok_or_else(|| {
                        format!(
                            "bundle: cannot resolve relative import \"{}\" from {} — \
                             no .ps/.py module or package __init__ found",
                            spec, src_label
                        )
                    })?
                }
                None => {
                    // npm / external — dropped from the bundle (pre-existing
                    // behavior; the runtime is already inlined per module).
                    continue;
                }
            };
            let ns = ns_by_path.get(&target).ok_or_else(|| {
                format!(
                    "bundle: relative import \"{}\" in {} resolved to {} which was \
                     not inlined (internal bundling error)",
                    spec,
                    src_label,
                    target.display()
                )
            })?;
            // FIX 1(a) — the export-surface gate that makes a silent
            // `undefined` binding IMPOSSIBLE in a bundle, for ANY cause: a
            // destructure of the namespace object would quietly yield
            // `undefined` for a name the module never exported, where a
            // per-module compile fails LOUD at ESM link time. Verify every
            // named/default import against the dep's RECORDED export surface
            // (deps are transformed before their importers — dependency
            // order) and hard-error on a miss, mirroring the link failure.
            let check_export = |name: &str| -> Result<(), String> {
                let surface = exports_by_path.get(&target).ok_or_else(|| {
                    format!(
                        "bundle: internal error — {} imported by {} was not \
                         transformed before its importer",
                        target.display(),
                        src_label
                    )
                })?;
                if surface.contains(name) {
                    Ok(())
                } else {
                    Err(format!(
                        "bundle: module \"{}\" (inlined from {}) does not export \
                         `{}`, imported by {} — a per-module compile fails the \
                         same way at ESM link time. Define/export the name in \
                         the module, or import the module namespace instead.",
                        spec,
                        target.display(),
                        name,
                        src_label
                    ))
                }
            };
            match parsed {
                ImportLine::SideEffect { .. } => {
                    // Dependency already initialized (dependency-ordered
                    // emission); nothing to bind.
                }
                ImportLine::Namespace { binding, .. } => lines.push(XLine {
                    text: format!("const {} = {};", binding, ns),
                    orig_line: None,
                }),
                ImportLine::Named { specs, .. } => {
                    for (exported, _) in &specs {
                        check_export(exported)?;
                    }
                    let inner = specs
                        .iter()
                        .map(|(e, l)| {
                            if e == l {
                                e.clone()
                            } else {
                                format!("{}: {}", e, l)
                            }
                        })
                        .collect::<Vec<_>>()
                        .join(", ");
                    lines.push(XLine {
                        text: format!("const {{ {} }} = {};", inner, ns),
                        orig_line: None,
                    });
                }
                ImportLine::Default { binding, .. } => {
                    check_export("default")?;
                    lines.push(XLine {
                        text: format!("const {} = {}.default;", binding, ns),
                        orig_line: None,
                    });
                }
            }
            continue;
        }

        if !is_entry {
            if let Some(rest) = line.strip_prefix("export ") {
                if let Some(default_rest) = rest.strip_prefix("default ") {
                    let var = format!("__pyModDefault{}", default_counter);
                    default_counter += 1;
                    record_export(
                        &mut exports,
                        &mut export_seen,
                        "default".into(),
                        var.clone(),
                    );
                    lines.push(XLine {
                        text: format!("const {} = {}", var, default_rest),
                        orig_line: None,
                    });
                    continue;
                }
                if rest.trim_start().starts_with('{') {
                    // `export { a, b as c };` (local re-export list).
                    // `export { … } from "…"` (wasm-bridge glue) cannot be
                    // inlined — fail loud rather than emit invalid JS.
                    if rest.contains(" from ") {
                        return Err(format!(
                            "bundle: `export … from` re-exports are not supported \
                             when inlining {} — compile per-module instead",
                            src_label
                        ));
                    }
                    let inner = rest
                        .trim_start()
                        .trim_start_matches('{')
                        .trim_end()
                        .trim_end_matches(';')
                        .trim_end()
                        .trim_end_matches('}');
                    for part in inner.split(',') {
                        let part = part.trim();
                        if part.is_empty() {
                            continue;
                        }
                        match part.split_once(" as ") {
                            Some((local, public)) => record_export(
                                &mut exports,
                                &mut export_seen,
                                public.trim().to_string(),
                                local.trim().to_string(),
                            ),
                            None => record_export(
                                &mut exports,
                                &mut export_seen,
                                part.to_string(),
                                part.to_string(),
                            ),
                        }
                    }
                    continue; // drop the line; the namespace return exposes them
                }
                // Declaration export: function / async function / function* /
                // class / const / let / var. Record the bound name(s) and
                // strip the marker WITHOUT shifting columns (7 spaces), so
                // per-line source-map column shifts stay exact.
                let decl = rest.strip_prefix("async ").unwrap_or(rest);
                let names_part = decl
                    .strip_prefix("function* ")
                    .or_else(|| decl.strip_prefix("function "))
                    .or_else(|| decl.strip_prefix("class "))
                    .or_else(|| decl.strip_prefix("const "))
                    .or_else(|| decl.strip_prefix("let "))
                    .or_else(|| decl.strip_prefix("var "));
                match names_part {
                    Some(np) => {
                        let mut bound = Vec::new();
                        if np.starts_with('[') || np.starts_with('{') {
                            collect_pattern_names(np, &mut bound);
                        } else {
                            let ident: String = np
                                .chars()
                                .take_while(|c| c.is_ascii_alphanumeric() || *c == '_' || *c == '$')
                                .collect();
                            if !ident.is_empty() {
                                bound.push(ident);
                            }
                        }
                        for n in bound {
                            record_export(&mut exports, &mut export_seen, n.clone(), n);
                        }
                        lines.push(XLine {
                            text: format!("       {}", rest),
                            orig_line: Some(mod_line as u32),
                        });
                        continue;
                    }
                    None => {
                        return Err(format!(
                            "bundle: unrecognized export statement in generated JS \
                             for {}: {}",
                            src_label, line
                        ));
                    }
                }
            }
        }

        lines.push(XLine {
            text: line.to_string(),
            orig_line: Some(mod_line as u32),
        });
    }

    Ok(TransformedModule { lines, exports })
}

/// The synthetic source every NON-user bundle line (banner, IIFE wrappers,
/// inlined runtime helpers, glue) is mapped to — and the one entry in the
/// bundle map's `ignoreList`/`x_google_ignoreList`, so DevTools "step into"
/// skips it all and stays in `.ps` code.
const GLUE_SOURCE: &str = "pyths:bundle-glue";
const GLUE_CONTENT: &str =
    "// PythScribe bundle glue + inlined pyths-runtime helpers (ignore-listed)\n";

pub fn run(
    entry: &str,
    output: Option<&str>,
    minify: bool,
    sourcemap: bool,
    no_sources_content: bool,
    verbosity: Verbosity,
) -> Result<(), Box<dyn std::error::Error>> {
    let entry_path = Path::new(entry);
    if !entry_path.exists() {
        return Err(format!("Entry file not found: {}", entry).into());
    }
    if minify && sourcemap {
        // The line-based minifier rewrites the layout the map was built
        // against; shipping a wrong map is worse than no map.
        return Err(
            "--minify with --sourcemap is not supported yet (the map would not \
                    match the minified layout) — drop one of the two flags"
                .into(),
        );
    }
    // P3 (CWE-94): the entry name is interpolated raw into a `// Entry: …`
    // comment (and module stems into `// --- Module: … ---` + `const <name>`),
    // so a control character — a newline in `foo\nconsole.log("PWNED");//.ps` —
    // would break out of the comment into executable bundle code. Reject it.
    super::compile::reject_control_bearing_name("entry file name", entry)?;

    super::output::info(&format!("Bundling from entry: {}", entry), verbosity);

    // ── Traversal ──────────────────────────────────────────────────────────
    // Modules are keyed by CANONICAL PATH (stems collide across directories),
    // and dependencies are discovered from the EMITTED JS import lines — the
    // same lines the rewrite phase links against — so traversal and linking
    // share one source of truth and can never disagree.
    let entry_key = entry_path
        .canonicalize()
        .map_err(|e| format!("cannot resolve entry path {}: {}", entry, e))?;
    // CANONICAL entry dir — queue paths are canonical, so relativizing the
    // map/banner labels against a non-canonical parent would silently keep
    // the full build-machine path (strip_prefix("") trivially succeeds).
    let base_dir = entry_key
        .parent()
        .map(Path::to_path_buf)
        .unwrap_or_else(|| PathBuf::from("."));
    let base_dir = base_dir.as_path();
    let mut modules: HashMap<PathBuf, ModuleUnit> = HashMap::new();
    let mut deps: HashMap<PathBuf, Vec<PathBuf>> = HashMap::new();
    let mut discovery_order: Vec<PathBuf> = Vec::new();

    let mut queue: Vec<PathBuf> = vec![entry_key.clone()];
    while let Some(file_path) = queue.pop() {
        let canonical = file_path
            .canonicalize()
            .unwrap_or_else(|_| file_path.clone());
        if modules.contains_key(&canonical) {
            continue;
        }

        let source = std::fs::read_to_string(&file_path)?;
        let filename = file_path.file_stem().unwrap().to_string_lossy().to_string();
        super::compile::reject_control_bearing_name("module file name", &filename)?; // P3

        // Parse
        let mut module = pyths_parser::parse(&source).map_err(|errors| {
            let messages: Vec<String> = errors.iter().map(|e| e.message.clone()).collect();
            format!(
                "Parse error in {}: {}",
                file_path.display(),
                messages.join(", ")
            )
        })?;
        // Expand relative star imports (commands::relstar) before codegen —
        // same pass every other CLI entry point runs.
        super::relstar::normalize_relative_imports(&mut module, &file_path)?;

        // Codegen — with raw mappings when a bundle map is requested.
        // Relativize to the entry's base dir (fallback: bare file name) so the
        // shipped bundle map's `sources` carry no absolute build-machine paths
        // (production hygiene + reproducible maps; matches compile's bare-name maps).
        let src_path = file_path
            .strip_prefix(base_dir)
            .map(std::path::Path::to_path_buf)
            .unwrap_or_else(|_| {
                PathBuf::from(file_path.file_name().unwrap_or(file_path.as_os_str()))
            })
            .to_string_lossy()
            .replace('\\', "/");
        // P3: directory components of the label are interpolated into the
        // `// --- Module: … ---` banner too.
        super::compile::reject_control_bearing_name("module path", &src_path)?;
        let (js, mappings) = if sourcemap {
            let out = pyths_codegen_js::codegen_inline_with_raw_map(&module, &source, &src_path);
            (out.js, out.mappings)
        } else {
            (pyths_codegen_js::codegen_inline(&module), Vec::new())
        };

        // Discover bundled dependencies from the emitted import lines.
        let dir = file_path
            .parent()
            .map(Path::to_path_buf)
            .unwrap_or_else(|| PathBuf::from("."));
        let mut dep_by_spec: HashMap<String, PathBuf> = HashMap::new();
        let mut dep_list: Vec<PathBuf> = Vec::new();
        for line in js.lines() {
            if !line.starts_with("import ") {
                continue;
            }
            let Some(parsed) = parse_import_line(line) else {
                continue;
            };
            let spec = match &parsed {
                ImportLine::SideEffect { spec }
                | ImportLine::Namespace { spec, .. }
                | ImportLine::Named { spec, .. }
                | ImportLine::Default { spec, .. } => spec.clone(),
            };
            let is_relative = spec.starts_with("./") || spec.starts_with("../") || spec == ".";
            let target = if is_relative {
                // A RELATIVE spec must resolve — silently dropping it was the
                // old failure mode.
                resolve_relative_spec(&dir, &spec).ok_or_else(|| {
                    format!(
                        "bundle: cannot resolve relative import \"{}\" from {} — no \
                         .ps/.py module or package __init__ found",
                        spec,
                        file_path.display()
                    )
                })?
            } else {
                match resolve_bare_local(&dir, &spec) {
                    Some(t) => t,     // absolute-local sibling — bundled (see doc)
                    None => continue, // npm / runtime — stays external
                }
            };
            if !dep_list.contains(&target) {
                dep_list.push(target.clone());
            }
            queue.push(target.clone());
            dep_by_spec.insert(spec, target);
        }

        deps.insert(canonical.clone(), dep_list);
        discovery_order.push(canonical.clone());
        modules.insert(
            canonical,
            ModuleUnit {
                js,
                mappings,
                src_path,
                src_content: source,
                dir,
                dep_by_spec,
                ns_var: String::new(), // assigned below
            },
        );
    }

    // Unique namespace vars for the non-entry modules (derived from the
    // entry-relative path so they are deterministic and readable).
    let mut used_ns: HashSet<String> = HashSet::new();
    for key in &discovery_order {
        if *key == entry_key {
            continue;
        }
        let unit = modules.get_mut(key).expect("discovered module present");
        let stem: String = unit
            .src_path
            .trim_end_matches(".ps")
            .trim_end_matches(".py")
            .chars()
            .map(|c| if c.is_ascii_alphanumeric() { c } else { '_' })
            .collect();
        let mut ns = format!("__pyMod_{}", stem);
        let mut n = 1usize;
        while !used_ns.insert(ns.clone()) {
            ns = format!("__pyMod_{}_{}", stem, n);
            n += 1;
        }
        unit.ns_var = ns;
    }
    let ns_by_path: HashMap<PathBuf, String> = modules
        .iter()
        .filter(|(k, _)| **k != entry_key)
        .map(|(k, u)| (k.clone(), u.ns_var.clone()))
        .collect();

    // ── Dependency-ordered emission plan ──────────────────────────────────
    // Post-order DFS from the entry: every module is emitted AFTER the
    // modules it imports (its IIFE references their namespace consts at
    // init time). Import cycles cannot be inlined as IIFEs — fail loud.
    let mut emit_order: Vec<PathBuf> = Vec::new();
    {
        #[derive(PartialEq)]
        enum Color {
            Gray,
            Black,
        }
        let mut colors: HashMap<PathBuf, Color> = HashMap::new();
        // Iterative DFS with an explicit stack: (node, child-index).
        let mut stack: Vec<(PathBuf, usize)> = vec![(entry_key.clone(), 0)];
        colors.insert(entry_key.clone(), Color::Gray);
        while let Some((node, idx)) = stack.pop() {
            let children = &deps[&node];
            if idx < children.len() {
                let child = children[idx].clone();
                stack.push((node.clone(), idx + 1));
                match colors.get(&child) {
                    None => {
                        colors.insert(child.clone(), Color::Gray);
                        stack.push((child, 0));
                    }
                    Some(Color::Gray) => {
                        return Err(format!(
                            "bundle: circular relative imports cannot be inlined — \
                             cycle through {} and {}",
                            node.display(),
                            child.display()
                        )
                        .into());
                    }
                    Some(Color::Black) => {}
                }
            } else {
                colors.insert(node.clone(), Color::Black);
                emit_order.push(node);
            }
        }
    }
    // Post-order puts the entry LAST already; split it off.
    assert_eq!(
        emit_order.last(),
        Some(&entry_key),
        "entry must be last in post-order"
    );
    emit_order.pop();

    let mut bundle = String::new();
    // Bundle line cursor (0-based, BEFORE the generated header is prepended)
    // and per-module `module JS line → (bundle line, column shift)` tables —
    // built while emitting so the bundle map needs no re-parse of the output.
    let mut cursor: u32 = 0;
    let push_line = |bundle: &mut String, cursor: &mut u32, s: &str| {
        bundle.push_str(s);
        bundle.push('\n');
        *cursor += 1;
    };
    let mut line_tables: Vec<HashMap<u32, (u32, u32)>> = Vec::new();

    // Banner
    push_line(
        &mut bundle,
        &mut cursor,
        "// PythScribe Bundle — generated by `pyths bundle`",
    );
    push_line(&mut bundle, &mut cursor, &format!("// Entry: {}", entry));
    push_line(&mut bundle, &mut cursor, "");

    // Non-entry modules, dependency-first, each wrapped in an IIFE that
    // RETURNS a live (getter-based) namespace object of its exports. Each
    // module's export surface is recorded as it is transformed so importers
    // (transformed later — dependency order) can be verified against it.
    let mut exports_by_path: HashMap<PathBuf, HashSet<String>> = HashMap::new();
    for key in &emit_order {
        let unit = &modules[key];
        let transformed = transform_unit(
            &unit.js,
            &unit.dir,
            &unit.dep_by_spec,
            &ns_by_path,
            &exports_by_path,
            false,
            &unit.src_path,
        )?;
        exports_by_path.insert(
            key.clone(),
            transformed.exports.iter().map(|(p, _)| p.clone()).collect(),
        );
        let mut table: HashMap<u32, (u32, u32)> = HashMap::new();
        push_line(
            &mut bundle,
            &mut cursor,
            &format!("// --- Module: {} ---", unit.src_path),
        );
        push_line(
            &mut bundle,
            &mut cursor,
            &format!("const {} = (() => {{", unit.ns_var),
        );
        for xl in &transformed.lines {
            if let Some(orig) = xl.orig_line {
                table.insert(orig, (cursor, 2));
            }
            push_line(&mut bundle, &mut cursor, &format!("  {}", xl.text));
        }
        // Live namespace: getters keep module-level rebinds visible through
        // the namespace object (Python module-attribute semantics), while
        // named-import destructures above still copy at import time
        // (CPython `from m import x` semantics).
        let props = transformed
            .exports
            .iter()
            .map(|(public, local)| format!("get {}() {{ return {}; }}", public, local))
            .collect::<Vec<_>>()
            .join(", ");
        push_line(
            &mut bundle,
            &mut cursor,
            &format!("  return {{ {} }};", props),
        );
        push_line(&mut bundle, &mut cursor, "})();");
        push_line(&mut bundle, &mut cursor, "");
        line_tables.push(table);
    }

    // Entry module directly (keeps its top-level `export`s — the bundle is
    // ESM; only its imports of inlined modules are rewritten).
    let entry_unit_key = entry_key.clone();
    {
        let unit = &modules[&entry_unit_key];
        let transformed = transform_unit(
            &unit.js,
            &unit.dir,
            &unit.dep_by_spec,
            &ns_by_path,
            &exports_by_path,
            true,
            &unit.src_path,
        )?;
        let mut table: HashMap<u32, (u32, u32)> = HashMap::new();
        push_line(&mut bundle, &mut cursor, "// --- Entry ---");
        for xl in &transformed.lines {
            if let Some(orig) = xl.orig_line {
                table.insert(orig, (cursor, 0));
            }
            push_line(&mut bundle, &mut cursor, &xl.text);
        }
        line_tables.push(table);
    }

    // Minify if requested (basic: strip comments and collapse whitespace).
    // (--minify + --sourcemap was rejected up front.)
    if minify {
        bundle = basic_minify(&bundle);
    }

    // Write output
    let output_path = output.map(|s| s.to_string()).unwrap_or_else(|| {
        let stem = entry_path.file_stem().unwrap().to_string_lossy();
        format!("{}.bundle.js", stem)
    });
    let map_path = format!("{}.map", output_path);

    // SEC #5 (CWE-59): the default entry-stem bundle path used a bare
    // `std::fs::write`, which FOLLOWS a checked-in symlink and would overwrite
    // its target with source-influenced JS. Route through the shared no-follow,
    // marker-gated writer instead: symlinks are refused, an unowned same-name
    // file is not clobbered, and the bundle carries the generated marker so a
    // rebuild recognizes its own output.
    let header = super::safewrite::generated_header();
    let header_lines = header.matches('\n').count() as u32;
    let mut bundle = format!("{}{}", header, bundle);
    if sourcemap {
        bundle.push_str(&format!(
            "//# sourceMappingURL={}\n",
            Path::new(&map_path).file_name().unwrap().to_string_lossy()
        ));
    }

    // Compose the bundle source map: user sections map to their `.ps`
    // sources; EVERY other line (generated header, banner, IIFE wrappers,
    // inlined runtime helpers) maps to the synthetic ignore-listed
    // `pyths:bundle-glue` source, so DevTools step-into skips it all.
    let map_json = if sourcemap {
        let mut sources: Vec<String> = Vec::new();
        let mut contents: Vec<Option<String>> = Vec::new();
        let mut mappings: Vec<BundleMapping> = Vec::new();
        let units: Vec<&ModuleUnit> = emit_order
            .iter()
            .map(|k| &modules[k])
            .chain(std::iter::once(&modules[&entry_unit_key]))
            .collect();
        for (src_idx, unit) in units.iter().enumerate() {
            sources.push(unit.src_path.clone());
            // A17 parity: `--no-sources-content` drops the inlined original
            // `.ps` from the shipped bundle map (DevTools still resolves by path).
            contents.push(if no_sources_content {
                None
            } else {
                Some(unit.src_content.clone())
            });
            let table = &line_tables[src_idx];
            for m in &unit.mappings {
                if let Some(&(bundle_line, col_shift)) = table.get(&m.gen_line) {
                    mappings.push(BundleMapping {
                        gen_line: header_lines + bundle_line,
                        gen_col: m.gen_col + col_shift,
                        src: src_idx as u32,
                        orig_line: m.orig_line,
                        orig_col: m.orig_col,
                    });
                }
            }
        }
        let glue_idx = sources.len() as u32;
        sources.push(GLUE_SOURCE.to_string());
        contents.push(if no_sources_content {
            None
        } else {
            Some(GLUE_CONTENT.to_string())
        });
        let user_lines: HashSet<u32> = mappings.iter().map(|m| m.gen_line).collect();
        let total_lines = bundle.matches('\n').count() as u32;
        for line in 0..total_lines {
            if !user_lines.contains(&line) {
                mappings.push(BundleMapping {
                    gen_line: line,
                    gen_col: 0,
                    src: glue_idx,
                    orig_line: 0,
                    orig_col: 0,
                });
            }
        }
        let file_name = Path::new(&output_path)
            .file_name()
            .unwrap()
            .to_string_lossy()
            .to_string();
        Some(encode_bundle_map(
            &file_name,
            &sources,
            &contents,
            &[glue_idx],
            &mappings,
        ))
    } else {
        None
    };

    // P11: a bundle from a PREVIOUS PythScribe version carries the old banner
    // but not the `@generated` marker, so the ownership gate would refuse to
    // rebuild it (the command has no `--force`). Treat OUR OWN banner — the new
    // `@generated` header (handled by the marker gate) or the legacy
    // `// PythScribe Bundle …` banner — as owned so a legitimate rebuild
    // succeeds; a genuinely hand-written file is still refused.
    let own_legacy = std::fs::read_to_string(&output_path)
        .ok()
        .map(|s| s.starts_with("// PythScribe Bundle — generated by `pyths bundle`"))
        .unwrap_or(false);
    // P10 discipline: preflight the COMPLETE output set (bundle + optional
    // `.js.map` sidecar) before writing either, so a refusal cannot leave a
    // fresh bundle beside a stale map.
    let mut planned = vec![super::safewrite::PlannedOutput::marked_text(Path::new(
        &output_path,
    ))];
    if map_json.is_some() {
        planned.push(super::safewrite::PlannedOutput::sidecar(
            Path::new(&map_path),
            Path::new(&output_path),
        ));
    }
    let plan = super::safewrite::OutputPlan::preflight(&planned, own_legacy)?;
    plan.write(Path::new(&output_path), bundle.as_bytes())?;
    if let Some(json) = &map_json {
        plan.write(Path::new(&map_path), json.as_bytes())?;
    }
    if verbosity != Verbosity::Quiet {
        if map_json.is_some() {
            output::success(&format!(
                "Bundle written to {} ({} bytes, + {})",
                output_path,
                bundle.len(),
                map_path
            ));
        } else {
            output::success(&format!(
                "Bundle written to {} ({} bytes)",
                output_path,
                bundle.len()
            ));
        }
    }

    Ok(())
}

/// Basic minification: remove comments, collapse blank lines
fn basic_minify(source: &str) -> String {
    let mut result = String::new();
    let mut prev_blank = false;
    // Whether the start of the current line is inside a `...` template literal.
    // Its continuation lines are string data, so `//`-leading lines (e.g. a URL)
    // must not be dropped as comments (issue #193). Nested `${` templates are a
    // known limitation of this parity heuristic.
    let mut in_template = false;

    for line in source.lines() {
        if in_template {
            result.push_str(line);
            result.push('\n');
            if count_unescaped_backticks(line) % 2 == 1 {
                in_template = false;
            }
            prev_blank = false;
            continue;
        }

        let trimmed = line.trim();

        // Skip comment-only lines
        if trimmed.starts_with("//") {
            continue;
        }

        // Skip excessive blank lines
        if trimmed.is_empty() {
            if prev_blank {
                continue;
            }
            prev_blank = true;
        } else {
            prev_blank = false;
        }

        // Remove inline trailing comments (simple heuristic, not in strings)
        let line_no_comment = if let Some(idx) = trimmed.find("//") {
            // Don't remove if inside a string (basic check)
            let before = &trimmed[..idx];
            if before.matches('"').count() % 2 == 0
                && before.matches('\'').count() % 2 == 0
                && before.matches('`').count() % 2 == 0
            {
                trimmed[..idx].trim_end()
            } else {
                trimmed
            }
        } else {
            trimmed
        };

        if !line_no_comment.is_empty() {
            result.push_str(line_no_comment);
            result.push('\n');
        }

        // If this line opened an unterminated template literal, following lines
        // are string data.
        if count_unescaped_backticks(line_no_comment) % 2 == 1 {
            in_template = true;
        }
    }

    result
}

/// Count backticks not preceded by a backslash.
fn count_unescaped_backticks(s: &str) -> usize {
    let bytes = s.as_bytes();
    let (mut n, mut i) = (0usize, 0usize);
    while i < bytes.len() {
        match bytes[i] {
            b'\\' => {
                i += 2;
                continue;
            }
            b'`' => n += 1,
            _ => {}
        }
        i += 1;
    }
    n
}

#[cfg(test)]
mod tests {
    use super::basic_minify;

    // Issue #193 (related): a `//`-leading continuation line inside a template
    // literal (e.g. a URL) was deleted as a comment.
    #[test]
    fn keeps_slashes_inside_template_literal() {
        let src = "const u = `\n//not-a-comment/here\n`;\n";
        let out = basic_minify(src);
        assert!(
            out.contains("//not-a-comment/here"),
            "template body dropped: {out:?}"
        );
    }

    #[test]
    fn still_strips_real_comment_lines() {
        let src = "// header\nconst x = 1;\n";
        let out = basic_minify(src);
        assert!(!out.contains("// header"));
        assert!(out.contains("const x = 1;"));
    }
}
