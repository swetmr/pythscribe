use std::path::Path;

use super::output::{self, Verbosity};

pub fn run(name: Option<&str>, verbosity: Verbosity) -> Result<(), Box<dyn std::error::Error>> {
    let project_name = name.unwrap_or("my-pyths-app");
    let project_dir = Path::new(project_name);

    if project_dir.exists() {
        return Err(format!("Directory '{}' already exists", project_name).into());
    }

    std::fs::create_dir_all(project_dir.join("src"))?;

    // pyths.toml
    std::fs::write(
        project_dir.join("pyths.toml"),
        format!(
            r#"[project]
name = "{}"
version = "0.1.0"

[build]
target = "js"
entry = "src/main.ps"
output = "dist/"
"#,
            project_name
        ),
    )?;

    // src/main.ps
    std::fs::write(
        project_dir.join("src").join("main.ps"),
        r#"# Welcome to PythScribe!
# Write Python. Ship to the Web.

def greet(name: str) -> str:
    return f"Hello, {name}!"

message = greet("World")
print(message)
"#,
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
