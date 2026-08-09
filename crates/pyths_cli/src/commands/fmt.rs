use std::path::{Path, PathBuf};

use super::output::{self, Verbosity};

pub fn run(
    path: Option<&str>,
    check: bool,
    indent_size: usize,
    verbosity: Verbosity,
) -> Result<(), Box<dyn std::error::Error>> {
    let target = path.map(Path::new).unwrap_or_else(|| Path::new("."));

    let files = if target.is_file() {
        vec![target.to_path_buf()]
    } else {
        discover_ps_files(target)?
    };

    if files.is_empty() {
        if verbosity != Verbosity::Quiet {
            println!("No .ps files found");
        }
        return Ok(());
    }

    let mut changed_count = 0;
    let mut unchanged_count = 0;
    let mut refused_count = 0;

    for file in &files {
        let source = std::fs::read_to_string(file)?;
        let formatted = format_source(&source, indent_size);

        if source == formatted {
            unchanged_count += 1;
        } else if let Err(reason) = reparse_guard(&source, &formatted) {
            // Absolute backstop against a formatter bug silently corrupting a
            // file: the original parsed, but the formatted output does not.
            // Refuse to write; leave the file byte-for-byte untouched.
            refused_count += 1;
            if verbosity != Verbosity::Quiet {
                output::warning_summary(&format!(
                    "Refused to reformat (would corrupt file): {} — {}",
                    file.display(),
                    reason
                ));
            }
        } else {
            changed_count += 1;
            if check {
                if verbosity != Verbosity::Quiet {
                    output::warning_summary(&format!("Would reformat: {}", file.display()));
                }
            } else {
                std::fs::write(file, &formatted)?;
                if verbosity != Verbosity::Quiet {
                    output::success(&format!("Reformatted: {}", file.display()));
                }
            }
        }
    }

    if refused_count > 0 {
        // A refusal is a hard failure (non-zero exit): the formatter produced
        // output that would have destroyed user data. Files were left intact.
        return Err(format!(
            "{} file(s) left untouched — formatting would have produced unparseable output. \
             This is a formatter bug; please report it.",
            refused_count
        )
        .into());
    }

    if check && changed_count > 0 {
        if verbosity != Verbosity::Quiet {
            println!(
                "\n{} file(s) would be reformatted, {} file(s) already formatted",
                changed_count, unchanged_count
            );
        }
        return Err(format!("{} file(s) need formatting", changed_count).into());
    }

    if verbosity != Verbosity::Quiet {
        println!(
            "{} file(s) reformatted, {} file(s) left unchanged",
            changed_count, unchanged_count
        );
    }

    Ok(())
}

fn discover_ps_files(dir: &Path) -> Result<Vec<PathBuf>, Box<dyn std::error::Error>> {
    let mut files = Vec::new();
    collect_ps_files(dir, &mut files)?;
    files.sort();
    Ok(files)
}

fn collect_ps_files(
    dir: &Path,
    files: &mut Vec<PathBuf>,
) -> Result<(), Box<dyn std::error::Error>> {
    for entry in std::fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();

        if path.is_dir() {
            let name = path.file_name().unwrap().to_string_lossy();
            if !name.starts_with('.') && name != "node_modules" && name != "target" {
                collect_ps_files(&path, files)?;
            }
        } else if path.extension().is_some_and(|e| e == "ps") {
            files.push(path);
        }
    }
    Ok(())
}

/// Format a PythScribe source file.
/// Normalizes indentation, trailing whitespace, and blank lines.
fn format_source(source: &str, indent_size: usize) -> String {
    let indent_str = " ".repeat(indent_size);
    let mut result = String::new();
    let mut consecutive_blank = 0;

    // Zone state at the start of each line. A line that does not begin in a code
    // zone (`Some(_)`) is inside an unterminated string / f-string / docstring,
    // where indentation, trailing spaces and blank lines are string *content* —
    // reformatting them changes the string's value. Such lines are emitted
    // byte-for-byte. Same bug class and same shared classifier as the Tier A fix
    // in `pyths_expand::lib` (issue #193 / #192).
    let line_states = pyths_expand::zones::line_start_states(source);

    // The lexer accepts any *consistent* indentation width, so the formatter
    // must NOT assume the file is indented in 4-space units. Infer the file's
    // own space-per-level unit; a 2-space, 3-space, or 8-space file then
    // round-trips correctly instead of having its body collapsed to column 0
    // by an integer `spaces / 4 == 0`. `indent_size` remains the *output*
    // width (one level → `indent_size` spaces); the inferred unit is only the
    // *input* divisor. Conflating the two was the data-loss bug.
    let input_unit = infer_indent_unit(source, &line_states).unwrap_or(indent_size);

    for (i, line) in source.lines().enumerate() {
        if matches!(line_states.get(i), Some(Some(_))) {
            // Inside a string zone: preserve the line exactly.
            result.push_str(line);
            result.push('\n');
            consecutive_blank = 0;
            continue;
        }

        let trimmed = line.trim_end(); // Remove trailing whitespace

        if trimmed.is_empty() {
            consecutive_blank += 1;
            // Max 2 consecutive blank lines
            if consecutive_blank <= 2 {
                result.push('\n');
            }
            continue;
        }

        consecutive_blank = 0;

        // Calculate the leading whitespace and normalize indentation
        let leading = line.len() - line.trim_start().len();
        let leading_chars: Vec<char> = line[..leading].chars().collect();

        // Count indent level: tabs count as 1 level, spaces by the inferred unit
        let indent_level = count_indent_level(&leading_chars, input_unit);

        // Re-emit with normalized indentation
        for _ in 0..indent_level {
            result.push_str(&indent_str);
        }
        result.push_str(trimmed.trim_start());
        result.push('\n');
    }

    // Ensure single trailing newline
    while result.ends_with("\n\n") {
        result.pop();
    }
    if !result.ends_with('\n') && !result.is_empty() {
        result.push('\n');
    }

    result
}

/// Map a line's leading whitespace to a block-nesting level.
///
/// `unit` is the file's inferred spaces-per-level (see `infer_indent_unit`),
/// NOT a hard-coded 4. Each tab is one level (tab-indented files nest one tab
/// per level); spaces are divided by the inferred unit. `unit` is clamped to
/// at least 1 so a file with no space indentation (pure-tab or unindented)
/// can never divide by zero.
fn count_indent_level(chars: &[char], unit: usize) -> usize {
    let unit = unit.max(1);
    let mut spaces = 0;
    let mut tabs = 0;
    for &c in chars {
        match c {
            ' ' => spaces += 1,
            '\t' => tabs += 1,
            _ => break,
        }
    }
    tabs + (spaces / unit)
}

/// Infer the source file's indentation unit — the number of spaces that makes
/// up one level of block nesting.
///
/// The lexer (`pyths_lexer::indent`) accepts any *consistent* indentation
/// width; it does not require 4. So the formatter must discover the file's own
/// unit rather than assume one. We take the **smallest positive leading-space
/// count** among genuine code lines:
///   - lines whose start is inside a string/f-string/docstring zone are skipped
///     (their leading whitespace is string *content*, not block structure);
///   - blank and comment-only lines are skipped;
///   - lines whose leading whitespace begins with a tab are skipped (tabs are
///     handled directly as one-level-per-tab in `count_indent_level`).
///
/// The minimum is robust against alignment/continuation lines, which are
/// almost always indented *more* than one block unit, never less. Returns
/// `None` when the file has no space-indented code line, in which case the
/// caller keeps the configured output `indent_size`.
fn infer_indent_unit(
    source: &str,
    line_states: &[Option<pyths_expand::zones::ScanState>],
) -> Option<usize> {
    let mut min_indent: Option<usize> = None;
    for (i, line) in source.lines().enumerate() {
        // Inside a multi-line string zone → leading whitespace is content.
        if matches!(line_states.get(i), Some(Some(_))) {
            continue;
        }
        let trimmed = line.trim_end();
        let content = trimmed.trim_start();
        if content.is_empty() || content.starts_with('#') {
            continue; // blank or comment-only
        }
        let leading_len = line.len() - line.trim_start().len();
        let leading = &line[..leading_len];
        if leading.starts_with('\t') {
            continue; // tab-indented line — handled by the tab path
        }
        let spaces = leading.chars().take_while(|&c| c == ' ').count();
        if spaces > 0 {
            min_indent = Some(match min_indent {
                Some(m) => m.min(spaces),
                None => spaces,
            });
        }
    }
    min_indent
}

/// Reparse-guard: refuse any formatted output that fails to parse when the
/// original parsed cleanly.
///
/// A formatter must NEVER produce non-reparsing output — doing so silently
/// corrupts user source. This is a cheap, absolute backstop independent of the
/// indent-inference fix: it catches this bug and any *future* formatter bug
/// before the corrupt bytes are written to disk. If the original itself does
/// not parse, we cannot use parse-success as a corruption signal, so we allow
/// the reformat (no regression versus the previous unconditional behavior).
fn reparse_guard(original: &str, formatted: &str) -> Result<(), String> {
    if pyths_parser::parse(original).is_ok() && pyths_parser::parse(formatted).is_err() {
        return Err("formatted output no longer parses (original parsed cleanly)".to_string());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::format_source;

    // Issue #193: a per-line reformat with no lexical state mangled the CONTENT
    // of triple-quoted strings (reindented, trailing-trimmed, blank-collapsed).
    #[test]
    fn preserves_triple_quoted_string_content() {
        let src = "x = \"\"\"\n  indented\n\"\"\"\n";
        // Indentation inside the string is content and must survive.
        assert_eq!(format_source(src, 4), src);
    }

    #[test]
    fn preserves_trailing_spaces_and_blank_runs_in_docstring() {
        let src = "doc = \"\"\"\nline with trailing   \n\n\n\n   deep indent\n\"\"\"\n";
        assert_eq!(format_source(src, 4), src);
    }

    #[test]
    fn still_formats_real_code_around_strings() {
        // Code before/after a string is still normalized; the string is not.
        let src = "def f():\n        x = \"\"\"\n  keep\n\"\"\"\n";
        let out = format_source(src, 4);
        assert!(out.contains("    x = \"\"\"")); // 8 spaces -> 4 (indent level 1)
        assert!(out.contains("\n  keep\n")); // string body untouched
    }

    // ---- Data-loss regression: non-4-space indentation ----

    fn parses(src: &str) -> bool {
        pyths_parser::parse(src).is_ok()
    }

    /// The original bug: a 2-space-indented file had every body line rewritten
    /// to column 0 (`2 / 4 == 0`), the file overwritten, and the result no
    /// longer parsed ("Expected INDENT"). The formatter must now infer the
    /// file's 2-space unit and produce PARSEABLE output.
    #[test]
    fn two_space_file_is_not_flattened_and_reparses() {
        let src = "def f():\n  x = 1\n  return x\n";
        assert!(parses(src), "sanity: the 2-space source itself parses");
        let out = format_source(src, 4);
        // Body must remain indented (never collapsed to column 0).
        assert!(
            out.contains("\n    x = 1\n") && out.contains("\n    return x\n"),
            "2-space body must map to one level (4 spaces), got:\n{out}"
        );
        // The whole point of the bug: output MUST still parse.
        assert!(parses(&out), "formatted 2-space file must parse:\n{out}");
    }

    #[test]
    fn three_space_file_reparses() {
        let src = "def f():\n   x = 1\n   if x:\n      return x\n";
        let out = format_source(src, 4);
        assert!(parses(&out), "3-space file:\n{out}");
        // Two nesting levels preserved: 3->4, 6->8.
        assert!(out.contains("\n    x = 1\n"), "{out}");
        assert!(out.contains("\n        return x\n"), "{out}");
    }

    #[test]
    fn tab_indented_file_reparses() {
        let src = "def f():\n\tx = 1\n\tif x:\n\t\treturn x\n";
        let out = format_source(src, 4);
        assert!(parses(&out), "tab file:\n{out}");
        assert!(out.contains("\n    x = 1\n"), "{out}");
        assert!(out.contains("\n        return x\n"), "{out}");
    }

    /// No regression on standard 4-space code.
    #[test]
    fn four_space_file_unchanged_and_reparses() {
        let src = "def f():\n    x = 1\n    if x:\n        return x\n";
        let out = format_source(src, 4);
        assert_eq!(out, src, "already-4-space code must be left as-is");
        assert!(parses(&out));
    }

    /// Idempotence: formatting twice equals formatting once.
    #[test]
    fn idempotent_across_indent_widths() {
        for src in [
            "def f():\n  x = 1\n  return x\n",     // 2-space
            "def f():\n   x = 1\n   return x\n",   // 3-space
            "def f():\n\tx = 1\n\treturn x\n",     // tab
            "def f():\n    x = 1\n    return x\n", // 4-space
        ] {
            let once = format_source(src, 4);
            let twice = format_source(&once, 4);
            assert_eq!(once, twice, "fmt must be idempotent for {src:?}");
        }
    }

    #[test]
    fn infer_unit_picks_minimum_positive_space_indent() {
        let states = pyths_expand::zones::line_start_states;
        let src = "def f():\n  x = 1\n    y = 2\n"; // 2 and 4 spaces present
        assert_eq!(super::infer_indent_unit(src, &states(src)), Some(2));
        let src4 = "def f():\n    x = 1\n        y = 2\n";
        assert_eq!(super::infer_indent_unit(src4, &states(src4)), Some(4));
        let none = "x = 1\ny = 2\n"; // no indentation
        assert_eq!(super::infer_indent_unit(none, &states(none)), None);
        // String-zone indentation must not be counted as the unit.
        let strsrc = "x = \"\"\"\n  two\n\"\"\"\ndef f():\n    y = 1\n";
        assert_eq!(super::infer_indent_unit(strsrc, &states(strsrc)), Some(4));
    }

    #[test]
    fn reparse_guard_refuses_corrupting_output_only() {
        // good original + good formatted → allowed
        assert!(super::reparse_guard("x = 1\n", "x = 1\n").is_ok());
        // good original + broken formatted → refused
        assert!(super::reparse_guard("def f():\n    x = 1\n", "def f():\nx = 1\n").is_err());
        // broken original → we don't gate (no regression)
        assert!(super::reparse_guard("def f(:\n", "def g(:\n").is_ok());
    }
}
