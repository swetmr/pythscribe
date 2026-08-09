use std::path::{Path, PathBuf};

use super::output::{self, Verbosity};

pub fn run(path: Option<&str>, verbosity: Verbosity) -> Result<(), Box<dyn std::error::Error>> {
    let search_dir = path.map(Path::new).unwrap_or_else(|| Path::new("."));

    if !search_dir.exists() {
        return Err(format!("Path not found: {}", search_dir.display()).into());
    }

    // Discover test files
    let test_files = discover_test_files(search_dir)?;

    if test_files.is_empty() {
        if verbosity != Verbosity::Quiet {
            println!("No test files found (looking for test_*.ps or *_test.ps)");
        }
        return Ok(());
    }

    if verbosity != Verbosity::Quiet {
        println!("Discovered {} test file(s)\n", test_files.len());
    }

    let mut passed = 0;
    let mut failed = 0;
    let mut errors: Vec<(String, String)> = Vec::new();

    for test_file in &test_files {
        let filename = test_file.display().to_string();
        let short_name = test_file.file_name().unwrap().to_string_lossy();

        if verbosity == Verbosity::Verbose {
            println!("Running {}...", filename);
        }

        match run_test_file(test_file) {
            Ok(test_output) => {
                passed += 1;
                if verbosity != Verbosity::Quiet {
                    output::success(&format!("  PASS {}", short_name));
                    if verbosity == Verbosity::Verbose && !test_output.is_empty() {
                        println!("    {}", test_output.trim().replace('\n', "\n    "));
                    }
                }
            }
            Err(e) => {
                failed += 1;
                if verbosity != Verbosity::Quiet {
                    println!("  FAIL {}", short_name);
                    let err_msg = e.to_string();
                    if verbosity == Verbosity::Verbose {
                        println!("    {}", err_msg.trim().replace('\n', "\n    "));
                    }
                    errors.push((filename, err_msg));
                } else {
                    errors.push((filename, e.to_string()));
                }
            }
        }
    }

    if verbosity != Verbosity::Quiet {
        println!(
            "\n{} passed, {} failed, {} total",
            passed,
            failed,
            test_files.len()
        );

        if !errors.is_empty() {
            println!("\nFailures:");
            for (file, err) in &errors {
                println!("  {} — {}", file, err.lines().next().unwrap_or(""));
            }
        }
    }

    if !errors.is_empty() {
        return Err(format!("{} test(s) failed", failed).into());
    }

    Ok(())
}

fn discover_test_files(dir: &Path) -> Result<Vec<PathBuf>, Box<dyn std::error::Error>> {
    let mut files = Vec::new();

    if dir.is_file() && dir.extension().is_some_and(|e| e == "ps") {
        files.push(dir.to_path_buf());
        return Ok(files);
    }

    if dir.is_dir() {
        collect_test_files(dir, &mut files)?;
    }

    files.sort();
    Ok(files)
}

fn collect_test_files(
    dir: &Path,
    files: &mut Vec<PathBuf>,
) -> Result<(), Box<dyn std::error::Error>> {
    for entry in std::fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();

        if path.is_dir() {
            // Skip hidden dirs and node_modules
            let name = path.file_name().unwrap().to_string_lossy();
            if !name.starts_with('.') && name != "node_modules" && name != "target" {
                collect_test_files(&path, files)?;
            }
        } else if path.extension().is_some_and(|e| e == "ps") {
            let stem = path.file_stem().unwrap().to_string_lossy();
            if stem.starts_with("test_") || stem.ends_with("_test") {
                files.push(path);
            }
        }
    }
    Ok(())
}

fn run_test_file(path: &Path) -> Result<String, Box<dyn std::error::Error>> {
    let source = std::fs::read_to_string(path)?;
    let filename = path.file_name().unwrap().to_string_lossy();

    // Parse
    let module = pyths_parser::parse(&source).map_err(|errors| {
        let messages: Vec<String> = errors.iter().map(|e| e.message.clone()).collect();
        format!("Parse error in {}: {}", filename, messages.join(", "))
    })?;

    // Codegen
    let js = pyths_codegen_js::codegen_inline(&module);

    // A9: write into a fresh, private, per-invocation temp DIRECTORY instead
    // of a predictable `pyths_test_<stem>.mjs` in the shared OS temp dir (a
    // fixed name is a symlink/TOCTOU target that node then executes).
    let temp_dir = super::procutil::make_private_temp_dir("test")?;
    let stem = path.file_stem().unwrap().to_string_lossy();
    let temp_file = temp_dir.join(format!("pyths_test_{}.mjs", stem));
    std::fs::write(&temp_file, &js)?;

    // A13: resolve `node` to an absolute path off PATH (never cwd); a hostile
    // `node.exe` in the project root must not be preferred. `PYTHS_NODE`
    // overrides the interpreter explicitly.
    let node = match super::procutil::resolve_program("node", Some("PYTHS_NODE")) {
        Some(n) => n,
        None => {
            let _ = std::fs::remove_dir_all(&temp_dir);
            return Err(
                "could not find `node` on PATH — install Node.js or set PYTHS_NODE to its path"
                    .into(),
            );
        }
    };

    // Run with Node.js
    let output = std::process::Command::new(&node).arg(&temp_file).output()?;

    // Clean up the whole private temp directory.
    let _ = std::fs::remove_dir_all(&temp_dir);

    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).to_string();

    if !output.status.success() {
        let msg = if stderr.is_empty() {
            stdout.clone()
        } else {
            stderr
        };
        return Err(format!("Test failed:\n{}", msg).into());
    }

    Ok(stdout)
}
