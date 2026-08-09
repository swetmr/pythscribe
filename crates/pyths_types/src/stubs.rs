//! Bundled `.pyi` type stubs for the React/Next.js ecosystem.
//!
//! These stubs ship with the binary and are consulted during `pyths
//! check`. When the type checker sees `from <module> import <name>`,
//! it looks up the module here, parses the stub via `pyths_parser`,
//! and uses the resulting function/class signatures to type-check
//! references to `<name>`.
//!
//! The stubs are hand-authored under `crates/pyths_types/stubs/`. Add
//! a row to `STUBS` to register a new module.
//!
//! ## Resolution order
//!
//! For a given `import` site, the checker tries (in order):
//!   1. Each `pyths.toml [stubs.paths]` directory in declaration
//!      order: `<path>/<module>.pyi`. Project-local stubs override
//!      bundled ones of the same module name.
//!   2. Bundled stub from this module's `STUBS` table.
//!
//! Returning `None` means "no stub available — treat the imported
//! name as `Type::Any`" (today's pre-stub behavior).

/// (python_module_name, .pyi source).
///
/// Module names use the Python-side spelling (with underscores), the
/// same form that appears in `from X import Y` statements.
pub static STUBS: &[(&str, &str)] = &[
    ("react", include_str!("../stubs/react.pyi")),
    ("pyths.react", include_str!("../stubs/react.pyi")),
    ("next", include_str!("../stubs/next.pyi")),
    // Next.js App Router sub-modules. These names use dots in the
    // Python-source import form: `from next.headers import cookies`.
    ("next.headers", include_str!("../stubs/next_headers.pyi")),
    ("next.server", include_str!("../stubs/next_server.pyi")),
    (
        "next.navigation",
        include_str!("../stubs/next_navigation.pyi"),
    ),
    (
        "react_router_dom",
        include_str!("../stubs/react_router_dom.pyi"),
    ),
    (
        "react_router",
        include_str!("../stubs/react_router_dom.pyi"),
    ),
    (
        "at_tanstack.react_query",
        include_str!("../stubs/tanstack_react_query.pyi"),
    ),
    // Ecosystem expansion — most commonly imported React libraries.
    ("lucide_react", include_str!("../stubs/lucide_react.pyi")),
    ("zustand", include_str!("../stubs/zustand.pyi")),
    ("swr", include_str!("../stubs/swr.pyi")),
    (
        "react_hook_form",
        include_str!("../stubs/react_hook_form.pyi"),
    ),
    // W* tier — Cloudflare Worker / Deno / edge runtime helpers.
    // W* expands to: from pyths.web import handler, Response
    ("pyths.web", include_str!("../stubs/web.pyi")),
    // Exact-arithmetic stdlib modules (decimal + fractions).
    ("decimal", include_str!("../stubs/decimal.pyi")),
    ("fractions", include_str!("../stubs/fractions.pyi")),
];

/// Look up the *bundled* stub source for a module. Returns `None` for
/// modules without a bundled stub. Use [`resolve_stub`] to honor
/// project-local `pyths.toml [stubs.paths]` first.
pub fn stub_for(module: &str) -> Option<&'static str> {
    STUBS
        .iter()
        .find(|(m, _)| *m == module)
        .map(|(_, src)| *src)
}

/// Resolve a stub for `module` honoring project-local `stubs.paths`
/// before the bundled table. Each directory is searched in declaration
/// order; the first `<dir>/<module>.pyi` that exists wins. If no
/// project-local match is found, falls back to the bundled stub.
///
/// Returns `None` only when no stub exists in either location.
pub fn resolve_stub(module: &str, project_paths: &[std::path::PathBuf]) -> Option<String> {
    for base in project_paths {
        let candidate = base.join(format!("{}.pyi", module));
        if candidate.is_file() {
            // I/O errors on a file we've already confirmed exists are
            // genuinely surprising (permissions, race). Fall through
            // to bundled rather than failing the whole check pass.
            if let Ok(src) = std::fs::read_to_string(&candidate) {
                return Some(src);
            }
        }
    }
    stub_for(module).map(str::to_string)
}

/// Iterate all bundled module names. Used by tests to confirm the
/// table is non-empty and well-formed.
pub fn iter_modules() -> impl Iterator<Item = &'static str> {
    STUBS.iter().map(|(m, _)| *m)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stubs_table_is_populated() {
        assert!(
            STUBS.len() >= 4,
            "expected at least 4 bundled stubs, got {}",
            STUBS.len()
        );
    }

    #[test]
    fn react_stub_present_and_parses() {
        let src = stub_for("react").expect("react stub registered");
        // The stub is valid Python; pyths_parser should accept it.
        let module = pyths_parser::parse(src).expect("react.pyi parses");
        assert!(!module.body.is_empty(), "stub has declarations");
    }

    #[test]
    fn pyths_react_alias_resolves_to_same_stub() {
        let a = stub_for("react").unwrap();
        let b = stub_for("pyths.react").unwrap();
        assert_eq!(
            a.as_ptr(),
            b.as_ptr(),
            "pyths.react and react should share the same bundled source"
        );
    }

    #[test]
    fn nonexistent_module_returns_none() {
        assert!(stub_for("nonexistent_xyz").is_none());
    }

    #[test]
    fn next_submodule_stubs_registered() {
        // App Router server-side imports — `from next.headers import
        // cookies`, `from next.server import NextRequest`, etc. — must
        // resolve to dedicated stubs separate from the top-level `next`.
        assert!(
            stub_for("next.headers").is_some(),
            "next.headers stub registered"
        );
        assert!(
            stub_for("next.server").is_some(),
            "next.server stub registered"
        );
        assert!(
            stub_for("next.navigation").is_some(),
            "next.navigation stub registered"
        );
    }

    #[test]
    fn next_headers_stub_declares_cookies_and_headers() {
        let src = stub_for("next.headers").expect("next.headers stub");
        assert!(src.contains("def cookies("), "cookies()");
        assert!(src.contains("def headers("), "headers()");
        assert!(src.contains("def draft_mode("), "draft_mode()");
        // Parses cleanly.
        let module = pyths_parser::parse(src).expect("next.headers parses");
        assert!(!module.body.is_empty());
    }

    #[test]
    fn next_navigation_stub_declares_redirect_and_hooks() {
        let src = stub_for("next.navigation").expect("next.navigation stub");
        assert!(src.contains("def redirect("));
        assert!(src.contains("def not_found("));
        assert!(src.contains("def use_pathname("));
        let module = pyths_parser::parse(src).expect("next.navigation parses");
        assert!(!module.body.is_empty());
    }

    #[test]
    fn next_server_stub_declares_request_and_response() {
        let src = stub_for("next.server").expect("next.server stub");
        assert!(src.contains("class NextRequest"));
        assert!(src.contains("class NextResponse"));
        let module = pyths_parser::parse(src).expect("next.server parses");
        assert!(!module.body.is_empty());
    }

    #[test]
    fn ecosystem_stubs_registered() {
        // The 2026-05 ecosystem expansion adds direct stub bundles for
        // the most-imported React libraries beyond React core itself.
        assert!(stub_for("lucide_react").is_some(), "lucide_react");
        assert!(stub_for("zustand").is_some(), "zustand");
        assert!(stub_for("swr").is_some(), "swr");
        assert!(stub_for("react_hook_form").is_some(), "react_hook_form");
    }

    // ---- pyths.web (W* tier) ----

    #[test]
    fn pyths_web_stub_registered() {
        // Fix B-029: W* expands to `from pyths.web import handler, Response`.
        // The stub must be registered so `pyths check` can type-check W* modules.
        assert!(stub_for("pyths.web").is_some(), "pyths.web stub registered");
    }

    #[test]
    fn pyths_web_stub_declares_handler_and_response() {
        let src = stub_for("pyths.web").expect("pyths.web stub");
        assert!(src.contains("def handler("), "handler declared in web.pyi");
        assert!(
            src.contains("class Response"),
            "Response declared in web.pyi"
        );
    }

    #[test]
    fn pyths_web_stub_parses_cleanly() {
        let src = stub_for("pyths.web").expect("pyths.web stub");
        let module = pyths_parser::parse(src).expect("web.pyi parses");
        assert!(!module.body.is_empty(), "web.pyi has declarations");
    }

    #[test]
    fn ecosystem_stubs_parse_cleanly() {
        for module in ["lucide_react", "zustand", "swr", "react_hook_form"] {
            let src = stub_for(module).expect("stub registered");
            pyths_parser::parse(src).expect(&format!("{}.pyi parses", module));
        }
    }

    // ---- decimal / fractions ----

    #[test]
    fn decimal_and_fractions_stubs_registered() {
        assert!(stub_for("decimal").is_some(), "decimal stub registered");
        assert!(stub_for("fractions").is_some(), "fractions stub registered");
    }

    #[test]
    fn decimal_stub_declares_class_and_rounding_constants() {
        let src = stub_for("decimal").expect("decimal stub");
        assert!(
            src.contains("class Decimal"),
            "Decimal declared in decimal.pyi"
        );
        assert!(
            src.contains("ROUND_HALF_EVEN"),
            "ROUND_HALF_EVEN declared in decimal.pyi"
        );
        assert!(
            src.contains("def quantize("),
            "quantize declared in decimal.pyi"
        );
    }

    #[test]
    fn fractions_stub_declares_class_and_properties() {
        let src = stub_for("fractions").expect("fractions stub");
        assert!(
            src.contains("class Fraction"),
            "Fraction declared in fractions.pyi"
        );
        assert!(
            src.contains("numerator:"),
            "numerator declared in fractions.pyi"
        );
        assert!(
            src.contains("denominator:"),
            "denominator declared in fractions.pyi"
        );
    }

    #[test]
    fn decimal_and_fractions_stubs_parse_cleanly() {
        for module in ["decimal", "fractions"] {
            let src = stub_for(module).expect("stub registered");
            let parsed = pyths_parser::parse(src).expect(&format!("{}.pyi parses", module));
            assert!(!parsed.body.is_empty(), "{}.pyi has declarations", module);
        }
    }

    // ---- Project-local stub resolution ----

    use std::path::PathBuf;
    use std::sync::atomic::{AtomicU64, Ordering};

    static SCRATCH_COUNTER: AtomicU64 = AtomicU64::new(0);

    fn scratch_dir(tag: &str) -> PathBuf {
        let id = SCRATCH_COUNTER.fetch_add(1, Ordering::SeqCst);
        let pid = std::process::id();
        let dir = std::env::temp_dir().join(format!("pyths_stub_test_{}_{}_{}", tag, pid, id));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn resolve_stub_returns_bundled_when_no_paths() {
        let result = resolve_stub("react", &[]);
        assert!(result.is_some(), "bundled react stub should resolve");
    }

    #[test]
    fn resolve_stub_returns_none_for_unknown_module() {
        let result = resolve_stub("totally_made_up", &[]);
        assert!(result.is_none());
    }

    #[test]
    fn resolve_stub_finds_project_local_for_new_module() {
        let dir = scratch_dir("new_module");
        std::fs::write(dir.join("my_lib.pyi"), "def greet(name: str) -> str: ...\n").unwrap();

        let result = resolve_stub("my_lib", &[dir.clone()]);
        let src = result.expect("project stub found");
        assert!(src.contains("def greet"), "project stub content: {}", src);

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn project_stub_overrides_bundled() {
        let dir = scratch_dir("override");
        std::fs::write(
            dir.join("react.pyi"),
            "def use_state_v2(initial: int) -> int: ...\n",
        )
        .unwrap();

        let result = resolve_stub("react", &[dir.clone()]);
        let src = result.expect("override stub found");
        assert!(
            src.contains("use_state_v2"),
            "override should win over bundled: {}",
            src
        );
        assert!(
            !src.contains("createElement"),
            "bundled content should NOT leak: {}",
            src
        );

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn missing_project_stub_falls_through_to_bundled() {
        let dir = scratch_dir("falltrough");
        // dir is empty — no react.pyi in it.
        let result = resolve_stub("react", &[dir.clone()]);
        let src = result.expect("falls through to bundled");
        // Bundled react.pyi declares standard hooks like use_state.
        assert!(
            src.contains("use_state"),
            "expected bundled content after fall-through: {}",
            src
        );

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn multiple_paths_searched_in_declaration_order() {
        let dir_a = scratch_dir("multi_a");
        let dir_b = scratch_dir("multi_b");
        std::fs::write(dir_a.join("foo.pyi"), "def from_a() -> int: ...\n").unwrap();
        std::fs::write(dir_b.join("foo.pyi"), "def from_b() -> int: ...\n").unwrap();

        // dir_a comes first → its stub wins.
        let result = resolve_stub("foo", &[dir_a.clone(), dir_b.clone()]);
        let src = result.unwrap();
        assert!(src.contains("from_a"));
        assert!(!src.contains("from_b"));

        // Reversed order → dir_b wins.
        let result = resolve_stub("foo", &[dir_b.clone(), dir_a.clone()]);
        let src = result.unwrap();
        assert!(src.contains("from_b"));
        assert!(!src.contains("from_a"));

        let _ = std::fs::remove_dir_all(&dir_a);
        let _ = std::fs::remove_dir_all(&dir_b);
    }
}
