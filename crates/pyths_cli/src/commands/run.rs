use std::path::Path;

use super::output::Verbosity;

pub fn run(
    file: &str,
    explain: bool,
    verbosity: Verbosity,
) -> Result<(), Box<dyn std::error::Error>> {
    let path = Path::new(file);
    if !path.exists() {
        return Err(format!("File not found: {}", file).into());
    }

    let source = std::fs::read_to_string(path)?;
    let filename = path.file_name().unwrap().to_string_lossy();

    // Parse
    let mut module = pyths_parser::parse(&source).map_err(|errors| {
        let diags: Vec<_> = errors
            .iter()
            .map(|e| {
                let mut diag = pyths_diagnostic::errors::Diagnostic::error(
                    &e.message,
                    pyths_syntax::span::Span::new(e.span.start, e.span.end),
                );
                for note in &e.notes {
                    diag = diag.with_note(note);
                }
                diag
            })
            .collect();

        pyths_diagnostic::render::render_diagnostics(&source, &filename, &diags)
    })?;
    // Expand relative star imports (commands::relstar) before codegen.
    super::relstar::normalize_relative_imports(&mut module, path)?;
    let module = module;

    // Codegen — inline runtime so output is self-contained for Node.js
    //
    // A9: write into a fresh, private, per-invocation temp DIRECTORY rather
    // than a predictable `pyths_<stem>.mjs` in the shared OS temp dir. A fixed
    // name is a TOCTOU / symlink target (a local attacker pre-creates it to
    // force an arbitrary overwrite or to substitute the code node executes).
    // Round-5: hold the TempDir HANDLE — cleanup is RAII-bound, so every exit
    // path below (including a failed `node` spawn `?`-return) removes the dir.
    let temp_dir = super::procutil::make_private_temp_dir("run")?;
    let stem = path.file_stem().unwrap().to_string_lossy();
    let temp_file = temp_dir.path().join(format!("pyths_{}.mjs", stem));
    let temp_map = temp_dir.path().join(format!("pyths_{}.mjs.map", stem));

    let js = if explain {
        // Emit source map so post-mortem can resolve stack frames back
        // to `.ps` source.
        let mjs_filename = format!("pyths_{}.mjs", stem);
        let map_filename = format!("pyths_{}.mjs.map", stem);
        let output = pyths_codegen_js::codegen_inline_with_sourcemap(
            &module,
            &source,
            &filename,
            &mjs_filename,
        );
        if let Some(map_json) = &output.source_map {
            std::fs::write(&temp_map, map_json)?;
        }
        // Inline-runtime `finish()` writes a `//# sourceMappingURL=` line
        // only when the user-facing CLI compile path sets a `.map`
        // sibling. We do the same here so Node `--enable-source-maps`
        // picks it up.
        format!("{}//# sourceMappingURL={}\n", output.js, map_filename)
    } else {
        pyths_codegen_js::codegen_inline(&module)
    };
    std::fs::write(&temp_file, &js)?;

    // A0: the compiled output can `import ... from "pyths-runtime/stdlib/X"`
    // (or "/web/X", "/core", etc.) even though helpers are otherwise
    // inlined. Node resolves that specifier by walking `node_modules`
    // upward from the temp file's directory, so materialize a real
    // `pyths-runtime` package there — mirroring what `npm install
    // pyths-runtime` would produce — before spawning node.
    pyths_runtime::materialize_runtime_package(temp_dir.path())?;

    super::output::info(&format!("Running {} via Node.js...", file), verbosity);

    // A13: resolve `node` to an absolute path via a PATH search that excludes
    // the current directory, so a hostile `node.exe` planted in the project
    // root is never preferred (Windows `CreateProcess` otherwise searches cwd).
    // `PYTHS_NODE` may override the interpreter explicitly.
    let node = super::procutil::resolve_program("node", Some("PYTHS_NODE")).ok_or(
        "could not find `node` on PATH — install Node.js or set PYTHS_NODE to its path",
    )?; // TempDir RAII cleans up on this early return

    let status = if explain {
        // Capture stderr so we can prepend a Python-flavored explanation
        // when the program crashes. stdout passes through unchanged.
        let output = std::process::Command::new(&node)
            .arg("--enable-source-maps")
            .arg(&temp_file)
            .output()?;
        // Forward stdout as-is.
        if !output.stdout.is_empty() {
            use std::io::Write;
            std::io::stdout().write_all(&output.stdout).ok();
        }
        let stderr = String::from_utf8_lossy(&output.stderr);
        if !stderr.is_empty() {
            if let Some(explanation) = explain_node_trace(&stderr, &filename) {
                eprintln!("{}", explanation);
            }
            eprint!("{}", stderr);
        }
        output.status
    } else {
        std::process::Command::new(&node).arg(&temp_file).status()?
    };

    // Clean up the whole private temp directory (mjs, map, materialized
    // runtime package, and anything else that landed inside). RAII: the drop
    // also runs on every earlier `?`-return, incl. a failed `node` spawn.
    drop(temp_dir);

    if !status.success() {
        return Err("Node.js execution failed".into());
    }

    Ok(())
}

/// Inspect a captured Node stderr string. If it looks like an
/// `Error [Name]: message` crash, return a short Python-flavored
/// explanation paragraph; otherwise return None.
///
/// Maps the common error classes the new runtime helpers throw —
/// IndexError, KeyError, ZeroDivisionError, AttributeError,
/// AssertionError, TypeError — to a sentence that reads as Python
/// idiom, then points at the `.ps` line if one can be found in the
/// stack trace.
fn explain_node_trace(stderr: &str, source_filename: &str) -> Option<String> {
    // Look for `Error [ClassName]: message` (Node format when err.name
    // is set on a thrown Error). Hand-parse to avoid pulling regex in.
    let (class, msg) = parse_error_header(stderr)?;

    // Find the first stack frame mentioning the .ps source file — that's
    // the user's code, as opposed to runtime-helper frames.
    let ps_loc = stderr
        .lines()
        .find_map(|line| {
            let trimmed = line.trim_start();
            if !trimmed.starts_with("at ") {
                return None;
            }
            let needle = source_filename.trim_end_matches(".ps");
            if line.contains(&format!("{}.ps:", needle)) || line.contains(source_filename) {
                Some(line.trim().to_string())
            } else {
                None
            }
        })
        .unwrap_or_else(|| "(no .ps frame found in trace)".to_string());

    let hint = match class {
        "IndexError" => format!(
            "IndexError — your code tried to read past the end of a sequence ({}). \n\
             In Python this raises IndexError; PythScribe follows the same rule rather than \n\
             silently returning undefined as raw JS would.",
            msg
        ),
        "KeyError" => format!(
            "KeyError — your code tried to read a missing dict key ({}). \n\
             In Python this raises KeyError; PythScribe follows the same rule rather than \n\
             silently returning undefined as raw JS would.",
            msg
        ),
        "ZeroDivisionError" => format!(
            "ZeroDivisionError — your code divided by zero ({}). \n\
             In Python this raises ZeroDivisionError; raw JS would have produced Infinity or NaN.",
            msg
        ),
        "AttributeError" => format!(
            "AttributeError — your code accessed an attribute on a value that doesn't have it ({}). \n\
             In Python this raises AttributeError.",
            msg
        ),
        "AssertionError" => format!(
            "AssertionError — an `assert` statement failed: {}. \n\
             Check the asserted condition.",
            msg
        ),
        "TypeError" => format!(
            "TypeError — a value had the wrong type for the operation ({}). \n\
             In Python this raises TypeError.",
            msg
        ),
        "ValueError" => format!(
            "ValueError — a value was the right type but the wrong content ({}).",
            msg
        ),
        _ => format!(
            "{} — {}.",
            class, msg
        ),
    };

    Some(format!(
        "\n─── PythScribe runtime error ──────────────────────────────────\n\
         {}\n\n\
         Source location: {}\n\
         ────────────────────────────────────────────────────────────────\n",
        hint, ps_loc
    ))
}

/// Extract `(class_name, message)` from the first `Error [ClassName]: msg`
/// line in a Node stderr blob. Returns None if the input doesn't match.
fn parse_error_header(stderr: &str) -> Option<(&str, &str)> {
    for line in stderr.lines() {
        let trimmed = line.trim_start();
        // Two shapes we care about:
        //   "Error [IndexError]: list index out of range"
        //   (Node prints this when `err.name = "IndexError"` is set)
        //   "IndexError: list index out of range"
        //   (when the class is a proper subclass of Error)
        if let Some(rest) = trimmed.strip_prefix("Error [") {
            if let Some(end) = rest.find("]: ") {
                let class = &rest[..end];
                let msg = &rest[end + 3..];
                return Some((class, msg));
            }
        }
        // Fallback: ClassName: message (any *Error subclass)
        if let Some(colon) = trimmed.find(": ") {
            let class = &trimmed[..colon];
            if class.ends_with("Error") || class == "AssertionError" {
                let msg = &trimmed[colon + 2..];
                return Some((class, msg));
            }
        }
    }
    None
}
