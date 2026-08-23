use std::path::Path;

use super::output::{self, Verbosity};

const MAIN_PS_TEMPLATE: &str = r#"# Welcome to PythScribe!
# Write Python. Ship to the Web.

def greet(name: str) -> str:
    return f"Hello, {name}!"

message = greet("World")
print(message)
"#;

pub fn run(name: Option<&str>, verbosity: Verbosity) -> Result<(), Box<dyn std::error::Error>> {
    let project_name = name.unwrap_or("my-pyths-app");
    let project_dir = Path::new(project_name);

    if project_dir.exists() {
        return Err(format!("Directory '{}' already exists", project_name).into());
    }

    std::fs::create_dir_all(project_dir.join("src"))?;

    // B7: scaffold files go through the ONE consolidated safe-write API
    // (ScaffoldIfAbsent) rather than a bare `std::fs::write`. Between the
    // `project_dir.exists()` check above and these writes a symlink could be
    // planted at either destination; ScaffoldIfAbsent inspects no-follow
    // (refusing symlinks / non-regular files) and creates exclusively
    // (`O_EXCL`/`CREATE_NEW`), so a raced file is refused, not followed.
    let toml = format!(
        r#"[project]
name = "{}"
version = "0.1.0"

[build]
# The compile target is chosen with the CLI `--target` flag; the default
# (no flag) is automatic routing — numeric kernels compile to WebAssembly,
# everything else to JavaScript. Pin JS-only with `--target js`.
entry = "src/main.ps"
output = "dist/"
"#,
        project_name
    );
    super::safewrite::write_single(
        &project_dir.join("pyths.toml"),
        toml.as_bytes(),
        super::safewrite::OutputKind::ScaffoldIfAbsent,
        false,
    )?;

    super::safewrite::write_single(
        &project_dir.join("src").join("main.ps"),
        MAIN_PS_TEMPLATE.as_bytes(),
        super::safewrite::OutputKind::ScaffoldIfAbsent,
        false,
    )?;

    if verbosity != Verbosity::Quiet {
        output::success(&format!("Created new PythScribe project: {}", project_name));
        println!();
        println!("  cd {}", project_name);
        println!("  pyths compile src/main.ps");
        println!("  pyths run src/main.ps");
    }

    Ok(())
}
