use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

/// A13: resolve `wasm-opt` to an absolute path via a PATH search that excludes
/// the current directory. On Windows `CreateProcess` otherwise searches the cwd
/// first, so a hostile `wasm-opt.exe` planted in a project root could be run.
/// `PYTHS_WASM_OPT` overrides the binary explicitly. Returns `None` when it is
/// not found — callers keep their existing graceful-skip behavior.
fn resolve_wasm_opt() -> Option<PathBuf> {
    // Explicit override wins.
    if let Ok(val) = std::env::var("PYTHS_WASM_OPT") {
        if !val.is_empty() {
            let p = PathBuf::from(&val);
            if p.is_file() {
                return Some(p);
            }
            if let Some(found) = search_path(&val) {
                return Some(found);
            }
            return None;
        }
    }
    search_path("wasm-opt")
}

fn search_path(name: &str) -> Option<PathBuf> {
    let as_path = Path::new(name);
    if as_path.is_absolute() && as_path.is_file() {
        return Some(as_path.to_path_buf());
    }
    let path_var = std::env::var_os("PATH")?;
    let exts = executable_extensions();
    for dir in std::env::split_paths(&path_var) {
        // Skip empty / `.` entries — never search the current directory.
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

pub struct OptimizeResult {
    pub size_before: usize,
    pub size_after: usize,
    /// Optimization level applied (`"Os"`, `"O2"`, `"O3"`, etc.).
    pub level: &'static str,
}

/// `wasm-opt` optimization profile. PythScribe ships for the web, so
/// `Size` (the `-Os` flag) is the production default. `Speed` (`-O2`)
/// can be useful for compute-heavy WASM workloads. `Aggressive` (`-O3`)
/// is rarely the right tradeoff — it's slower to optimize and the
/// extra reduction usually doesn't beat `-Os` for bundle bytes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OptLevel {
    Size,
    Speed,
    Aggressive,
}

impl OptLevel {
    fn flag(&self) -> &'static str {
        match self {
            OptLevel::Size => "-Os",
            OptLevel::Speed => "-O2",
            OptLevel::Aggressive => "-O3",
        }
    }

    fn label(&self) -> &'static str {
        match self {
            OptLevel::Size => "Os",
            OptLevel::Speed => "O2",
            OptLevel::Aggressive => "O3",
        }
    }
}

/// Backward-compatible entry: runs `wasm-opt -Os` (size-optimized
/// production default) on the given `.wasm` in place.
///
/// Returns `Ok(None)` if `wasm-opt` is not found on PATH — the rest
/// of the build proceeds with the unoptimized module.
/// Returns `Ok(Some(result))` with size before/after on success.
pub fn run_wasm_opt(wasm_path: &str) -> Result<Option<OptimizeResult>, String> {
    run_wasm_opt_with_level(wasm_path, OptLevel::Size)
}

/// Run `wasm-opt` at the requested level on a `.wasm` file in place.
pub fn run_wasm_opt_with_level(
    wasm_path: &str,
    level: OptLevel,
) -> Result<Option<OptimizeResult>, String> {
    // A13: resolve to an absolute path off PATH (never cwd) before spawning.
    // Not found => graceful skip (unchanged behavior), so an absent wasm-opt is
    // never an error.
    let wasm_opt = match resolve_wasm_opt() {
        Some(p) => p,
        None => return Ok(None),
    };

    let check = Command::new(&wasm_opt).arg("--version").output();
    if check.is_err() {
        return Ok(None);
    }

    let size_before = fs::metadata(wasm_path)
        .map_err(|e| format!("Cannot read {}: {}", wasm_path, e))?
        .len() as usize;

    let opt_path = format!("{}.opt", wasm_path);

    let result = Command::new(&wasm_opt)
        .args([level.flag(), wasm_path, "-o", &opt_path])
        .output()
        .map_err(|e| format!("Failed to run wasm-opt: {}", e))?;

    if !result.status.success() {
        let _ = fs::remove_file(&opt_path);
        let stderr = String::from_utf8_lossy(&result.stderr);
        return Err(format!("wasm-opt failed: {}", stderr));
    }

    fs::rename(&opt_path, wasm_path)
        .map_err(|e| format!("Failed to replace with optimized file: {}", e))?;

    let size_after = fs::metadata(wasm_path)
        .map_err(|e| format!("Cannot read optimized {}: {}", wasm_path, e))?
        .len() as usize;

    Ok(Some(OptimizeResult {
        size_before,
        size_after,
        level: level.label(),
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn opt_level_flags_match_wasm_opt_documented_options() {
        assert_eq!(OptLevel::Size.flag(), "-Os");
        assert_eq!(OptLevel::Speed.flag(), "-O2");
        assert_eq!(OptLevel::Aggressive.flag(), "-O3");
    }

    #[test]
    fn opt_level_labels_are_short_and_distinctive() {
        // Labels surface in CLI verbose output; must be short and
        // unambiguous.
        assert_eq!(OptLevel::Size.label(), "Os");
        assert_eq!(OptLevel::Speed.label(), "O2");
        assert_eq!(OptLevel::Aggressive.label(), "O3");
    }
}
