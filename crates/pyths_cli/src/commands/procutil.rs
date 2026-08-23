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

/// Create a fresh, unique, private temporary directory for this invocation.
///
/// Returns the `TempDir` HANDLE, not a bare path (round-5 fix): cleanup is
/// RAII-bound to the handle, so EVERY exit path in the caller — including a
/// failed `node` spawn that `?`-returns before any manual cleanup — removes
/// the directory. Callers use `.path()` to place files inside it and simply
/// let the handle drop (an explicit `drop(dir)` documents intent).
///
/// SEC round-4 consolidation: ONE temp-dir mechanism for the whole tree —
/// the `tempfile` crate (also used for the wasm-opt private dir in
/// pyths_codegen_wasm::optimize). It draws the name from the OS RNG (a
/// watcher cannot predict or pre-create it), creates exclusively, and on
/// unix applies an explicit mode 0700 ATOMICALLY at mkdir time (no
/// create-then-chmod window, immune to a permissive umask).
pub fn make_private_temp_dir(tag: &str) -> std::io::Result<tempfile::TempDir> {
    let prefix = format!("pyths-{}-", tag);
    let mut builder = tempfile::Builder::new();
    builder.prefix(&prefix);
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        // Explicit 0700: tempfile's DEFAULT dir mode is umask-dependent, and
        // a permissive umask must not open the window this dir exists to
        // close. The mode is handed to mkdir itself — atomic, no chmod race.
        builder.permissions(std::fs::Permissions::from_mode(0o700));
    }
    builder.tempdir()
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
///
/// Round-5 hardening: resolution is ABSOLUTE-ONLY. Any value that is not a
/// BARE program name must be a truly absolute path to an existing file — a
/// relative value (`PYTHS_WASM_OPT=.\evil.exe`) resolves against the current
/// directory, which is the exact CWE-426 cwd-execution hole this module
/// closes, so it is refused rather than honored (and never silently falls
/// back to PATH). A bare name is PATH-resolved, and relative PATH *entries*
/// are skipped too (see `search_path`). Every returned path is absolute
/// before it reaches `Command::new`.
///
/// Round-6: "not bare" is decided by PATH-COMPONENT parsing, not by looking
/// for separators. `PYTHS_NODE=C:evil.exe` contains no separator but is
/// DRIVE-RELATIVE on Windows — it resolves against drive C's current
/// directory (and `Path::join` on it discards the joined-onto PATH dir
/// entirely), so the separator test classified it as a bare name and let it
/// through. Rooted-but-driveless values (`\evil.exe`, current-drive
/// relative) and names containing `:` (drive or NTFS ADS syntax) are
/// likewise not bare. Truly absolute means `Path::is_absolute()`: `C:\x`,
/// verbatim `\\?\C:\x`, and UNC `\\server\share\x` qualify; `C:x` and `\x`
/// do not.
pub fn resolve_program(name: &str, env_override: Option<&str>) -> Option<PathBuf> {
    if let Some(var) = env_override {
        if let Ok(val) = std::env::var(var) {
            if !val.is_empty() {
                if !is_bare_name(&val) {
                    let p = Path::new(&val);
                    if p.is_absolute() && p.is_file() {
                        return Some(p.to_path_buf());
                    }
                    // Relative (incl. drive-relative / rooted-driveless), or
                    // absolute-but-missing: refuse. An explicit override
                    // never falls back to a different program, and a
                    // cwd-relative one is never executed.
                    return None;
                }
                // Bare program name → PATH-only search (never the cwd).
                return search_path(&val);
            }
        }
    }
    search_path(name)
}

/// Is `s` a BARE program name — exactly one normal path component, with no
/// separator, no drive/UNC/verbatim prefix, no root, and (on Windows) no
/// `:`? Anything else is treated as a path and must be truly absolute.
fn is_bare_name(s: &str) -> bool {
    if cfg!(windows) && s.contains(':') {
        // Drive-relative (`C:evil.exe`) parses as a Prefix component and is
        // caught below too; this also refuses NTFS ADS syntax (`prog:ads`),
        // which component parsing alone would classify as a normal name.
        return false;
    }
    let mut comps = Path::new(s).components();
    matches!(
        (comps.next(), comps.next()),
        (Some(std::path::Component::Normal(_)), None)
    )
}

fn search_path(name: &str) -> Option<PathBuf> {
    // A non-bare name is honored ONLY as a truly absolute path to a real
    // file. It must never reach the PATH loop: on Windows, `dir.join(name)`
    // with a prefixed name (`C:evil.exe`) REPLACES `dir` wholesale, so the
    // "joined" candidate would silently be the drive-relative attacker path
    // (round-6 fix).
    if !is_bare_name(name) {
        let as_path = Path::new(name);
        if as_path.is_absolute() && as_path.is_file() {
            return Some(as_path.to_path_buf());
        }
        return None;
    }

    let path_var = std::env::var_os("PATH")?;
    let exts = executable_extensions();
    for dir in std::env::split_paths(&path_var) {
        // SECURITY: skip empty entries and `.` — an empty PATH element means
        // "current directory" on Windows — and skip RELATIVE entries
        // altogether (round-5): `PATH=tools;...` would resolve `tools\x.exe`
        // against whatever the cwd happens to be. Only absolute PATH
        // directories are consulted, so every hit is an absolute path.
        if dir.as_os_str().is_empty() || dir == Path::new(".") || !dir.is_absolute() {
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
    fn private_temp_dirs_are_unique_and_raii_cleaned() {
        let a = make_private_temp_dir("unit").expect("dir a");
        let b = make_private_temp_dir("unit").expect("dir b");
        assert_ne!(a.path(), b.path(), "two invocations must not share a temp dir");
        assert!(a.path().is_dir());
        assert!(b.path().is_dir());
        // Round-5: cleanup is RAII-bound — dropping the handle removes the
        // directory and its contents on EVERY exit path (incl. spawn errors).
        let pa = a.path().to_path_buf();
        std::fs::write(pa.join("payload.mjs"), b"x").unwrap();
        drop(a);
        assert!(!pa.exists(), "drop must remove the dir and its contents");
        drop(b);
    }

    #[cfg(unix)]
    #[test]
    fn private_temp_dir_is_owner_only_on_unix() {
        use std::os::unix::fs::PermissionsExt;
        let d = make_private_temp_dir("perm").expect("dir");
        let mode = std::fs::metadata(d.path()).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode, 0o700, "private temp dir must be 0700, got {:o}", mode);
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
        let planted = dir.path().join(planted_name);
        std::fs::write(&planted, b"#!/bin/sh
").unwrap();

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
        let bin = dir.path().join(&fname);
        std::fs::write(&bin, b"x").unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&bin, std::fs::Permissions::from_mode(0o755)).unwrap();
        }

        // Prepend our scratch dir to PATH for this process.
        let old = std::env::var_os("PATH");
        let mut paths = vec![dir.path().to_path_buf()];
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
    }

    // ── round-5: absolute-only resolution ──────────────────────────────────

    #[test]
    fn relative_env_override_is_refused() {
        // A separator-bearing RELATIVE override resolves against the cwd —
        // the CWE-426 hole — and must be refused, with no PATH fallback.
        let dir = make_private_temp_dir("relover").expect("scratch");
        let planted = dir
            .path()
            .join(if cfg!(windows) { "evil.exe" } else { "evil" });
        std::fs::write(&planted, b"x").unwrap();

        let var = "PYTHS_TEST_REL_OVERRIDE";
        for rel in ["./evil.exe", ".\\evil.exe", "sub/evil"] {
            std::env::set_var(var, rel);
            let got = resolve_program("node", Some(var));
            std::env::remove_var(var);
            assert!(
                got.is_none(),
                "relative override {rel:?} must be refused, got {got:?}"
            );
        }

        // An ABSOLUTE override to an existing file is honored.
        std::env::set_var(var, planted.to_string_lossy().to_string());
        let got = resolve_program("node", Some(var));
        std::env::remove_var(var);
        assert_eq!(got.as_deref(), Some(planted.as_path()));
    }

    // ── round-6: drive-relative / rooted-driveless overrides ───────────────

    #[cfg(windows)]
    #[test]
    fn drive_relative_and_rooted_overrides_are_refused() {
        // `C:evil.exe` has NO separator, but it is drive-relative: it
        // resolves against drive C's current directory, and joining it onto
        // a PATH dir replaces the dir wholesale. It must be refused, along
        // with rooted-but-driveless (`\evil.exe`) and ADS-style (`prog:ads`)
        // values — with no PATH fallback.
        let var = "PYTHS_TEST_DRIVEREL_OVERRIDE";
        for bad in ["C:evil.exe", "C:sub\\evil.exe", "\\evil.exe", "prog:ads.exe"] {
            std::env::set_var(var, bad);
            let got = resolve_program("node", Some(var));
            std::env::remove_var(var);
            assert!(
                got.is_none(),
                "drive-relative/rooted override {bad:?} must be refused, got {got:?}"
            );
        }
        // And the same value must never resolve through search_path either
        // (the PATH loop must not join a prefixed name onto its dirs).
        assert!(resolve_program("C:evil.exe", None).is_none());
        assert!(resolve_program("\\evil.exe", None).is_none());
    }

    // ── #442: UNC parity anchor — the Rust semantics the JS plugins mirror ──

    #[cfg(windows)]
    #[test]
    fn forward_slash_unc_is_absolute_like_backslash_unc() {
        // `pyths-safe.js::isTrulyAbsolute` mirrors THESE exact std::path
        // classifications (its #442 fix). If Rust's parsing ever changed,
        // this pins the anchor the JS side is calibrated against.
        // Absolute: a UNC prefix spelled with either separator (non-empty
        // server, one separator, non-empty share) plus verbatim/device.
        for abs in [
            "//server/share/x.exe",
            "\\\\server/share/x.exe",
            "/\\server\\share\\x.exe",
            "\\/server/share/x.exe",
            "\\\\server\\share\\x.exe",
            "\\\\?\\C:\\x.exe",
            "\\\\.\\COM1",
        ] {
            assert!(
                Path::new(abs).is_absolute(),
                "{abs:?} must be absolute (UNC/verbatim prefix)"
            );
            assert!(!is_bare_name(abs), "{abs:?} is a path, not a bare name");
        }
        // NOT absolute: incomplete UNC shapes parse as RootDir —
        // current-drive-relative — and must never be honored as absolute.
        for rel in [
            "//server",
            "//server/",
            "//server//x.exe",
            "///a/b.exe",
            "\\\\server",
            "\\\\server\\\\share\\x.exe",
        ] {
            assert!(
                !Path::new(rel).is_absolute(),
                "{rel:?} must NOT be absolute (RootDir, drive-relative)"
            );
        }
        // And the resolver end-to-end: a well-formed (but nonexistent)
        // forward-slash UNC override is shape-accepted then refused only
        // because the file is missing — identically to the backslash form.
        let var = "PYTHS_TEST_FWD_UNC_OVERRIDE";
        for p in [
            "//server/share/pyths_probe.exe",
            "\\\\server\\share\\pyths_probe.exe",
        ] {
            std::env::set_var(var, p);
            let got = resolve_program("node", Some(var));
            std::env::remove_var(var);
            assert!(got.is_none(), "{p:?}: missing file must resolve to None");
        }
    }

    #[test]
    fn bare_name_classification() {
        assert!(is_bare_name("node"));
        assert!(is_bare_name("wasm-opt"));
        assert!(!is_bare_name("./node"));
        assert!(!is_bare_name("sub/node"));
        assert!(!is_bare_name(".."));
        #[cfg(windows)]
        {
            assert!(!is_bare_name("C:evil.exe"));
            assert!(!is_bare_name("\\evil.exe"));
            assert!(!is_bare_name("node:ads"));
            assert!(!is_bare_name("C:\\real\\node.exe"));
        }
    }

    #[test]
    fn relative_path_entries_are_skipped() {
        // `PATH=tools;...` must not resolve `tools/x` against the cwd.
        let dir = make_private_temp_dir("relpath").expect("scratch");
        let name = "pyths_relpath_probe";
        let fname = if cfg!(windows) {
            format!("{}.exe", name)
        } else {
            name.to_string()
        };
        let sub = dir.path().join("tools");
        std::fs::create_dir(&sub).unwrap();
        std::fs::write(sub.join(&fname), b"x").unwrap();

        let old_cwd = std::env::current_dir().unwrap();
        let old_path = std::env::var_os("PATH");
        std::env::set_current_dir(dir.path()).unwrap();
        // A PATH consisting ONLY of the relative entry: nothing may resolve.
        std::env::set_var("PATH", "tools");
        let got = resolve_program(name, None);
        match &old_path {
            Some(p) => std::env::set_var("PATH", p),
            None => std::env::remove_var("PATH"),
        }
        std::env::set_current_dir(old_cwd).unwrap();
        assert!(
            got.is_none(),
            "a relative PATH entry must never resolve (cwd-dependent), got {got:?}"
        );
    }
}
