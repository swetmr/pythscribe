// PythScribe Runtime — Main entry point
// =====================================
// delta4 DRIFT-PROOF export surface: the four canonical helper modules ARE
// the export surface. Every runtime helper the compiler can emit an import
// for lives in (or is re-exported through) runtime.js / operators.js /
// types.js / classes.js, and BOTH package entry points (this file and the
// worker/edge `core.js`) re-export ALL of them with `export *` — so adding a
// helper to any of the four modules automatically exposes it on both entry
// points. Hand-curated name lists are exactly what drifted four times
// (missing __pyRangeIter, then pyComplex + 43 worker symbols); do NOT go
// back to one.
//
// The guarantee is ENFORCED, not just structural:
//   - `EMITTABLE_RUNTIME_SYMBOLS` in crates/pyths_codegen_js/src/emit.rs is
//     the checked manifest of every symbol the codegen can emit;
//     `need_runtime` debug_asserts membership (sole choke point).
//   - cli_test.rs `runtime_export_surface_covers_all_emittable_symbols`
//     imports BOTH entry points in node and fails on any manifest symbol
//     missing from either.
//   - cli_test.rs `round5_broad_package_esm_load_both_targets` compiles a
//     broad-helper program on the default AND `--target worker` targets and
//     runs the emitted ESM against the real package entries.

export * from "./runtime.js";
export * from "./operators.js";
export * from "./types.js";
export * from "./classes.js";

// Browser/React layer — index-only (core.js must stay worker-safe).
export { createPropTransformer, style, classes, component, psx } from "./react.js";

//# sourceMappingURL=index.js.map
