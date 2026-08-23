//! B5 — the ROOT guard for the WB-22/23 react-helper routing table: import
//! every emitted identifier from its DECLARED module against the REAL installed
//! React 19 / react-dom 19 packages. The old table tests only asserted the
//! table against ITSELF, so a symbol mapped to a module that does not export it
//! (`findDOMNode`, removed in React 19) passed a self-referential check and then
//! crashed the app at load with "no such export".
//!
//! This test ITERATES the compiler's own `pub` routing data —
//! `react::REACT_HELPER_TABLE` and `react::REACT_19_REMOVED` — so coverage is
//! exhaustive BY CONSTRUCTION: every table entry's (jsIdentifier, module) pair
//! (via the same `snake_to_camel`) is imported from the real package by a real
//! `node`; a nonexistent export fails the test, and every removed symbol is
//! asserted GENUINELY ABSENT from its would-be module. There is no second
//! manually-synced name list that a new table entry could silently miss.
//!
//! Requires react 19 installed. Discovery: `PYTHS_REACT_NODE_MODULES` env var,
//! else a small set of sibling `frontend/node_modules` candidates. If none is
//! found (e.g. a minimal CI checkout), the test SKIPS (green) — the pure-Rust
//! `react::react_19_removed` unit tests still run everywhere.

use std::path::{Path, PathBuf};
use std::process::Command;

use pyths_codegen_js::react::{self, ReactHelperSource};

fn is_react19(node_modules: &Path) -> bool {
    let check = |pkg: &str| {
        let p = node_modules.join(pkg).join("package.json");
        std::fs::read_to_string(&p)
            .ok()
            .and_then(|s| {
                s.split("\"version\"")
                    .nth(1)
                    .map(|t| t.trim_start_matches([':', ' ', '"']).to_string())
            })
            .is_some_and(|v| v.starts_with("19"))
    };
    check("react") && check("react-dom")
}

fn discover_node_modules() -> Option<PathBuf> {
    if let Ok(p) = std::env::var("PYTHS_REACT_NODE_MODULES") {
        let pb = PathBuf::from(p);
        if is_react19(&pb) {
            return Some(pb);
        }
    }
    // Walk up from the crate manifest and probe sibling frontend installs.
    let manifest = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let mut base = manifest.as_path();
    for _ in 0..7 {
        for rel in [
            "reference-app/frontend/node_modules",
            "reference-app/frontend/node_modules",
            "frontend/node_modules",
        ] {
            let cand = base.join(rel);
            if is_react19(&cand) {
                return Some(cand);
            }
        }
        base = match base.parent() {
            Some(p) => p,
            None => break,
        };
    }
    None
}

#[test]
fn every_routed_identifier_exists_in_real_react19_packages() {
    let Some(nm) = discover_node_modules() else {
        eprintln!(
            "SKIP react boundary test: no react 19 node_modules found \
             (set PYTHS_REACT_NODE_MODULES)"
        );
        return;
    };
    // Build the (jsIdent, module) checklist by iterating THE routing table
    // itself — `react::REACT_HELPER_TABLE`, the same `pub` data the compiler
    // routes with. Exhaustive BY CONSTRUCTION: an entry added to the table is
    // automatically verified against the real packages; there is no second
    // manually-"kept-in-sync" list to forget (the old ROUTABLE array was
    // exactly that escape hatch — a new `react_helper_source` arm not mirrored
    // into it was silently untested).
    let mut checks: Vec<(String, String)> = Vec::new();
    for &(name, src) in react::REACT_HELPER_TABLE {
        if src == ReactHelperSource::PythsRuntime {
            // The runtime meta-helpers live in pyths-runtime/react, not a
            // public React package.
            continue;
        }
        if react::react_19_removed(name).is_some() {
            // Removed symbols keep a would-be route in the table; they are
            // asserted genuinely ABSENT below instead.
            continue;
        }
        let ident = react::snake_to_camel(name);
        checks.push((ident, src.module().to_string()));
    }
    assert!(
        checks.len() >= 30,
        "table-driven checklist suspiciously small ({}) — table iteration broken?",
        checks.len()
    );
    // Removed symbols must be genuinely ABSENT from their would-be module —
    // the FULL react_19_removed set, iterated from the same pub table.
    let removed: Vec<(String, String)> = react::REACT_19_REMOVED
        .iter()
        .map(|e| {
            assert!(
                react::REACT_HELPER_TABLE.iter().any(|(n, _)| *n == e.py_name),
                "{} must have a would-be route in REACT_HELPER_TABLE",
                e.py_name
            );
            (
                e.js_name.to_string(),
                react::react_helper_source(e.py_name).module().to_string(),
            )
        })
        .collect();
    assert_eq!(
        removed.len(),
        5,
        "the audited React-19 removed set is findDOMNode/render/hydrate/\
         unmountComponentAtNode/createFactory — re-verify against the real \
         packages if this changes"
    );

    let checks_js = checks
        .iter()
        .map(|(i, m)| format!("[{i:?},{m:?}]"))
        .collect::<Vec<_>>()
        .join(",");
    let removed_js = removed
        .iter()
        .map(|(i, m)| format!("[{i:?},{m:?}]"))
        .collect::<Vec<_>>()
        .join(",");

    let runner = format!(
        r#"
const checks = [{checks_js}];
const removed = [{removed_js}];
const mods = {{}};
async function ns(m) {{ if (!(m in mods)) mods[m] = await import(m); return mods[m]; }}
function has(n, ident) {{ return n[ident] !== undefined || (n.default && n.default[ident] !== undefined); }}
const missing = [];
for (const [ident, m] of checks) {{ if (!has(await ns(m), ident)) missing.push(ident + " <- " + m); }}
const leaked = [];
for (const [ident, m] of removed) {{ if (has(await ns(m), ident)) leaked.push(ident + " <- " + m); }}
console.log(JSON.stringify({{ missing, leaked }}));
"#,
    );

    // The runner must resolve bare specifiers from the package root (parent of
    // node_modules). Write it there, run, then remove it.
    let parent = nm.parent().expect("node_modules has a parent");
    let runner_path = parent.join(format!("__pyths_react_boundary_{}.mjs", std::process::id()));
    std::fs::write(&runner_path, runner).expect("write runner");
    let out = Command::new("node").arg(&runner_path).output();
    let _ = std::fs::remove_file(&runner_path);
    let out = match out {
        Ok(o) => o,
        Err(_) => {
            eprintln!("SKIP react boundary test: node unavailable");
            return;
        }
    };
    let stdout = String::from_utf8_lossy(&out.stdout);
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        out.status.success(),
        "node runner failed:\n{stderr}\n--- runner ---\n(node_modules: {})",
        nm.display()
    );
    let line = stdout.trim().lines().last().unwrap_or("");
    // Cheap JSON scrape (no serde dep in this test): both arrays must be empty.
    assert!(
        line.contains("\"missing\":[]"),
        "some routed identifiers are NOT exported by the real react 19 packages: {line}"
    );
    assert!(
        line.contains("\"leaked\":[]"),
        "a react_19_removed symbol is unexpectedly present in the real package \
         (update react_19_removed): {line}"
    );
}
