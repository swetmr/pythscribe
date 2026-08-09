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
            std::fs::write(out, &expanded)?;
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
