pub mod bridge;
pub mod cert;
pub mod emit;
pub mod optimize;
pub mod types;

use std::collections::{BTreeMap, BTreeSet};

use pyths_hir::{analyze_module, WasmAnalysis};
use pyths_syntax::ast::Module;
use types::{to_wasm_type, WasmType};

/// #364 compile-time fallback reason recorded against a function that codegen
/// could not lower to VALID WASM (a type-lowering gap the AST-level fragment
/// checker did not catch — e.g. an i32 boolean stored into an i64 accumulator,
/// `ans += x < y`, cluster03). The function stays on the correct JS path.
const INVALID_WASM_FALLBACK_REASON: &str =
    "WASM codegen produced invalid WASM for this function (type-lowering gap); \
     it stays on the correct JS path (#364 compile-time fallback)";

/// Validate emitted WASM bytes against the spec (with the compiler's feature
/// set). `false` means the module would fail `WebAssembly.instantiate` — the
/// signal for the compile-time fallback.
fn wasm_is_valid(bytes: &[u8]) -> bool {
    if bytes.is_empty() {
        return true;
    }
    wasmparser::Validator::new().validate_all(bytes).is_ok()
}

/// Per-function metadata needed by the bridge generator.
pub struct WasmExportInfo {
    pub name: String,
    pub params: Vec<(String, WasmType)>,
    /// None for void functions.
    pub return_type: Option<WasmType>,
}

/// Output from WASM code generation.
pub struct WasmCodegenOutput {
    /// The compiled WASM binary.
    pub wasm: Vec<u8>,
    /// Names of functions that were compiled to WASM.
    pub compiled_functions: Vec<String>,
    /// Functions that were rejected with reasons.
    pub rejected_functions: Vec<(String, String)>,
    /// Per-function metadata for bridge generation.
    pub export_info: Vec<WasmExportInfo>,
    /// Sorted set of math.* functions imported (e.g. {"pow", "sqrt", "sin"}).
    /// Replaces the previous `needs_pow_import` boolean.
    pub math_imports: BTreeSet<String>,
    /// Whether any compiled function uses strings, requiring string helpers in bridge.
    pub needs_strings: bool,
    /// Whether any compiled function uses raise/assert/try (Tier 7).
    /// When true, the bridge checks `__err_code` after every call and throws.
    pub needs_errors: bool,
    /// User-defined exception classes (name → error code), assigned codes 100+.
    /// Surfaced to the bridge so JS errors carry the correct `name`.
    pub custom_exceptions: BTreeMap<String, i32>,
    /// Whether any compiled function uses dict operations. When true, the
    /// bridge generates a `__dict` import namespace.
    pub needs_dicts: bool,
    /// #358: whether the module exports the `__ovf` i64-exactness flag
    /// (true for every non-empty module). The bridge checks the flag after
    /// every call and re-runs on the exact JS twin (or throws when no twin
    /// is available) instead of returning a silently wrapped value.
    pub has_ovf: bool,
}

impl WasmCodegenOutput {
    /// Backwards-compatible accessor: was math.pow imported?
    pub fn needs_pow_import(&self) -> bool {
        self.math_imports.contains("pow")
    }
}

/// Empty output (no function admitted to WASM). Carries the rejection reasons
/// through so `--verbose` can explain why each function stayed on the JS path.
fn empty_output(rejected: Vec<(String, String)>) -> WasmCodegenOutput {
    WasmCodegenOutput {
        wasm: vec![],
        compiled_functions: vec![],
        rejected_functions: rejected,
        export_info: vec![],
        math_imports: BTreeSet::new(),
        needs_strings: false,
        needs_errors: false,
        custom_exceptions: BTreeMap::new(),
        needs_dicts: false,
        has_ovf: false,
    }
}

/// #364 compile-time fallback: given that emitting `analysis.eligible` yields
/// INVALID WASM (would fail `WebAssembly.instantiate`), move the offending
/// function(s) from `eligible` to `rejected` until the remaining admitted set
/// emits valid WASM (possibly the empty set). Precisely isolates a single
/// culprit by leave-one-out (`ans += x < y` — cluster03); handles multiple
/// interacting culprits by removing candidates deterministically until valid.
/// Only ever runs when the initial emit is already invalid, so the happy path
/// pays one validation, not this loop.
fn exclude_invalid_functions(module: &Module, analysis: &mut WasmAnalysis) {
    loop {
        if analysis.eligible.is_empty() {
            return;
        }
        let mut names: Vec<String> = analysis.eligible.keys().cloned().collect();
        names.sort();

        // Try to isolate a single culprit: the function whose removal makes the
        // rest of the admitted set validate.
        let mut isolated = false;
        for n in &names {
            let mut trial = analysis.eligible.clone();
            trial.remove(n);
            let reduced = WasmAnalysis {
                eligible: trial,
                rejected: Vec::new(),
            };
            let bytes = emit::WasmEmitter::new().emit_module(module, &reduced);
            if reduced.eligible.is_empty() || wasm_is_valid(&bytes) {
                analysis.eligible.remove(n);
                analysis
                    .rejected
                    .push((n.clone(), INVALID_WASM_FALLBACK_REASON.to_string()));
                isolated = true;
                break;
            }
        }
        if isolated {
            return;
        }

        // No single removal validated → multiple interacting culprits. Remove
        // the first candidate and re-loop; each pass drops ≥1 function, so this
        // terminates (worst case: empty admitted set → no WASM, all JS).
        let n = names[0].clone();
        analysis.eligible.remove(&n);
        analysis
            .rejected
            .push((n, INVALID_WASM_FALLBACK_REASON.to_string()));
    }
}

/// Compile a PythScribe module to WASM, targeting only eligible numeric functions.
pub fn codegen_wasm(module: &Module) -> WasmCodegenOutput {
    let mut analysis = analyze_module(module);

    if analysis.eligible.is_empty() {
        return empty_output(analysis.rejected);
    }

    // #364 compile-time fallback: emit once and validate. If the module is
    // invalid WASM (a lowering gap the AST fragment checker missed), demote the
    // offending function(s) to JS and settle on the largest admitted subset
    // that produces a valid module. Guarantees js+wasm output NEVER ships a
    // module that fails to instantiate. On the happy path this is a single
    // extra validation of an already-valid module.
    {
        let bytes = emit::WasmEmitter::new().emit_module(module, &analysis);
        if !wasm_is_valid(&bytes) {
            exclude_invalid_functions(module, &mut analysis);
        }
    }
    if analysis.eligible.is_empty() {
        return empty_output(analysis.rejected);
    }

    let mut emitter = emit::WasmEmitter::new();
    let wasm = emitter.emit_module(module, &analysis);
    let compiled_functions: Vec<String> = analysis.eligible.keys().cloned().collect();

    // Build export_info from eligible functions
    let mut export_info: Vec<WasmExportInfo> = analysis
        .eligible
        .values()
        .map(|info| {
            let params: Vec<(String, WasmType)> = info
                .params
                .iter()
                .filter_map(|(name, ty)| to_wasm_type(ty).map(|wt| (name.clone(), wt)))
                .collect();
            let return_type = to_wasm_type(&info.return_type);
            WasmExportInfo {
                name: info.name.clone(),
                params,
                return_type,
            }
        })
        .collect();
    // Sort for deterministic output
    export_info.sort_by(|a, b| a.name.cmp(&b.name));

    // Math imports were collected during emit_module
    let math_imports = emitter.math_imports().clone();
    let needs_errors = emitter.needs_errors();
    let custom_exceptions = emitter.custom_exceptions().clone();
    let needs_dicts = emitter.needs_dicts();

    // "Needs strings" really means "the module emitted a heap" — true for
    // string OR collection params/returns/bodies. Use the emitter's own flag
    // (the single source of truth) rather than recomputing a narrower version:
    // the codegen exports `__alloc`/`__heap_ptr` exactly when this is set, and
    // the bridge must agree so it emits the arena reset + marshallers (B-034).
    let needs_strings = emitter.needs_strings();

    WasmCodegenOutput {
        wasm,
        compiled_functions,
        rejected_functions: analysis.rejected,
        export_info,
        math_imports,
        needs_strings,
        needs_errors,
        custom_exceptions,
        needs_dicts,
        has_ovf: true,
    }
}

/// Generate JavaScript bridge/glue code for a WASM output (Browser default).
///
/// `js_twins` (#358): optional JS source containing the exact
/// (arbitrary-precision) implementations of the compiled functions —
/// embedded in the glue so a flagged `__ovf` call transparently re-runs on
/// the exact path. When `None`, a flagged call throws instead.
pub fn generate_bridge_js(
    output: &WasmCodegenOutput,
    wasm_filename: &str,
    js_twins: Option<&str>,
) -> String {
    bridge::generate_bridge(
        wasm_filename,
        &output.export_info,
        &output.math_imports,
        output.needs_strings,
        output.needs_errors,
        &output.custom_exceptions,
        output.needs_dicts,
        output.has_ovf,
        js_twins,
    )
}

/// Generate the bridge for a specific runtime target.
pub fn generate_bridge_for_target(
    target: bridge::BridgeTarget,
    wasm_filename: &str,
    output: &WasmCodegenOutput,
) -> String {
    bridge::generate_bridge_for_target(
        target,
        wasm_filename,
        &output.wasm,
        &output.export_info,
        &output.math_imports,
        output.needs_strings,
        output.needs_errors,
        &output.custom_exceptions,
        output.needs_dicts,
        output.has_ovf,
        // Edge targets are self-contained modules with no runtime import
        // resolution — no JS twin; a flagged call throws loudly instead of
        // returning a silently wrong value.
        None,
    )
}
