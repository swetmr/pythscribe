pub mod builtins;
pub mod cert;
pub mod css_properties;
pub mod dts;
pub mod emit;
pub mod method_lowering;
pub mod method_table;
pub mod react;
pub mod sourcemap;

pub use emit::JsCodegen;
pub use emit::{take_emit_overflow, MAX_EMIT_DEPTH};
// E7 sub-part 3 r2: the manifest-membership assertion behind the
// fast-path shadow gate — exported (hidden) for the injection proof test.
#[doc(hidden)]
pub use emit::assert_specially_lowered_manifest;
#[doc(hidden)]
pub use emit::inline_runtime_for_test;
// delta4: the checked manifest of every compiler-emittable runtime symbol —
// consumed by the cli_test.rs export-surface drift guard.
pub use emit::EMITTABLE_RUNTIME_SYMBOLS;
// E7 sub-part 3: the checked manifest of every specially-lowered builtin —
// consumed by the generated shadow matrix in behavioral_differential.rs.
pub use emit::SPECIALLY_LOWERED_BUILTINS;
pub use sourcemap::SourceMapBuilder;

/// Result of compilation with optional source map.
pub struct CodegenOutput {
    pub js: String,
    pub source_map: Option<String>,
}

/// Compile an AST module to JavaScript source code (with import statement for runtime).
pub fn codegen(module: &pyths_syntax::ast::Module) -> String {
    let mut gen = JsCodegen::new();
    gen.emit_module(module);
    gen.finish()
}

/// Like [`codegen`] but with `pyths.toml [npm.imports]` overrides
/// applied. Map keys are Python-source module names (with dots for
/// sub-paths); values are the JS module specifier emitted verbatim.
pub fn codegen_with_npm_imports(
    module: &pyths_syntax::ast::Module,
    npm_imports: &std::collections::HashMap<String, String>,
) -> String {
    let mut gen = JsCodegen::new();
    gen.set_npm_imports(npm_imports.clone());
    gen.emit_module(module);
    gen.finish()
}

/// Builder-style options for [`codegen_with_options`]. Each field has a
/// sensible default so callers only specify what they want non-default.
#[derive(Default)]
pub struct CodegenOptions<'a> {
    pub npm_imports: Option<&'a std::collections::HashMap<String, String>>,
    pub react_refresh: bool,
    pub wasm_skip: Option<&'a [String]>,
    pub wasm_glue_filename: Option<&'a str>,
    /// When `true`, runtime-helper imports are emitted from
    /// `"pyths-runtime/core"` instead of `"pyths-runtime"`. Set this
    /// for `--target worker` (Cloudflare Workers / DOM-free numeric output)
    /// so the compiled JS has no React/DOM transitive deps. (B-030 follow-up D)
    pub worker_runtime: bool,
}

/// Compile a module with the full set of optional codegen features.
/// `wasm_skip` + `wasm_glue_filename` together opt into bridge mode
/// (those functions become re-exports of the WASM glue).
pub fn codegen_with_options(
    module: &pyths_syntax::ast::Module,
    options: &CodegenOptions,
) -> String {
    let mut gen = JsCodegen::new();
    if let Some(imports) = options.npm_imports {
        gen.set_npm_imports(imports.clone());
    }
    if options.react_refresh {
        gen.enable_react_refresh();
    }
    if options.worker_runtime {
        gen.set_runtime_pkg("pyths-runtime/core");
    }
    if let Some(skip) = options.wasm_skip {
        gen.set_wasm_skip(skip.iter().cloned().collect());
    }
    gen.emit_module(module);
    if let (Some(_), Some(glue)) = (options.wasm_skip, options.wasm_glue_filename) {
        gen.emit_wasm_reexports(glue);
    }
    gen.finish()
}

/// Like [`codegen_with_options`] but also returns the subscript-routing
/// certificate (credible compilation, §7.2) and any codegen diagnostics.
pub struct CertifiedOutput {
    pub js: String,
    pub certificate: cert::Certificate,
    pub errors: Vec<String>,
}

pub fn codegen_certified(
    module: &pyths_syntax::ast::Module,
    options: &CodegenOptions,
) -> CertifiedOutput {
    let mut gen = JsCodegen::new();
    if let Some(imports) = options.npm_imports {
        gen.set_npm_imports(imports.clone());
    }
    if options.react_refresh {
        gen.enable_react_refresh();
    }
    if options.worker_runtime {
        gen.set_runtime_pkg("pyths-runtime/core");
    }
    if let Some(skip) = options.wasm_skip {
        gen.set_wasm_skip(skip.iter().cloned().collect());
    }
    gen.emit_module(module);
    if let (Some(_), Some(glue)) = (options.wasm_skip, options.wasm_glue_filename) {
        gen.emit_wasm_reexports(glue);
    }
    let mut certificate = gen.take_certificate();
    let errors = gen.take_errors();
    // `finish_certified` returns the final js plus the layout needed to map the
    // BODY-relative site offsets (recorded during codegen, before the prelude)
    // to FINAL-js offsets: `final = body_shift + o` for `o >= directive_len`.
    // Stamp them onto the drained certificate so `check_certificate` can run
    // its positional (route-swap) cross-check. (`take_certificate` already
    // moved the cert off `gen`, so `finish` cannot set these itself.)
    let (js, body_shift, directive_len) = gen.finish_certified();
    certificate.body_shift = body_shift;
    certificate.directive_len = directive_len;
    CertifiedOutput {
        js,
        certificate,
        errors,
    }
}

/// Compile an AST module to self-contained JavaScript (runtime helpers inlined).
pub fn codegen_inline(module: &pyths_syntax::ast::Module) -> String {
    let mut gen = JsCodegen::new_inline();
    gen.emit_module(module);
    gen.finish()
}

/// Inline-runtime codegen with source maps — used by `pyths run --explain`.
/// Output is a self-contained `.mjs` (no external imports) plus a `.js.map`
/// JSON string for mapping post-mortem stack frames back to `.ps` source.
pub fn codegen_inline_with_sourcemap(
    module: &pyths_syntax::ast::Module,
    source: &str,
    source_file: &str,
    output_file: &str,
) -> CodegenOutput {
    let mut gen = JsCodegen::new_inline();
    gen.enable_sourcemap(source, source_file, output_file);
    gen.emit_module(module);
    let (js, source_map) = gen.finish_with_sourcemap();
    CodegenOutput {
        js,
        source_map: Some(source_map),
    }
}

/// Inline-runtime codegen returning the RAW resolved source mappings (final-JS
/// positions) instead of serialized JSON — used by `pyths bundle --sourcemap`
/// to compose per-module maps into one multi-source bundle map.
pub struct InlineRawMapOutput {
    pub js: String,
    pub mappings: Vec<sourcemap::Mapping>,
}

pub fn codegen_inline_with_raw_map(
    module: &pyths_syntax::ast::Module,
    source: &str,
    source_file: &str,
) -> InlineRawMapOutput {
    let mut gen = JsCodegen::new_inline();
    gen.enable_sourcemap(source, source_file, source_file);
    gen.emit_module(module);
    let (js, mappings) = gen.finish_with_raw_map();
    InlineRawMapOutput { js, mappings }
}

/// Generate TypeScript declaration file (.d.ts) from an AST module.
pub fn codegen_dts(module: &pyths_syntax::ast::Module) -> String {
    let mut gen = dts::DtsGenerator::new();
    gen.generate(module)
}

/// Compile an AST module to JavaScript, skipping WASM-compiled functions and
/// re-exporting them from the glue module. `npm_imports` is consulted before
/// the built-in module-resolution chain (pass an empty `HashMap` for the
/// default behavior).
pub fn codegen_with_wasm_bridge(
    module: &pyths_syntax::ast::Module,
    wasm_functions: &[String],
    glue_filename: &str,
    npm_imports: &std::collections::HashMap<String, String>,
) -> String {
    let mut gen = JsCodegen::new();
    gen.set_npm_imports(npm_imports.clone());
    gen.set_wasm_skip(wasm_functions.iter().cloned().collect());
    gen.emit_module(module);
    gen.emit_wasm_reexports(glue_filename);
    gen.finish()
}

/// Compile an AST module to JavaScript with source map. `npm_imports` is
/// consulted before the built-in module-resolution chain (pass an empty
/// `HashMap` for the default behavior). When `worker_runtime` is `true`,
/// runtime-helper imports come from `"pyths-runtime/core"` (B-030 follow-up D).
pub fn codegen_with_sourcemap(
    module: &pyths_syntax::ast::Module,
    source: &str,
    source_file: &str,
    output_file: &str,
    npm_imports: &std::collections::HashMap<String, String>,
) -> CodegenOutput {
    codegen_with_sourcemap_and_options(
        module,
        source,
        source_file,
        output_file,
        npm_imports,
        false,
        false,
    )
}

/// Like [`codegen_with_sourcemap`] but with extended options.
/// - `worker_runtime`: import runtime helpers from `"pyths-runtime/core"`.
/// - `omit_sources_content` (A17): drop `sourcesContent` from the emitted map
///   so original `.ps` source is not shipped in production `.js.map` files.
pub fn codegen_with_sourcemap_and_options(
    module: &pyths_syntax::ast::Module,
    source: &str,
    source_file: &str,
    output_file: &str,
    npm_imports: &std::collections::HashMap<String, String>,
    worker_runtime: bool,
    omit_sources_content: bool,
) -> CodegenOutput {
    let opts = CodegenOptions {
        npm_imports: Some(npm_imports),
        worker_runtime,
        ..Default::default()
    };
    codegen_with_sourcemap_full(
        module,
        source,
        source_file,
        output_file,
        &opts,
        omit_sources_content,
    )
}

/// Sourcemap codegen with the FULL [`CodegenOptions`] set — closes the
/// `--sourcemap` + auto/kernel gap where the sourcemap path ignored
/// `wasm_skip`/`wasm_glue_filename` (and `react_refresh`), so a kernel
/// module's `--sourcemap` output did not route through the WASM glue while
/// the non-sourcemap output did. The bridge re-exports are appended after the
/// mapped body and carry no mappings (they are generated shim lines, not
/// user code), so `.ps` frame resolution is unaffected.
pub fn codegen_with_sourcemap_full(
    module: &pyths_syntax::ast::Module,
    source: &str,
    source_file: &str,
    output_file: &str,
    options: &CodegenOptions,
    omit_sources_content: bool,
) -> CodegenOutput {
    let mut gen = JsCodegen::new();
    if let Some(imports) = options.npm_imports {
        gen.set_npm_imports(imports.clone());
    }
    if options.react_refresh {
        gen.enable_react_refresh();
    }
    if options.worker_runtime {
        gen.set_runtime_pkg("pyths-runtime/core");
    }
    if let Some(skip) = options.wasm_skip {
        gen.set_wasm_skip(skip.iter().cloned().collect());
    }
    gen.enable_sourcemap(source, source_file, output_file);
    gen.set_omit_sources_content(omit_sources_content);
    gen.emit_module(module);
    if let (Some(_), Some(glue)) = (options.wasm_skip, options.wasm_glue_filename) {
        gen.emit_wasm_reexports(glue);
    }
    let (js, source_map) = gen.finish_with_sourcemap();
    CodegenOutput {
        js,
        source_map: Some(source_map),
    }
}
