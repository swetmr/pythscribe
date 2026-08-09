pub mod checker;
pub mod errors;
pub mod stubs;
pub mod types;

pub use checker::TypeChecker;
pub use errors::TypeError;

/// Run the type checker on a parsed module, returning any type errors
/// found. Uses only bundled `.pyi` stubs.
pub fn check(module: &pyths_syntax::ast::Module) -> Vec<TypeError> {
    TypeChecker::check(module)
}

/// Run the type checker honoring project-local `.pyi` stub directories
/// before falling back to bundled stubs. Each `stub_paths` entry is
/// searched in declaration order for a `<module>.pyi`.
pub fn check_with_stub_paths(
    module: &pyths_syntax::ast::Module,
    stub_paths: &[std::path::PathBuf],
) -> Vec<TypeError> {
    TypeChecker::check_with_stub_paths(module, stub_paths)
}
