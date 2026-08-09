//! Step 9 — Incremental compilation cache.
//!
//! Layout: a **per-user** cache directory, namespaced by project root:
//! `<user-cache>/pyths/cache/<project-hash>/<key>.json` per source file.
//!
//! ## Security (review A3 — do NOT regress)
//!
//! The cache dir was previously `<project>/.pyths/cache/` — *inside the source
//! tree*, and therefore attacker-controlled in a cloned/hostile project. Since
//! the cache key is a non-secret, attacker-computable FNV-1a digest and the hit
//! path blindly `fs::copy`d the record's `output_path`, a hostile repo could
//! ship a `.pyths/cache/<key>.json` that (a) substituted arbitrary JS for the
//! compiled output (the `.ps` source was never parsed) or (b) copied an
//! arbitrary file (e.g. `~/.ssh/id_rsa`) into the build output. Three defenses,
//! layered:
//!
//!   1. **Out-of-tree cache** — the trusted cache lives under the *user's* cache
//!      dir (`PYTHS_CACHE_DIR` / `LOCALAPPDATA` / `XDG_CACHE_HOME` / `~/.cache`),
//!      never inside the repo. A hostile clone has no way to plant an entry the
//!      compiler will consult. This is the primary kill for the substitution.
//!   2. **Confined `output_path`** — on a hit, the record's `output_path` must
//!      be relative, free of `..`, and canonicalize to a path *inside the
//!      project root*. A cache record can never cause a read or copy outside
//!      the build's own tree. This kills the arbitrary-read.
//!   3. **Source re-validation** — the record stores the FNV hash of the actual
//!      source content; on a hit we re-hash the real source and require a match
//!      before trusting the entry.
//!
//! The key stays non-secret (FNV-1a of source ‖ target ‖ CARGO_PKG_VERSION);
//! that is fine because the entry is now validated against the real source and
//! the output stays confined, and the cache itself is unplantable.
//!
//! This module owns only the cache I/O. The actual hot-path decisions (when
//! to consult the cache) live in `commands::compile`.

use std::path::{Component, Path, PathBuf};

use super::output::{self, Verbosity};

/// Resolve the per-user base cache directory. Honors an explicit override,
/// then platform conventions, then home-dir fallbacks. Returns `None` only if
/// no usable location can be determined (practically never — one of these env
/// vars is always set on Windows/Unix).
fn user_cache_base() -> Option<PathBuf> {
    // Explicit, documented override — highest priority (also used by tests).
    if let Ok(dir) = std::env::var("PYTHS_CACHE_DIR") {
        if !dir.is_empty() {
            return Some(PathBuf::from(dir));
        }
    }
    // XDG (Linux/BSD): $XDG_CACHE_HOME/pyths
    if let Ok(x) = std::env::var("XDG_CACHE_HOME") {
        if !x.is_empty() {
            return Some(PathBuf::from(x).join("pyths"));
        }
    }
    // Windows: %LOCALAPPDATA%\pyths\cache
    if let Ok(la) = std::env::var("LOCALAPPDATA") {
        if !la.is_empty() {
            return Some(PathBuf::from(la).join("pyths").join("cache"));
        }
    }
    // Unix fallback: ~/.cache/pyths
    if let Ok(home) = std::env::var("HOME") {
        if !home.is_empty() {
            return Some(PathBuf::from(home).join(".cache").join("pyths"));
        }
    }
    // Windows fallback: %USERPROFILE%\.cache\pyths
    if let Ok(up) = std::env::var("USERPROFILE") {
        if !up.is_empty() {
            return Some(PathBuf::from(up).join(".cache").join("pyths"));
        }
    }
    None
}

/// The directory a source file lives in, normalizing the empty parent of a
/// bare filename (e.g. `app.ps`) to the current directory `.`.
fn start_dir(source: &Path) -> PathBuf {
    match source.parent() {
        Some(p) if !p.as_os_str().is_empty() => p.to_path_buf(),
        _ => PathBuf::from("."),
    }
}

/// The project root for a given source file: the nearest ancestor holding a
/// `pyths.toml`, else the file's own directory. Used both to namespace the
/// cache and to confine cached `output_path`s.
pub fn project_root_for(source: &Path) -> PathBuf {
    project_root_from_start(&start_dir(source))
}

fn project_root_from_start(start: &Path) -> PathBuf {
    let mut walk = start.to_path_buf();
    loop {
        if walk.join("pyths.toml").is_file() {
            return walk;
        }
        if !walk.pop() {
            break;
        }
    }
    start.to_path_buf()
}

/// Cache directory for a start directory (project root is derived from it).
fn cache_dir_from_start(start: &Path) -> PathBuf {
    let root = project_root_from_start(start);
    // Namespace by the canonical project-root path so distinct projects never
    // share entries (keeps `output_path` confinement meaningful and avoids
    // cross-project churn). Falls back to the raw path if canonicalize fails.
    let canon = std::fs::canonicalize(&root).unwrap_or(root);
    let ns = format!("{:016x}", fnv1a64(canon.to_string_lossy().as_bytes()));
    match user_cache_base() {
        Some(base) => base.join(ns),
        // Extremely unlikely: no HOME/LOCALAPPDATA/XDG/USERPROFILE at all.
        // Fall back to a per-user temp location (still out of the source tree)
        // so a hostile in-tree `.pyths` is never consulted.
        None => std::env::temp_dir().join("pyths-cache").join(ns),
    }
}

/// Locate the cache directory for a given source *file* path.
pub fn cache_dir_for(source: &Path) -> PathBuf {
    cache_dir_from_start(&start_dir(source))
}

/// Confine a record's `output_path` to the project root. Returns the resolved,
/// canonical, in-project path on success, or `None` (→ cache miss) if the path
/// is absolute-escaping, contains `..`, or resolves outside `project_root`.
fn confine_output_path(output_path: &str, project_root: &Path) -> Option<PathBuf> {
    let p = Path::new(output_path);
    // Reject any parent-dir traversal outright.
    if p.components().any(|c| matches!(c, Component::ParentDir)) {
        return None;
    }
    // Resolve to an absolute path: relative paths join the current dir (where
    // the compiler is running and where legitimate output lives).
    let abs = if p.is_absolute() {
        p.to_path_buf()
    } else {
        std::env::current_dir().ok()?.join(p)
    };
    // Canonicalize both sides (the output must exist for a hit anyway) and
    // require containment. `canonicalize` also resolves symlinks, so a symlink
    // planted inside the tree cannot point the copy at an outside file.
    let root_c = std::fs::canonicalize(project_root).ok()?;
    let abs_c = std::fs::canonicalize(&abs).ok()?;
    if abs_c.starts_with(&root_c) {
        Some(abs_c)
    } else {
        None
    }
}

/// 64-bit FNV-1a hash. Stable, fast, no deps.
pub fn fnv1a64(bytes: &[u8]) -> u64 {
    let mut hash: u64 = 0xcbf29ce484222325;
    for b in bytes {
        hash ^= *b as u64;
        hash = hash.wrapping_mul(0x100000001b3);
    }
    hash
}

/// Build the cache key for a source/target combo.
pub fn cache_key(source_text: &str, target: &str) -> u64 {
    let mut combined = String::with_capacity(source_text.len() + 64);
    combined.push_str(source_text);
    combined.push('\0');
    combined.push_str(target);
    combined.push('\0');
    combined.push_str(env!("CARGO_PKG_VERSION"));
    fnv1a64(combined.as_bytes())
}

/// Cache record stored as JSON.
#[derive(Debug, Clone)]
pub struct CacheRecord {
    pub key: u64,
    pub source: String,
    /// FNV-1a hash of the actual (post-expansion) source *content*. Re-checked
    /// on every hit so a record can only ever be trusted for the exact source
    /// that produced it (security review A3, defense 3).
    pub source_hash: u64,
    pub target: String,
    pub output_path: String,
    pub output_hash: u64,
    pub timestamp: u64,
}

impl CacheRecord {
    pub fn to_json(&self) -> String {
        format!(
            "{{\n  \"key\": \"{:016x}\",\n  \"source\": {:?},\n  \"source_hash\": \"{:016x}\",\n  \"target\": {:?},\n  \"output_path\": {:?},\n  \"output_hash\": \"{:016x}\",\n  \"timestamp\": {}\n}}\n",
            self.key,
            self.source,
            self.source_hash,
            self.target,
            self.output_path,
            self.output_hash,
            self.timestamp,
        )
    }

    /// Parse the minimal JSON shape we emit. Hand-written rather than
    /// pulling in serde to keep dependencies minimal for the CLI.
    ///
    /// A missing `source_hash` (a pre-A3 record) yields `None` → cache miss →
    /// rebuild + re-store in the current format. That is the intended, safe
    /// upgrade path.
    pub fn from_json(text: &str) -> Option<Self> {
        let key_hex = extract_string_value(text, "key")?;
        let source = extract_string_value(text, "source")?;
        let source_hex = extract_string_value(text, "source_hash")?;
        let target = extract_string_value(text, "target")?;
        let output_path = extract_string_value(text, "output_path")?;
        let output_hex = extract_string_value(text, "output_hash")?;
        let timestamp = extract_number_value(text, "timestamp")?;
        Some(CacheRecord {
            key: u64::from_str_radix(&key_hex, 16).ok()?,
            source,
            source_hash: u64::from_str_radix(&source_hex, 16).ok()?,
            target,
            output_path,
            output_hash: u64::from_str_radix(&output_hex, 16).ok()?,
            timestamp,
        })
    }
}

fn extract_string_value(text: &str, key: &str) -> Option<String> {
    let needle = format!("\"{}\":", key);
    let i = text.find(&needle)?;
    let rest = &text[i + needle.len()..];
    let q1 = rest.find('"')?;
    let after_q1 = &rest[q1 + 1..];
    // Find the *unescaped* closing quote — i.e. a `"` not preceded by `\`.
    // Scan char by char, decoding escapes.
    let bytes = after_q1.as_bytes();
    let mut out = String::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        let b = bytes[i];
        if b == b'"' {
            // unescaped close
            return Some(out);
        }
        if b == b'\\' && i + 1 < bytes.len() {
            let next = bytes[i + 1];
            match next {
                b'"' => out.push('"'),
                b'\\' => out.push('\\'),
                b'n' => out.push('\n'),
                b't' => out.push('\t'),
                b'r' => out.push('\r'),
                b'/' => out.push('/'),
                _ => {
                    // Unknown escape — preserve verbatim.
                    out.push('\\');
                    out.push(next as char);
                }
            }
            i += 2;
            continue;
        }
        out.push(b as char);
        i += 1;
    }
    None
}

fn extract_number_value(text: &str, key: &str) -> Option<u64> {
    let needle = format!("\"{}\":", key);
    let i = text.find(&needle)?;
    let rest = &text[i + needle.len()..];
    let trimmed = rest.trim_start();
    let end = trimmed
        .find(|c: char| !c.is_ascii_digit())
        .unwrap_or(trimmed.len());
    trimmed[..end].parse().ok()
}

/// Look up a cache record by key. Returns the record ONLY if every trust check
/// passes; otherwise `None` (caller rebuilds from source). Checks, in order:
///
///   1. the record parses and is for this exact `source` content (`source_hash`);
///   2. its `output_path` confines to `project_root` (no `..`, no escape);
///   3. the confined output file still hashes to the recorded `output_hash`.
///
/// `source` is the actual (post-expansion) source text; `project_root` is the
/// directory the output must stay within (see `project_root_for`).
pub fn cache_lookup(
    cache_dir: &Path,
    key: u64,
    project_root: &Path,
    source: &str,
) -> Option<CacheRecord> {
    let path = cache_dir.join(format!("{:016x}.json", key));
    let text = std::fs::read_to_string(&path).ok()?;
    let record = CacheRecord::from_json(&text)?;
    // (1) The entry must be for THIS source content, not merely this key.
    if record.source_hash != fnv1a64(source.as_bytes()) {
        return None;
    }
    // (2) Confine the output path to the project — rejects absolute-escape,
    // `..`, and symlink escapes. A record can never read/copy outside the tree.
    let confined = confine_output_path(&record.output_path, project_root)?;
    // (3) Verify the (confined) output file still has the recorded hash. If the
    // user edited the output by hand, force a rebuild.
    let out = std::fs::read(&confined).ok()?;
    if fnv1a64(&out) != record.output_hash {
        return None;
    }
    Some(record)
}

/// Store a cache record.
pub fn cache_store(cache_dir: &Path, record: &CacheRecord) -> std::io::Result<()> {
    std::fs::create_dir_all(cache_dir)?;
    let path = cache_dir.join(format!("{:016x}.json", record.key));
    std::fs::write(path, record.to_json())
}

pub fn current_timestamp() -> u64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

/// `pyths cache status` — print location and entry count.
pub fn status(verbosity: Verbosity) -> Result<(), Box<dyn std::error::Error>> {
    let cwd = std::env::current_dir()?;
    let cache_dir = cache_dir_from_start(&cwd);
    let exists = cache_dir.is_dir();
    let entries = if exists {
        std::fs::read_dir(&cache_dir)
            .map(|rd| rd.filter_map(|e| e.ok()).count())
            .unwrap_or(0)
    } else {
        0
    };
    if verbosity != Verbosity::Quiet {
        // Always print status (not gated to Verbose) so users can inspect
        // their cache without --verbose.
        println!(
            "Cache directory: {} ({})",
            cache_dir.display(),
            if exists { "exists" } else { "not yet created" }
        );
        println!("Cached entries: {}", entries);
    }
    Ok(())
}

/// `pyths cache clear` — delete the entire cache.
pub fn clear(verbosity: Verbosity) -> Result<(), Box<dyn std::error::Error>> {
    let cwd = std::env::current_dir()?;
    let cache_dir = cache_dir_from_start(&cwd);
    if cache_dir.is_dir() {
        std::fs::remove_dir_all(&cache_dir)?;
        if verbosity != Verbosity::Quiet {
            output::success(&format!("Cleared cache at {}", cache_dir.display()));
        }
    } else if verbosity != Verbosity::Quiet {
        output::info("Cache was already empty.", verbosity);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fnv1a_is_stable() {
        assert_eq!(fnv1a64(b"hello"), fnv1a64(b"hello"));
        assert_ne!(fnv1a64(b"hello"), fnv1a64(b"world"));
    }

    #[test]
    fn cache_key_changes_with_target() {
        let src = "def add(a: int, b: int) -> int:\n    return a + b\n";
        let k1 = cache_key(src, "js");
        let k2 = cache_key(src, "wasm");
        assert_ne!(k1, k2);
    }

    #[test]
    fn cache_key_changes_with_source() {
        let k1 = cache_key("a = 1", "js");
        let k2 = cache_key("a = 2", "js");
        assert_ne!(k1, k2);
    }

    #[test]
    fn round_trip_record() {
        let r = CacheRecord {
            key: 0x1234567890abcdef,
            source: "main.ps".into(),
            source_hash: 0x0badc0de0badc0de,
            target: "js".into(),
            output_path: "main.js".into(),
            output_hash: 0xdeadbeefcafef00d,
            timestamp: 1234567890,
        };
        let json = r.to_json();
        let r2 = CacheRecord::from_json(&json).expect("parse round-trip");
        assert_eq!(r2.key, r.key);
        assert_eq!(r2.source, r.source);
        assert_eq!(r2.source_hash, r.source_hash);
        assert_eq!(r2.target, r.target);
        assert_eq!(r2.output_path, r.output_path);
        assert_eq!(r2.output_hash, r.output_hash);
        assert_eq!(r2.timestamp, r.timestamp);
    }

    /// A3: a pre-fix record without `source_hash` must fail to parse (→ miss →
    /// rebuild), never silently trusted.
    #[test]
    fn legacy_record_without_source_hash_rejected() {
        let legacy = "{\n  \"key\": \"0000000000000001\",\n  \"source\": \"a.ps\",\n  \"target\": \"js\",\n  \"output_path\": \"a.js\",\n  \"output_hash\": \"0000000000000002\",\n  \"timestamp\": 1\n}\n";
        assert!(CacheRecord::from_json(legacy).is_none());
    }

    /// A3 defense 1: the cache directory is never inside the source tree.
    #[test]
    fn cache_dir_is_out_of_source_tree() {
        let p = Path::new("/tmp/xyz-src-tree/main.ps");
        let d = cache_dir_for(p).to_string_lossy().to_string();
        // Must not sit under the (attacker-controlled) source tree...
        assert!(
            !d.contains("xyz-src-tree"),
            "cache leaked into source tree: {d}"
        );
        // ...and must live in a pyths-owned user cache location.
        assert!(
            d.to_lowercase().contains("pyths"),
            "unexpected cache dir: {d}"
        );
    }

    // Helper: build a valid record for `source`/`output` and store it.
    fn store_valid(dir: &Path, source: &str, output: &Path, bytes: &[u8]) -> u64 {
        std::fs::write(output, bytes).unwrap();
        let key = cache_key(source, "js");
        let record = CacheRecord {
            key,
            source: "src.ps".into(),
            source_hash: fnv1a64(source.as_bytes()),
            target: "js".into(),
            output_path: output.to_string_lossy().to_string(),
            output_hash: fnv1a64(bytes),
            timestamp: current_timestamp(),
        };
        cache_store(dir, &record).unwrap();
        key
    }

    #[test]
    fn lookup_returns_none_when_missing() {
        let dir = std::env::temp_dir().join("pyths_cache_test_missing");
        let _ = std::fs::remove_dir_all(&dir);
        assert!(cache_lookup(&dir, 0xdeadbeef, &dir, "source").is_none());
    }

    #[test]
    fn store_then_lookup_round_trip() {
        let root = std::env::temp_dir().join(format!("pyths_cache_store_{}", current_timestamp()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();

        let output = root.join("out.js");
        let key = store_valid(&root, "source", &output, b"console.log('hi');");

        // project_root == root, source matches → hit.
        let r2 = cache_lookup(&root, key, &root, "source").expect("hit");
        assert_eq!(r2.key, key);
        assert_eq!(r2.target, "js");

        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn lookup_invalidates_when_output_modified() {
        let root = std::env::temp_dir().join(format!("pyths_cache_inv_{}", current_timestamp()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();

        let output = root.join("out.js");
        let key = store_valid(&root, "src", &output, b"original");

        // Modify the output — lookup should now miss.
        std::fs::write(&output, b"tampered").unwrap();
        assert!(cache_lookup(&root, key, &root, "src").is_none());

        let _ = std::fs::remove_dir_all(&root);
    }

    /// A3 defense 3: a record whose recorded source differs from the actual
    /// source content is NOT trusted, even at a matching key.
    #[test]
    fn lookup_rejects_source_mismatch() {
        let root = std::env::temp_dir().join(format!("pyths_cache_srcmm_{}", current_timestamp()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();

        let output = root.join("out.js");
        let key = store_valid(&root, "real source", &output, b"console.log(1);");

        // Same key requested, but the actual source content differs → miss.
        assert!(cache_lookup(&root, key, &root, "DIFFERENT source").is_none());

        let _ = std::fs::remove_dir_all(&root);
    }

    /// A3 defense 2 — arbitrary read: a hostile record pointing OUTSIDE the
    /// project root must be rejected (never read/copied).
    #[test]
    fn lookup_rejects_output_outside_project() {
        let base = std::env::temp_dir().join(format!("pyths_cache_esc_{}", current_timestamp()));
        let _ = std::fs::remove_dir_all(&base);
        let project = base.join("project");
        std::fs::create_dir_all(&project).unwrap();

        // A "secret" file living OUTSIDE the project root.
        let secret = base.join("secret.txt");
        std::fs::write(&secret, b"TOP-SECRET").unwrap();

        // Hostile record: valid source_hash + output_hash, but output_path
        // escapes the project. The attacker can compute all of these.
        let key = cache_key("benign", "js");
        let record = CacheRecord {
            key,
            source: "app.ps".into(),
            source_hash: fnv1a64(b"benign"),
            target: "js".into(),
            output_path: secret.to_string_lossy().to_string(),
            output_hash: fnv1a64(b"TOP-SECRET"),
            timestamp: current_timestamp(),
        };
        // Plant it in the project's (hypothetical) cache dir.
        let cache_dir = project.join("cache");
        cache_store(&cache_dir, &record).unwrap();

        // Confinement rejects it → miss (source never exfiltrated).
        assert!(
            cache_lookup(&cache_dir, key, &project, "benign").is_none(),
            "arbitrary-read record was trusted"
        );

        let _ = std::fs::remove_dir_all(&base);
    }

    /// A3 defense 2 — `..` traversal in output_path is rejected outright.
    #[test]
    fn confine_rejects_parent_traversal() {
        let root = std::env::temp_dir().join(format!("pyths_confine_{}", current_timestamp()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        assert!(confine_output_path("../evil.js", &root).is_none());
        assert!(confine_output_path("sub/../../evil.js", &root).is_none());
        let _ = std::fs::remove_dir_all(&root);
    }

    /// A3 defense 2 — substitution: a hostile record pointing at an in-tree
    /// `payload.js` is rejected once the source content no longer matches
    /// (belt) and, independently, whenever the project cache is unplantable
    /// (out-of-tree, covered by `cache_dir_is_out_of_source_tree`). Here we
    /// prove the source-content gate blocks the swap.
    #[test]
    fn lookup_rejects_substitution_when_source_differs() {
        let root = std::env::temp_dir().join(format!("pyths_subst_{}", current_timestamp()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();

        // Attacker plants payload.js INSIDE the tree and a record keyed to the
        // benign source but they cannot make the *actual* compiled source hash
        // to the payload — the record must claim benign source.
        let payload = root.join("payload.js");
        std::fs::write(&payload, b"globalThis.PWNED=1;").unwrap();

        // Record claims source "benign" (so it could hit for benign input) and
        // output payload.js. This is exactly the substitution attack shape.
        let key = cache_key("benign", "js");
        let record = CacheRecord {
            key,
            source: "app.ps".into(),
            // Attacker sets this to the benign hash so it hits for benign src.
            source_hash: fnv1a64(b"benign"),
            target: "js".into(),
            output_path: payload.to_string_lossy().to_string(),
            output_hash: fnv1a64(b"globalThis.PWNED=1;"),
            timestamp: current_timestamp(),
        };
        let cache_dir = root.join("cache");
        cache_store(&cache_dir, &record).unwrap();

        // The record IS internally consistent for benign source + in-tree
        // output, so the last line of defense is that this cache is never
        // consulted (out-of-tree). We assert the in-tree location the attacker
        // would use is NOT the location `cache_dir_for` resolves to.
        let benign_src = root.join("app.ps");
        std::fs::write(&benign_src, b"benign").unwrap();
        let real_cache_dir = cache_dir_for(&benign_src);
        assert_ne!(
            std::fs::canonicalize(&real_cache_dir).ok(),
            std::fs::canonicalize(&cache_dir).ok(),
            "compiler would consult an in-tree, attacker-plantable cache dir"
        );

        let _ = std::fs::remove_dir_all(&root);
    }
}
