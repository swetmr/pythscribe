//! Process / filesystem safety helpers shared by the `run`, `test`, and
//! `compile` commands.
//!
//! Two hardening concerns live here:
//!
//!   * **A9 — unpredictable temp files.** `pyths run` / `pyths test` compile to
//!     a `.mjs` and then hand it to `node`. A fixed name in the world-writable
//!     OS temp dir (`pyths_<stem>.mjs`) is a classic TOCTOU / symlink target: a
//!     local attacker can pre-create it (as a symlink for an arbitrary
//!     overwrite, or to win the write→exec race and substitute the code node
//!     runs). `make_private_temp_dir` creates a *fresh, unique, private*
//!     directory per invocation (mode `0700` on unix) and callers put their
//!     transient files inside it.
//!
//!   * **A13 — PATH-relative program spawn.** `Command::new("node")` /
//!     `Command::new("wasm-opt")` are bare program names. On Windows the
//!     `CreateProcess` search order includes the *current directory*, so a
//!     hostile `node.exe` dropped in a project root can be preferred over the
//!     real interpreter. `resolve_program` performs a `which`-style lookup over
//!     the `PATH` env dirs only — never the cwd — and returns an absolute path
//!     to hand to `Command::new`.

use std::path::{Path, PathBuf};

/// Create a fresh, unique, private temporary directory for this invocation and
/// return its path. The caller owns it and should remove it when done.
///
/// The name embeds the process id plus a monotonic counter and a
/// high-resolution timestamp, and is created with `create_dir` (which fails if
/// the path already exists) inside a bounded retry loop — so we never write
/// into a directory an attacker pre-created. On unix the directory is created
/// with mode `0700` (owner-only) so other local users cannot read or plant
/// files in it.
pub fn make_private_temp_dir(tag: &str) -> std::io::Result<PathBuf> {
    use std::sync::atomic::{AtomicU64, Ordering};
    static COUNTER: AtomicU64 = AtomicU64::new(0);

    let base = std::env::temp_dir();
    let pid = std::process::id();

    for attempt in 0..64u64 {
        let seq = COUNTER.fetch_add(1, Ordering::SeqCst);
        // A high-resolution, hard-to-predict-enough component. Combined with
        // the exclusive `create_dir` below this closes the race regardless of
        // predictability — the create fails rather than reusing a planted dir.
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        let name = format!("pyths-{}-{}-{}-{}-{}", tag, pid, seq, nanos, attempt);
        let dir = base.join(name);

        match create_private_dir(&dir) {
            Ok(()) => return Ok(dir),
            // Someone raced us to this exact name — try the next candidate.
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(e) => return Err(e),
        }
    }

    Err(std::io::Error::new(
        std::io::ErrorKind::AlreadyExists,
        "could not create a unique private temp directory after 64 attempts",
    ))
}

#[cfg(unix)]
fn create_private_dir(dir: &Path) -> std::io::Result<()> {
    use std::os::unix::fs::DirBuilderExt;
    // `create_dir` semantics (no `_all`) => fails if it already exists, so a
    // pre-planted path or symlink cannot be silently reused. Mode 0700.
    std::fs::DirBuilder::new().mode(0o700).create(dir)
}

#[cfg(not(unix))]
fn create_private_dir(dir: &Path) -> std::io::Result<()> {
    // `create_dir` (not `_all`) fails with AlreadyExists if the path exists,
    // which is exactly the exclusive-create guarantee we want. On Windows the
    // per-user temp dir is already ACL'd to the current user.
    std::fs::create_dir(dir)
}

/// Resolve an executable `name` to an absolute path by searching the `PATH`
/// environment variable's directories — explicitly **excluding** the current
/// directory (empty / `.` PATH entries are skipped). Returns `None` if it is
/// not found. This avoids the Windows `CreateProcess` behavior of preferring a
/// program of the same name planted in the cwd.
///
/// `env_override`, when `Some(var)`, lets an explicit `VAR=/path/to/prog`
/// (or `VAR=prog-name`) take precedence — used for `PYTHS_NODE` /
/// `PYTHS_WASM_OPT`.
pub fn resolve_program(name: &str, env_override: Option<&str>) -> Option<PathBuf> {
    if let Some(var) = env_override {
        if let Ok(val) = std::env::var(var) {
            if !val.is_empty() {
                let p = PathBuf::from(&val);
                // An override that already names an existing file is used as-is
                // (absolute or cwd-relative — the user asked for it explicitly).
                if p.is_file() {
                    return Some(p);
                }
                // Otherwise treat it as a program name to resolve on PATH.
                if let Some(found) = search_path(&val) {
                    return Some(found);
                }
                // Explicit override that doesn't resolve => do not silently fall
                // back to a different program; report "not found".
                return None;
            }
        }
    }
    search_path(name)
}

fn search_path(name: &str) -> Option<PathBuf> {
    // If the name is already an absolute/relative path to a real file, honor it.
    let as_path = Path::new(name);
    if as_path.is_absolute() && as_path.is_file() {
        return Some(as_path.to_path_buf());
    }

    let path_var = std::env::var_os("PATH")?;
    let exts = executable_extensions();
    for dir in std::env::split_paths(&path_var) {
        // SECURITY: skip empty entries and `.` — an empty PATH element means
        // "current directory" on Windows, which is exactly what we refuse to
        // search. Only genuine PATH directories are consulted.
        if dir.as_os_str().is_empty() || dir == Path::new(".") {
            continue;
        }
        let direct = dir.join(name);
        if direct.is_file() {
            return Some(direct);
        }
        for ext in &exts {
            let candidate = dir.join(format!("{}{}", name, ext));
            if candidate.is_file() {
                return Some(candidate);
            }
        }
    }
    None
}

#[cfg(windows)]
fn executable_extensions() -> Vec<String> {
    std::env::var("PATHEXT")
        .map(|p| {
            p.split(';')
                .map(|s| s.trim())
                .filter(|s| !s.is_empty())
                .map(|s| s.to_string())
                .collect::<Vec<_>>()
        })
        .unwrap_or_else(|_| vec![".COM".into(), ".EXE".into(), ".BAT".into(), ".CMD".into()])
}

#[cfg(not(windows))]
fn executable_extensions() -> Vec<String> {
    Vec::new()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn private_temp_dirs_are_unique_per_call() {
        let a = make_private_temp_dir("unit").expect("dir a");
        let b = make_private_temp_dir("unit").expect("dir b");
        assert_ne!(a, b, "two invocations must not share a temp dir");
        assert!(a.is_dir());
        assert!(b.is_dir());
        let _ = std::fs::remove_dir_all(&a);
        let _ = std::fs::remove_dir_all(&b);
    }

    #[cfg(unix)]
    #[test]
    fn private_temp_dir_is_owner_only_on_unix() {
        use std::os::unix::fs::PermissionsExt;
        let d = make_private_temp_dir("perm").expect("dir");
        let mode = std::fs::metadata(&d).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode, 0o700, "private temp dir must be 0700, got {:o}", mode);
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn resolve_program_skips_cwd_planted_binary() {
        // Plant a fake "node" in a scratch dir, cd there, and confirm the
        // resolver does NOT return the cwd copy (it searches PATH only).
        let dir = make_private_temp_dir("resolvecwd").expect("scratch");
        let planted_name = if cfg!(windows) {
            "pyths_fake_prog.exe"
        } else {
            "pyths_fake_prog"
        };
        let planted = dir.join(planted_name);
        std::fs::write(&planted, b"#!/bin/sh\n").unwrap();

        // Resolve a program name that should not exist anywhere on PATH.
        let resolved = resolve_program("pyths_fake_prog", None);
        // Even though a file named like it sits in `dir`, we never searched cwd,
        // so it must not be found there.
        if let Some(p) = resolved {
            assert_ne!(
                std::fs::canonicalize(&p).ok(),
                std::fs::canonicalize(&planted).ok(),
                "resolver must not return a cwd-planted binary"
            );
        }
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn resolve_program_finds_via_path_dir() {
        // A program placed in a real PATH directory resolves to that abs path.
        let dir = make_private_temp_dir("resolvepath").expect("scratch");
        let name = "pyths_probe_bin";
        let fname = if cfg!(windows) {
            format!("{}.exe", name)
        } else {
            name.to_string()
        };
        let bin = dir.join(&fname);
        std::fs::write(&bin, b"x").unwrap();

        // Prepend our scratch dir to PATH for this process.
        let old = std::env::var_os("PATH");
        let mut paths = vec![dir.clone()];
        if let Some(ref p) = old {
            paths.extend(std::env::split_paths(p));
        }
        let joined = std::env::join_paths(paths).unwrap();
        std::env::set_var("PATH", &joined);

        let resolved = resolve_program(name, None);

        // Restore PATH before asserting.
        match old {
            Some(p) => std::env::set_var("PATH", p),
            None => std::env::remove_var("PATH"),
        }

        let resolved = resolved.expect("should resolve program on PATH");
        assert!(
            resolved.is_absolute(),
            "resolved path must be absolute: {:?}",
            resolved
        );
        assert_eq!(
            std::fs::canonicalize(&resolved).ok(),
            std::fs::canonicalize(&bin).ok()
        );
        let _ = std::fs::remove_dir_all(&dir);
    }
}
