// PythScribe runtime crate — holds the JS runtime files.
// The actual runtime is in the js/ directory.

// A1 (single source of truth): the runtime source lives in the npm package
// (../../../runtime/src/) and is embedded via runtime_package_files() below.
// The old runtime_js()/types_js()/operators_js()/core_js() accessors read a
// second, drifting js/ copy and had no callers — removed. The remaining
// crates/pyths_runtime/js/{runtime,operators}.js are thin re-export shims of
// runtime/src, kept only so the JS unit tests exercise the canonical runtime.

// ─── A0: materializable `node_modules/pyths-runtime` package ──────────
//
// `pyths run` compiles a `.ps` file to a `.mjs` and writes it to the OS
// temp dir, then spawns `node` on it directly. `codegen_inline` inlines
// the small helper functions, but stdlib imports (`from math import
// sqrt` etc.) still emit bare `import ... from "pyths-runtime/stdlib/
// math"` specifiers — Node has to actually resolve the `pyths-runtime`
// package from `node_modules` starting at the temp file's directory.
// Since nothing put a `pyths-runtime` package there, every stdlib
// import failed with ERR_MODULE_NOT_FOUND.
//
// Fix: embed the real npm package (`runtime/`) here as
// `include_str!` pairs, keyed by their path *relative to the package
// root* (i.e. matching `runtime/package.json`'s `exports` map exactly).
// `pyths_cli::commands::run` materializes these under
// `<temp_dir>/node_modules/pyths-runtime/...` before spawning node, so
// Node's normal upward `node_modules` resolution finds a real package.
// No specifier rewriting needed — this is exactly what `npm install
// pyths-runtime` would have produced.

/// `(path relative to the `pyths-runtime` package root, file contents)`
/// for every file the compiled output can import. Mirrors `runtime/`
/// (the published npm package) exactly — the `package.json` "exports"
/// map is the canonical list of what's reachable, plus the internal
/// files those entry points import.
pub fn runtime_package_files() -> &'static [(&'static str, &'static str)] {
    &[
        (
            "package.json",
            include_str!("../../../runtime/package.json"),
        ),
        ("README.md", include_str!("../../../runtime/README.md")),
        // #177: the asyncio shim lives at the package ROOT (exports map:
        // "./asyncio" -> "./asyncio.js") — omitting it made `import asyncio`
        // fail under `pyths run` while stdlib modules worked.
        ("asyncio.js", include_str!("../../../runtime/asyncio.js")),
        // Entry points + their internal (non-exported) dependencies.
        (
            "src/index.js",
            include_str!("../../../runtime/src/index.js"),
        ),
        ("src/core.js", include_str!("../../../runtime/src/core.js")),
        ("src/dom.js", include_str!("../../../runtime/src/dom.js")),
        (
            "src/react.js",
            include_str!("../../../runtime/src/react.js"),
        ),
        (
            "src/classes.js",
            include_str!("../../../runtime/src/classes.js"),
        ),
        (
            "src/runtime.js",
            include_str!("../../../runtime/src/runtime.js"),
        ),
        (
            "src/types.js",
            include_str!("../../../runtime/src/types.js"),
        ),
        (
            "src/operators.js",
            include_str!("../../../runtime/src/operators.js"),
        ),
        ("src/web.js", include_str!("../../../runtime/src/web.js")),
        (
            "src/web/index.js",
            include_str!("../../../runtime/src/web/index.js"),
        ),
        (
            "src/web/fetch.js",
            include_str!("../../../runtime/src/web/fetch.js"),
        ),
        (
            "src/web/storage.js",
            include_str!("../../../runtime/src/web/storage.js"),
        ),
        (
            "src/web/router.js",
            include_str!("../../../runtime/src/web/router.js"),
        ),
        (
            "src/utils/tenacity.js",
            include_str!("../../../runtime/src/utils/tenacity.js"),
        ),
        // Standard library — one exports entry per module, plus the
        // barrel `stdlib/index.js` (not individually exported, but kept
        // for parity / any `pyths-runtime/stdlib` bulk import).
        (
            "src/stdlib/index.js",
            include_str!("../../../runtime/src/stdlib/index.js"),
        ),
        (
            "src/stdlib/math.js",
            include_str!("../../../runtime/src/stdlib/math.js"),
        ),
        (
            "src/stdlib/operator.js",
            include_str!("../../../runtime/src/stdlib/operator.js"),
        ),
        (
            "src/stdlib/json.js",
            include_str!("../../../runtime/src/stdlib/json.js"),
        ),
        (
            "src/stdlib/itertools.js",
            include_str!("../../../runtime/src/stdlib/itertools.js"),
        ),
        (
            "src/stdlib/functools.js",
            include_str!("../../../runtime/src/stdlib/functools.js"),
        ),
        (
            "src/stdlib/collections.js",
            include_str!("../../../runtime/src/stdlib/collections.js"),
        ),
        (
            "src/stdlib/random.js",
            include_str!("../../../runtime/src/stdlib/random.js"),
        ),
        (
            "src/stdlib/datetime.js",
            include_str!("../../../runtime/src/stdlib/datetime.js"),
        ),
        (
            "src/stdlib/re.js",
            include_str!("../../../runtime/src/stdlib/re.js"),
        ),
        (
            "src/stdlib/decimal.js",
            include_str!("../../../runtime/src/stdlib/decimal.js"),
        ),
        (
            "src/stdlib/fractions.js",
            include_str!("../../../runtime/src/stdlib/fractions.js"),
        ),
        (
            "src/stdlib/copy.js",
            include_str!("../../../runtime/src/stdlib/copy.js"),
        ),
        (
            "src/stdlib/string.js",
            include_str!("../../../runtime/src/stdlib/string.js"),
        ),
        (
            "src/stdlib/heapq.js",
            include_str!("../../../runtime/src/stdlib/heapq.js"),
        ),
        (
            "src/stdlib/bisect.js",
            include_str!("../../../runtime/src/stdlib/bisect.js"),
        ),
        (
            "src/stdlib/sys.js",
            include_str!("../../../runtime/src/stdlib/sys.js"),
        ),
    ]
}

/// Write [`runtime_package_files`] out under `<node_modules_parent>/
/// node_modules/pyths-runtime/...`, creating directories as needed.
/// Write-if-changed: skips the write when the file on disk already has
/// identical contents, so repeated `pyths run` invocations (or parallel
/// test processes sharing the OS temp dir) don't churn mtimes or race
/// on a write neither of them actually needs to make.
pub fn materialize_runtime_package(node_modules_parent: &std::path::Path) -> std::io::Result<()> {
    let pkg_root = node_modules_parent
        .join("node_modules")
        .join("pyths-runtime");
    for (rel_path, contents) in runtime_package_files() {
        let dest = pkg_root.join(rel_path);
        if let Some(parent) = dest.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let needs_write = match std::fs::read(&dest) {
            Ok(existing) => existing != contents.as_bytes(),
            Err(_) => true,
        };
        if needs_write {
            std::fs::write(&dest, contents)?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// #177 regression guard: every target of the package.json "exports" map
    /// must be present in `runtime_package_files()`, or the corresponding
    /// specifier fails with ERR_MODULE_NOT_FOUND under `pyths run` (that is
    /// exactly how "./asyncio" and "./stdlib/operator" slipped through).
    /// Hand-parse the embedded package.json to avoid a serde dependency:
    /// inside the "exports" object, every value is a "./path.js" string.
    #[test]
    fn exports_map_fully_materialized() {
        let listed: std::collections::HashSet<&str> =
            runtime_package_files().iter().map(|(p, _)| *p).collect();
        let pkg = runtime_package_files()
            .iter()
            .find(|(p, _)| *p == "package.json")
            .expect("package.json embedded")
            .1;
        let exports_start = pkg.find("\"exports\"").expect("exports map present");
        let body = &pkg[exports_start..];
        let end = body
            .find("},")
            .or_else(|| body.find("}\n"))
            .unwrap_or(body.len());
        let mut checked = 0;
        for cap in body[..end].split('"') {
            if let Some(rel) = cap.strip_prefix("./") {
                if rel.ends_with(".js") {
                    assert!(
                        listed.contains(rel),
                        "exports-map target {} is not materialized by runtime_package_files()",
                        rel
                    );
                    checked += 1;
                }
            }
        }
        assert!(
            checked >= 20,
            "exports parse looks broken (only {} targets found)",
            checked
        );
    }
}
