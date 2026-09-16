//! #486 drift guard: the WASM-admission arity authority
//! (`pyths_hir::wasm_math_arity`, in the HIR crate) MUST agree, for every math
//! function, with the codegen arity table
//! (`pyths_codegen_wasm::emit::MATH_FUNCTIONS`). The two live in different
//! crates and are read at different phases (admission vs lowering); a boundary
//! test binds them so neither can silently drift into an under- or over-checked
//! arity (the exact class #486 closes — an unchecked arity that reaches codegen
//! and panics / drops an argument).

use pyths_codegen_wasm::emit::MATH_FUNCTIONS;
use pyths_hir::wasm_math_arity;

#[test]
fn wasm_math_arity_matches_codegen_table() {
    // Every codegen math function has the same arity in the admission authority.
    for (name, arity) in MATH_FUNCTIONS {
        assert_eq!(
            wasm_math_arity(name),
            Some(*arity as usize),
            "math.{name}: codegen table says arity {arity}, but the WASM-admission \
             authority `wasm_math_arity` disagrees — the two have drifted, so a \
             wrong-arity call would be admitted and miscompiled (#486)"
        );
    }
    // And the admission authority names no math function the codegen table lacks
    // (a name admitted-with-arity but never lowered would emit a call to a
    // missing import). Cross-check the reverse direction over the same set.
    let codegen_names: std::collections::BTreeSet<&str> =
        MATH_FUNCTIONS.iter().map(|(n, _)| *n).collect();
    for name in [
        "sqrt", "sin", "cos", "tan", "asin", "acos", "atan", "atan2", "log", "log2", "log10",
        "exp", "ceil", "floor", "fabs", "pow",
    ] {
        assert!(
            codegen_names.contains(name) == wasm_math_arity(name).is_some(),
            "math.{name}: admission authority and codegen table disagree on whether \
             it is a lowered math function"
        );
    }
}
