//! `pyths_config` — project-level configuration via `pyths.toml`.
//!
//! Cargo-style discovery: walks upward from the current working directory
//! looking for a `pyths.toml`. Never errors at the public surface — a
//! missing or malformed file falls back to defaults so a stray TOML typo
//! cannot break a user's build.
//!
//! ## Schema (all sections optional)
//!
//! ```toml
//! [expand]
//! mode = "auto"                       # auto (default) | always | never
//!
//! [expand.dictionary]                 # user `$NAME` aliases (layered atop bundled)
//! BRAND_GRAY = "\"#9ca3af\""
//!
//! [expand.idioms]                     # Tier E `%NAME` → canonical code fragment
//! # For multi-line code fragments, use TOML multiline literal strings ('''...''').
//! # This preserves newlines and quotes verbatim without escape processing.
//! HTTPCHECK = '''
//! if not response.ok:
//!     raise Exception(f"HTTP {response.status}")
//! return await response.json()
//! '''
//!
//! [stubs]
//! paths = ["./stubs", "../shared/stubs"]
//!
//! [npm.imports]                       # python_name = "npm-name"
//! foo_bar = "@scope/foo-bar"
//! ```

use serde::Deserialize;
use std::collections::HashMap;
use std::path::{Path, PathBuf};

/// How aggressively to run the `.psc` expander.
///
/// * `Auto` (default): `.psc` files expand, `.ps` files pass through.
/// * `Always`: every input is expanded — useful for testing alias sugar
///   in `.ps` files.
/// * `Never`: never expand, even on `.psc`. Useful for debugging the
///   raw token stream.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ExpandMode {
    #[default]
    Auto,
    Always,
    Never,
}

impl ExpandMode {
    // clippy: public inherent `from_str` is part of the config API; renaming would break callers
    #[allow(clippy::should_implement_trait)]
    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "auto" => Some(ExpandMode::Auto),
            "always" => Some(ExpandMode::Always),
            "never" => Some(ExpandMode::Never),
            _ => None,
        }
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            ExpandMode::Auto => "auto",
            ExpandMode::Always => "always",
            ExpandMode::Never => "never",
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct ExpandConfig {
    pub mode: ExpandMode,
    pub dictionary: HashMap<String, String>,
    /// Tier E-fixed: `%NAME` → canonical code fragment map, from `[expand.idioms]`.
    pub idioms: HashMap<String, String>,
}

#[derive(Debug, Clone, Default)]
pub struct StubsConfig {
    pub paths: Vec<PathBuf>,
}

#[derive(Debug, Clone, Default)]
pub struct NpmConfig {
    pub imports: HashMap<String, String>,
}

#[derive(Debug, Clone, Default)]
pub struct ProjectConfig {
    pub expand: ExpandConfig,
    pub stubs: StubsConfig,
    pub npm: NpmConfig,
    /// Path to the `pyths.toml` that produced this config, if any. `None`
    /// means defaults (no file found).
    pub source: Option<PathBuf>,
}

// --- Raw deserialization layer (mirrors the TOML schema 1:1). ---------------

#[derive(Deserialize, Default)]
struct RawConfig {
    #[serde(default)]
    expand: Option<RawExpand>,
    #[serde(default)]
    stubs: Option<RawStubs>,
    #[serde(default)]
    npm: Option<RawNpm>,
}

#[derive(Deserialize, Default)]
struct RawExpand {
    #[serde(default)]
    mode: Option<String>,
    #[serde(default)]
    dictionary: Option<HashMap<String, String>>,
    /// Tier E-fixed: `[expand.idioms]` table — mirrors `[expand.dictionary]`.
    #[serde(default)]
    idioms: Option<HashMap<String, String>>,
}

#[derive(Deserialize, Default)]
struct RawStubs {
    #[serde(default)]
    paths: Option<Vec<String>>,
}

#[derive(Deserialize, Default)]
struct RawNpm {
    #[serde(default)]
    imports: Option<HashMap<String, String>>,
}

impl RawConfig {
    fn into_project(self, source: Option<PathBuf>) -> ProjectConfig {
        let expand = self.expand.unwrap_or_default();
        let stubs = self.stubs.unwrap_or_default();
        let npm = self.npm.unwrap_or_default();

        let base = source
            .as_ref()
            .and_then(|p| p.parent().map(Path::to_path_buf));

        ProjectConfig {
            expand: ExpandConfig {
                mode: expand
                    .mode
                    .as_deref()
                    .and_then(ExpandMode::from_str)
                    .unwrap_or_default(),
                dictionary: expand.dictionary.unwrap_or_default(),
                idioms: expand.idioms.unwrap_or_default(),
            },
            stubs: StubsConfig {
                paths: stubs
                    .paths
                    .unwrap_or_default()
                    .into_iter()
                    .map(|p| match &base {
                        Some(b) => b.join(p),
                        None => PathBuf::from(p),
                    })
                    .collect(),
            },
            npm: NpmConfig {
                imports: npm.imports.unwrap_or_default(),
            },
            source,
        }
    }
}

// --- Public loaders. --------------------------------------------------------

/// Walk upward from `start_dir` looking for a `pyths.toml`. Returns the
/// parsed config if found and valid; returns an error if found but
/// malformed (callers using `load_or_default` swallow that case).
pub fn load_from(start_dir: &Path) -> Result<ProjectConfig, ConfigError> {
    let Some(path) = find_pyths_toml(start_dir) else {
        return Ok(ProjectConfig::default());
    };
    let text =
        std::fs::read_to_string(&path).map_err(|e| ConfigError::Io(path.clone(), e.to_string()))?;
    let raw: RawConfig =
        toml::from_str(&text).map_err(|e| ConfigError::Parse(path.clone(), e.to_string()))?;
    Ok(raw.into_project(Some(path)))
}

/// Same as [`load_from`] starting at the current working directory.
pub fn load_from_cwd() -> Result<ProjectConfig, ConfigError> {
    let cwd =
        std::env::current_dir().map_err(|e| ConfigError::Io(PathBuf::new(), e.to_string()))?;
    load_from(&cwd)
}

/// Infallible loader. Missing file → defaults. Malformed file → warning on
/// stderr, then defaults. Intended for the CLI happy path where a typo in
/// `pyths.toml` must never break compilation.
pub fn load_or_default() -> ProjectConfig {
    match load_from_cwd() {
        Ok(cfg) => cfg,
        Err(e) => {
            eprintln!("warning: {} — using defaults", e);
            ProjectConfig::default()
        }
    }
}

fn find_pyths_toml(start: &Path) -> Option<PathBuf> {
    let mut cur: Option<&Path> = Some(start);
    while let Some(dir) = cur {
        let candidate = dir.join("pyths.toml");
        if candidate.is_file() {
            return Some(candidate);
        }
        cur = dir.parent();
    }
    None
}

#[derive(Debug)]
pub enum ConfigError {
    Io(PathBuf, String),
    Parse(PathBuf, String),
}

impl std::fmt::Display for ConfigError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ConfigError::Io(p, msg) => write!(f, "could not read {}: {}", p.display(), msg),
            ConfigError::Parse(p, msg) => write!(f, "could not parse {}: {}", p.display(), msg),
        }
    }
}

impl std::error::Error for ConfigError {}

// ----------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::sync::atomic::{AtomicU64, Ordering};

    static COUNTER: AtomicU64 = AtomicU64::new(0);

    /// Each test gets its own scratch directory so they never collide with
    /// the user's real `pyths.toml` (which might exist anywhere up the
    /// tree from the test runner's cwd).
    fn scratch_dir() -> PathBuf {
        let id = COUNTER.fetch_add(1, Ordering::SeqCst);
        let pid = std::process::id();
        let dir = std::env::temp_dir().join(format!("pyths_config_test_{}_{}", pid, id));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn no_file_returns_defaults() {
        let dir = scratch_dir();
        let cfg = load_from(&dir).expect("ok");
        assert_eq!(cfg.expand.mode, ExpandMode::Auto);
        assert!(cfg.expand.dictionary.is_empty());
        assert!(cfg.stubs.paths.is_empty());
        assert!(cfg.npm.imports.is_empty());
        assert!(cfg.source.is_none());
    }

    #[test]
    fn empty_toml_uses_defaults() {
        let dir = scratch_dir();
        fs::write(dir.join("pyths.toml"), "").unwrap();
        let cfg = load_from(&dir).expect("ok");
        assert_eq!(cfg.expand.mode, ExpandMode::Auto);
        assert!(cfg.source.is_some());
    }

    #[test]
    fn expand_mode_never_parses() {
        let dir = scratch_dir();
        fs::write(dir.join("pyths.toml"), "[expand]\nmode = \"never\"\n").unwrap();
        let cfg = load_from(&dir).expect("ok");
        assert_eq!(cfg.expand.mode, ExpandMode::Never);
    }

    #[test]
    fn expand_mode_always_parses() {
        let dir = scratch_dir();
        fs::write(dir.join("pyths.toml"), "[expand]\nmode = \"always\"\n").unwrap();
        let cfg = load_from(&dir).expect("ok");
        assert_eq!(cfg.expand.mode, ExpandMode::Always);
    }

    #[test]
    fn expand_dictionary_populated() {
        let dir = scratch_dir();
        fs::write(
            dir.join("pyths.toml"),
            "[expand.dictionary]\nFOO = \"bar.baz.Qux\"\nBRAND = \"#9ca3af\"\n",
        )
        .unwrap();
        let cfg = load_from(&dir).expect("ok");
        assert_eq!(
            cfg.expand.dictionary.get("FOO").map(String::as_str),
            Some("bar.baz.Qux")
        );
        assert_eq!(
            cfg.expand.dictionary.get("BRAND").map(String::as_str),
            Some("#9ca3af")
        );
    }

    #[test]
    fn expand_idioms_populated() {
        let dir = scratch_dir();
        // TOML basic strings interpret \n as a real newline character.
        // Use a TOML literal string (single-quoted) for the multi-line
        // fragment to avoid TOML escape processing.
        fs::write(
            dir.join("pyths.toml"),
            "[expand.idioms]\nHTTPCHECK = 'if not response.ok:\\n    raise Exception(f\"HTTP {response.status}\")\\nreturn await response.json()'\nLOG = 'print(f\"[debug] {__name__}\")'\n",
        )
        .unwrap();
        let cfg = load_from(&dir).expect("ok");
        // TOML literal strings (single-quoted) are taken verbatim — backslashes
        // are NOT interpreted, so \n stays as the two chars `\` + `n`.
        assert_eq!(
            cfg.expand.idioms.get("HTTPCHECK").map(String::as_str),
            Some("if not response.ok:\\n    raise Exception(f\"HTTP {response.status}\")\\nreturn await response.json()")
        );
        assert_eq!(
            cfg.expand.idioms.get("LOG").map(String::as_str),
            Some("print(f\"[debug] {__name__}\")")
        );
    }

    #[test]
    fn expand_idioms_multiline_literal_strings() {
        let dir = scratch_dir();
        // TOML multiline literal strings ('''...''') preserve newlines and quotes verbatim.
        let toml_content = r#"[expand.idioms]
HTTPCHECK = '''
if not response.ok:
    raise Exception(f"HTTP {response.status}")
return await response.json()
'''
"#;
        fs::write(dir.join("pyths.toml"), toml_content).unwrap();
        let cfg = load_from(&dir).expect("ok");
        let httpcheck = cfg
            .expand
            .idioms
            .get("HTTPCHECK")
            .expect("HTTPCHECK should exist");
        // Multiline literal strings should contain real newline chars (not escaped \n),
        // and inner double-quotes should be preserved.
        assert!(
            httpcheck.contains('\n'),
            "multiline idiom should contain real newlines"
        );
        assert!(
            httpcheck.contains("f\"HTTP"),
            "inner double-quotes should be preserved"
        );
        // The actual content should be verbatim from the TOML source.
        assert_eq!(
            httpcheck,
            "if not response.ok:\n    raise Exception(f\"HTTP {response.status}\")\nreturn await response.json()\n"
        );
    }

    #[test]
    fn expand_idioms_empty_by_default() {
        let dir = scratch_dir();
        fs::write(dir.join("pyths.toml"), "[expand]\nmode = \"auto\"\n").unwrap();
        let cfg = load_from(&dir).expect("ok");
        assert!(cfg.expand.idioms.is_empty());
    }

    #[test]
    fn expand_idioms_and_dictionary_coexist() {
        let dir = scratch_dir();
        fs::write(
            dir.join("pyths.toml"),
            "[expand.dictionary]\nBRAND = \"#abc\"\n\n[expand.idioms]\nFOO = \"pass\"\n",
        )
        .unwrap();
        let cfg = load_from(&dir).expect("ok");
        assert_eq!(
            cfg.expand.dictionary.get("BRAND").map(String::as_str),
            Some("#abc")
        );
        assert_eq!(
            cfg.expand.idioms.get("FOO").map(String::as_str),
            Some("pass")
        );
    }

    #[test]
    fn stubs_paths_resolve_relative_to_config_file() {
        let dir = scratch_dir();
        fs::write(
            dir.join("pyths.toml"),
            "[stubs]\npaths = [\"./stubs\", \"../shared\"]\n",
        )
        .unwrap();
        let cfg = load_from(&dir).expect("ok");
        assert_eq!(cfg.stubs.paths.len(), 2);
        // The first path is dir/stubs.
        assert_eq!(cfg.stubs.paths[0], dir.join("stubs"));
    }

    #[test]
    fn npm_imports_populated() {
        let dir = scratch_dir();
        fs::write(
            dir.join("pyths.toml"),
            "[npm.imports]\nfoo_bar = \"@scope/foo-bar\"\n",
        )
        .unwrap();
        let cfg = load_from(&dir).expect("ok");
        assert_eq!(
            cfg.npm.imports.get("foo_bar").map(String::as_str),
            Some("@scope/foo-bar"),
        );
    }

    #[test]
    fn malformed_toml_returns_parse_error() {
        let dir = scratch_dir();
        fs::write(dir.join("pyths.toml"), "this is = not [ valid ] toml ===\n").unwrap();
        let err = load_from(&dir).expect_err("should fail");
        assert!(matches!(err, ConfigError::Parse(_, _)));
    }

    #[test]
    fn unknown_expand_mode_falls_back_to_default() {
        let dir = scratch_dir();
        fs::write(dir.join("pyths.toml"), "[expand]\nmode = \"banana\"\n").unwrap();
        let cfg = load_from(&dir).expect("ok");
        // Unknown mode string is silently ignored; default applies.
        assert_eq!(cfg.expand.mode, ExpandMode::Auto);
    }

    #[test]
    fn walk_up_finds_parent_config() {
        let dir = scratch_dir();
        fs::write(dir.join("pyths.toml"), "[expand]\nmode = \"never\"\n").unwrap();
        let nested = dir.join("a").join("b").join("c");
        fs::create_dir_all(&nested).unwrap();
        let cfg = load_from(&nested).expect("ok");
        assert_eq!(cfg.expand.mode, ExpandMode::Never);
        assert_eq!(cfg.source.as_ref(), Some(&dir.join("pyths.toml")));
    }

    #[test]
    fn expand_mode_round_trip_strings() {
        assert_eq!(ExpandMode::from_str("auto"), Some(ExpandMode::Auto));
        assert_eq!(ExpandMode::from_str("always"), Some(ExpandMode::Always));
        assert_eq!(ExpandMode::from_str("never"), Some(ExpandMode::Never));
        assert_eq!(ExpandMode::from_str("nope"), None);
        assert_eq!(ExpandMode::Auto.as_str(), "auto");
        assert_eq!(ExpandMode::Always.as_str(), "always");
        assert_eq!(ExpandMode::Never.as_str(), "never");
    }
}
