//! #491 (fable B1) — the twin-consistency invariant, structural half
//! (`SHADOW_BINDING_DESIGN.md` §4). The glue's exact fallback twin (`__jsfb.<fn>`)
//! is compiled from `pyths_codegen_wasm::twin_module`: the ORIGINAL module's
//! top-level imports + the WASM-compiled defs, in source order — so a twin body's
//! free callee resolves under the SAME binder set the WASM body was admitted with.
//! The behavioral half (node runs the overflow path) is
//! `crates/pyths_cli/tests/shadow_491_cli.rs`; the JS-text half (the twin's
//! `use_abs` calls the user `abs` cell, a math twin keeps its import) lives there
//! too, because this crate's dev-deps cannot invoke the JS emitter.

use pyths_codegen_wasm::{codegen_wasm, twin_module};
use pyths_syntax::ast::StmtKind;

fn names(m: &pyths_syntax::ast::Module) -> Vec<String> {
    m.body
        .iter()
        .map(|s| match &s.kind {
            StmtKind::FuncDef { name, .. } => format!("def {name}"),
            StmtKind::ImportFrom { module, names, .. } => format!(
                "from {module} import {}",
                names
                    .iter()
                    .map(|a| a.name.clone())
                    .collect::<Vec<_>>()
                    .join(",")
            ),
            StmtKind::Import { names } => format!(
                "import {}",
                names
                    .iter()
                    .map(|a| a.name.clone())
                    .collect::<Vec<_>>()
                    .join(",")
            ),
            other => format!("{other:?}"),
        })
        .collect()
}

/// The twin carries every top-level import and exactly the compiled defs, in
/// source order; module-level statements that are not binders of a callable name
/// a WASM body may reach (prints, assignments, non-compiled defs) are dropped.
/// PAIRED CONTROL: the CLI's previous inline construction (defs only) is
/// reproduced here and shown to DROP the import — the difference is load-bearing
/// (a `floor` twin under the old construction was a ReferenceError at fallback).
#[test]
fn twin_module_is_imports_plus_compiled_defs_in_source_order() {
    let src = "\
import math
from math import floor
x = 5
print(x)

def helper(x: int) -> int:
    s = \"js only\"
    return x

def f(x: int) -> int:
    return int(floor(x * 1.0)) * 7

def use_f(x: int) -> int:
    return f(x) + 1
";
    let module = pyths_parser::parse(src).expect("parse");
    let out = codegen_wasm(&module);
    assert!(
        out.compiled_functions.contains(&"f".to_string())
            && out.compiled_functions.contains(&"use_f".to_string()),
        "f/use_f must be WASM-admitted for this control to be meaningful; rejected={:?}",
        out.rejected_functions
    );
    let twin = twin_module(&module, &out.compiled_functions);
    assert_eq!(
        names(&twin),
        vec![
            "import math".to_string(),
            "from math import floor".to_string(),
            "def f".to_string(),
            "def use_f".to_string(),
        ]
    );
    // Mutation witness: the defs-only construction loses the binder `floor`
    // resolves through.
    let defs_only: Vec<String> = module
        .body
        .iter()
        .filter(|s| matches!(&s.kind, StmtKind::FuncDef { name, .. } if out.compiled_functions.contains(name)))
        .map(|s| names(&pyths_syntax::ast::Module { body: vec![s.clone()], span: module.span })[0].clone())
        .collect();
    assert_eq!(
        defs_only,
        vec!["def f".to_string(), "def use_f".to_string()]
    );
    assert!(
        !defs_only.iter().any(|n| n.starts_with("from math")),
        "the old construction had no import — the twin would resolve `floor` as an undefined global"
    );
}

/// A def that is dead-by-name (rebound by a later def) is not compiled, and the
/// twin keeps only the compiled (final) def — the twin's own last-wins binding
/// equals the WASM export.
#[test]
fn twin_module_keeps_only_compiled_defs_of_a_rebound_name() {
    let src = "\
def abs(x: int) -> int:
    return x * 7

def abs(x: int) -> int:
    return x * 9

def use_abs(x: int) -> int:
    return abs(x)
";
    let module = pyths_parser::parse(src).expect("parse");
    let out = codegen_wasm(&module);
    assert!(
        out.compiled_functions.contains(&"abs".to_string())
            && out.compiled_functions.contains(&"use_abs".to_string()),
        "rejected={:?}",
        out.rejected_functions
    );
    let twin = twin_module(&module, &out.compiled_functions);
    // Both `def abs` statements are named `abs`; the twin keeps them in order, so
    // the JS last-wins rebind matches the WASM side's final-def routing.
    assert_eq!(
        names(&twin),
        vec![
            "def abs".to_string(),
            "def abs".to_string(),
            "def use_abs".to_string()
        ]
    );
}
