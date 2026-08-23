//! DX-4 (step-into): runtime ignore-list source maps — generator + sync guard.
//!
//! Every SHIPPED runtime `.js` (the canonical npm package `runtime/` plus the
//! `crates/pyths_runtime/js/` re-export shims) carries:
//!   1. a sibling identity `<file>.js.map` whose single source is the file
//!      itself and whose `ignoreList`/`x_google_ignoreList` mark that source
//!      (`[0]`), so DevTools "step into" skips pyths-runtime internals and
//!      stays in the developer's `.ps` code (the anti-Transcrypt property);
//!   2. a trailing `//# sourceMappingURL=<file>.js.map` comment that binds
//!      the `.js` to that map.
//!
//! ONE audited generator produces all of these (deep-fix discipline: no
//! hand-written per-file maps that can drift). It is this test file:
//!
//!   - default (`cargo test`): VERIFY mode — fails if any runtime `.js` is
//!     missing its comment, missing its map, or its map is stale relative to
//!     the current file contents (the two runtime copies therefore cannot
//!     drift apart silently);
//!   - `PYTHS_REGEN_RUNTIME_MAPS=1 cargo test -p pyths_cli --test runtime_maps`:
//!     REGEN mode — rewrites the comments + maps in place, then verifies.
//!
//! Excluded: `*.test.js` / `*.mjs` files (test harnesses, never shipped).

use std::path::{Path, PathBuf};

use pyths_codegen_js::sourcemap::identity_ignore_map;

const MAP_COMMENT_PREFIX: &str = "//# sourceMappingURL=";

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .canonicalize()
        .expect("repo root")
}

/// Every shipped runtime `.js` file, across BOTH runtime copies.
fn runtime_js_files() -> Vec<PathBuf> {
    let root = repo_root();
    let mut files = Vec::new();
    collect_js(&root.join("runtime").join("src"), &mut files);
    files.push(root.join("runtime").join("asyncio.js"));
    collect_js(
        &root.join("crates").join("pyths_runtime").join("js"),
        &mut files,
    );
    files.sort();
    assert!(
        files.len() >= 30,
        "runtime file walk looks broken (found only {} files)",
        files.len()
    );
    files
}

fn collect_js(dir: &Path, out: &mut Vec<PathBuf>) {
    for entry in std::fs::read_dir(dir).unwrap_or_else(|e| panic!("read {dir:?}: {e}")) {
        let path = entry.expect("dir entry").path();
        if path.is_dir() {
            collect_js(&path, out);
            continue;
        }
        let name = path.file_name().unwrap().to_string_lossy().to_string();
        if name.ends_with(".js") && !name.contains(".test.") {
            out.push(path);
        }
    }
}

/// Newline-insensitive equality (git `core.autocrlf` checkouts differ).
fn norm(s: &str) -> String {
    s.replace("\r\n", "\n")
}

/// The expected `(final_js, map_json)` pair for one runtime file, derived
/// solely from its current contents — the single source of truth the guard
/// compares against and REGEN writes out.
fn expected_pair(js_path: &Path) -> (String, String) {
    let raw = std::fs::read_to_string(js_path).unwrap_or_else(|e| panic!("read {js_path:?}: {e}"));
    let newline = if raw.contains("\r\n") { "\r\n" } else { "\n" };
    // Strip any existing trailing sourceMappingURL comment (idempotent regen).
    let mut lines: Vec<&str> = raw.lines().collect();
    while matches!(lines.last(), Some(l) if l.trim_start().starts_with(MAP_COMMENT_PREFIX)) {
        lines.pop();
    }
    while matches!(lines.last(), Some(l) if l.trim().is_empty()) {
        lines.pop();
    }
    let file_name = js_path.file_name().unwrap().to_string_lossy().to_string();
    let map_name = format!("{}.map", file_name);
    lines.push(""); // blank line before the map comment
    let comment = format!("{}{}", MAP_COMMENT_PREFIX, map_name);
    lines.push(&comment);
    let mut final_js = lines.join(newline);
    final_js.push_str(newline);
    let map_json = identity_ignore_map(&final_js, &file_name);
    (final_js, map_json)
}

fn map_path_for(js_path: &Path) -> PathBuf {
    let mut p = js_path.as_os_str().to_owned();
    p.push(".map");
    PathBuf::from(p)
}

#[test]
fn runtime_maps_in_sync() {
    let regen = std::env::var("PYTHS_REGEN_RUNTIME_MAPS").is_ok_and(|v| v == "1");
    let mut stale: Vec<String> = Vec::new();

    for js_path in runtime_js_files() {
        let (final_js, map_json) = expected_pair(&js_path);
        let map_path = map_path_for(&js_path);

        if regen {
            let current = std::fs::read_to_string(&js_path).unwrap();
            if norm(&current) != norm(&final_js) {
                std::fs::write(&js_path, &final_js).unwrap();
            }
            std::fs::write(&map_path, format!("{}\n", map_json)).unwrap();
        }

        // VERIFY (also after regen): comment bound, map present, map fresh.
        let js_now = std::fs::read_to_string(&js_path).unwrap();
        if norm(&js_now) != norm(&final_js) {
            stale.push(format!(
                "{}: missing/incorrect trailing `{}` comment",
                js_path.display(),
                MAP_COMMENT_PREFIX
            ));
            continue;
        }
        match std::fs::read_to_string(&map_path) {
            Ok(map_now) => {
                if norm(map_now.trim_end()) != norm(&map_json) {
                    stale.push(format!("{}: stale identity map", map_path.display()));
                }
            }
            Err(_) => stale.push(format!("{}: sibling map missing", map_path.display())),
        }
    }

    assert!(
        stale.is_empty(),
        "runtime ignore-list maps out of sync — rerun the generator:\n  \
         PYTHS_REGEN_RUNTIME_MAPS=1 cargo test -p pyths_cli --test runtime_maps\n\n{}",
        stale.join("\n")
    );
}

/// MUST-HAVE (b): the runtime ENTRY files ship a sibling `.js.map` that
/// carries BOTH ignore-list keys marking the file itself (`sources[0]`), an
/// identity `mappings` string covering every line, and no inlined content.
#[test]
fn runtime_entry_maps_carry_ignore_list() {
    let root = repo_root();
    for rel in [
        "runtime/src/index.js",
        "runtime/src/core.js",
        "runtime/src/runtime.js",
        "runtime/src/operators.js",
        "crates/pyths_runtime/js/runtime.js",
        "crates/pyths_runtime/js/operators.js",
    ] {
        let js_path = root.join(rel);
        let map_path = map_path_for(&js_path);
        let map: serde_json::Value = serde_json::from_str(
            &std::fs::read_to_string(&map_path)
                .unwrap_or_else(|e| panic!("missing sibling map {map_path:?}: {e}")),
        )
        .expect("valid JSON map");

        let file_name = js_path.file_name().unwrap().to_string_lossy().to_string();
        assert_eq!(map["version"], 3, "{rel}");
        assert_eq!(map["sources"], serde_json::json!([file_name]), "{rel}");
        assert_eq!(map["ignoreList"], serde_json::json!([0]), "{rel}");
        assert_eq!(map["x_google_ignoreList"], serde_json::json!([0]), "{rel}");
        assert_eq!(
            map["ignoreList"], map["x_google_ignoreList"],
            "{rel}: the two ignore-list keys must be identical"
        );

        // Identity mapping covers the whole file: one `AAAA`/`AACA` segment
        // per line, so DevTools associates EVERY runtime line with the
        // ignore-listed source (partial coverage would leave steppable holes).
        let js_now = std::fs::read_to_string(&js_path).unwrap();
        let line_count = js_now.lines().count();
        let mappings = map["mappings"].as_str().unwrap();
        assert_eq!(
            mappings.split(';').count(),
            line_count,
            "{rel}: identity map must cover all {line_count} lines"
        );
        assert!(mappings.starts_with("AAAA"), "{rel}: not an identity map");

        // The `.js` itself must reference the map.
        let expected_ref = format!("{}{}.map", MAP_COMMENT_PREFIX, file_name);
        assert!(
            js_now.trim_end().ends_with(&expected_ref),
            "{rel}: missing trailing `{expected_ref}`"
        );
    }
}
