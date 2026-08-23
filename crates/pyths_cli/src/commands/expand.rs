//! `pyths expand` — preview the `.psc` → `.ps` expansion without compiling.
//!
//! Reads `file`, applies the same Tier A/B/C/D/Dictionary expander pipeline
//! that `pyths compile` runs for `.psc` inputs, and writes the canonical
//! PythScribe source to `-o <path>` or stdout. Useful for debugging
//! compressed sources and for users who want to commit a canonical `.ps`
//! while authoring in `.psc`.

use std::path::Path;

use super::output::{self, Verbosity};

pub fn run(
    file: &str,
    output_path_arg: Option<&str>,
    verify: bool,
    verbosity: Verbosity,
) -> Result<(), Box<dyn std::error::Error>> {
    let path = Path::new(file);
    if !path.exists() {
        return Err(format!("File not found: {}", file).into());
    }

    let raw_source = std::fs::read_to_string(path)?;
    let config = pyths_config::load_or_default();
    // Thread both the $NAME dictionary and the %NAME idiom map through.
    let expanded = pyths_expand::expand_with_config(
        &raw_source,
        &config.expand.dictionary,
        &config.expand.idioms,
    );

    if verify {
        let ps_path = path.with_extension("ps");
        let ps_src = std::fs::read_to_string(&ps_path)
            .map_err(|_| format!("--verify needs a sibling {}", ps_path.display()))?;
        let lhs = pyths_print::canonicalize(&expanded)
            .map_err(|e| format!("expanded .psc does not parse: {:?}", e))?;
        let rhs = pyths_print::canonicalize(&ps_src)
            .map_err(|e| format!("sibling .ps does not parse: {:?}", e))?;
        if lhs != rhs {
            return Err(format!(
                "Iron Rule violation: canonicalize(expand({})) != canonicalize({})",
                file,
                ps_path.display()
            )
            .into());
        }
        if verbosity != Verbosity::Quiet {
            output::success(&format!(
                "verified: {} round-trips to {}",
                file,
                ps_path.display()
            ));
        }
        if let Some(out) = output_path_arg {
            output::warning_summary(&format!(
                "--verify: -o {} ignored (verify mode writes nothing)",
                out
            ));
        }
        return Ok(());
    }

    match output_path_arg {
        Some(out) => {
            // B7: route through the ONE consolidated safe-write API instead of a
            // bare `std::fs::write`, which FOLLOWS a symlink at `out` and would
            // overwrite the link's target with source-influenced bytes (CWE-59).
            // Expand emits clean canonical `.ps` (Python — `//` is not a legal
            // comment, so no `@generated` marker can be embedded), and `-o` is
            // an explicit destination, so this uses the marker-free
            // `write_user_named`: symlinks / non-regular files are refused, the
            // write is TOCTOU-safe, and the bytes are written verbatim.
            super::safewrite::write_user_named(std::path::Path::new(out), expanded.as_bytes())?;
            if verbosity != Verbosity::Quiet {
                output::success(&format!("Expanded {} → {}", file, out));
            }
        }
        None => {
            // stdout. Use print! (not println!) so the file's trailing
            // newline — or lack thereof — round-trips byte-for-byte.
            print!("{}", expanded);
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch(tag: &str) -> std::path::PathBuf {
        use std::sync::atomic::{AtomicU64, Ordering};
        static N: AtomicU64 = AtomicU64::new(0);
        let id = N.fetch_add(1, Ordering::SeqCst);
        let d = std::env::temp_dir().join(format!(
            "pyths_expand_{}_{}_{}",
            tag,
            std::process::id(),
            id
        ));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    #[cfg(unix)]
    fn try_symlink(target: &Path, link: &Path) -> std::io::Result<()> {
        std::os::unix::fs::symlink(target, link)
    }
    #[cfg(windows)]
    fn try_symlink(target: &Path, link: &Path) -> std::io::Result<()> {
        std::os::windows::fs::symlink_file(target, link)
    }

    /// B7: `pyths expand -o <symlink>` must refuse the symlink and leave the
    /// link's target byte-for-byte intact (CWE-59) — not write through it.
    #[test]
    fn expand_o_refuses_a_symlink_target() {
        let d = scratch("symlink");
        let psc = d.join("a.psc");
        std::fs::write(&psc, "X = 1\n").unwrap();
        let victim = d.join("victim.txt");
        std::fs::write(&victim, b"ORIGINAL-SECRET").unwrap();
        let link = d.join("out.ps");
        if try_symlink(&victim, &link).is_err() {
            eprintln!("skipping: OS denied symlink creation");
            let _ = std::fs::remove_dir_all(&d);
            return;
        }
        let err = run(
            psc.to_str().unwrap(),
            Some(link.to_str().unwrap()),
            false,
            Verbosity::Quiet,
        )
        .unwrap_err();
        assert!(err.to_string().contains("symlink"), "err: {}", err);
        assert_eq!(std::fs::read(&victim).unwrap(), b"ORIGINAL-SECRET");
        let _ = std::fs::remove_dir_all(&d);
    }

    /// The working `-o` path still produces the expanded output byte-for-byte.
    #[test]
    fn expand_o_writes_expansion_to_a_fresh_path() {
        let d = scratch("fresh");
        let psc = d.join("a.psc");
        std::fs::write(&psc, "X = 1\n").unwrap();
        let out = d.join("out.ps");
        run(
            psc.to_str().unwrap(),
            Some(out.to_str().unwrap()),
            false,
            Verbosity::Quiet,
        )
        .unwrap();
        assert!(out.exists());
        assert!(std::fs::read_to_string(&out).unwrap().contains("X = 1"));
        let _ = std::fs::remove_dir_all(&d);
    }
}
