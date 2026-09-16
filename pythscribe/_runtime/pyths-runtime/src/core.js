// PythScribe Runtime — Core (Worker / tree-shake-safe) subpath
// ============================================================
// The `--target worker` entry (`pyths-runtime/core`). Zero references to
// dom.js, react.js, document, or window — safe to import in Cloudflare
// Workers, Deno, WASM-host environments, and any context where browser
// globals are unavailable. (Fix: B-030.)
//
// delta4 DRIFT-PROOF export surface: identical to index.js minus the React
// layer. The compiler emits the SAME helper set for `--target worker` as for
// the default target (only the import specifier changes), so this entry must
// export every compiler-emittable symbol too. `export *` from the four
// canonical helper modules — none of which touches a browser global — makes
// that automatic; the hand-curated list this replaced silently drifted 43
// symbols behind (int("1") failed ESM instantiation under --target worker).
// Enforced by EMITTABLE_RUNTIME_SYMBOLS + the cli_test.rs drift guard and
// both-targets package-boundary tests (see index.js's header comment).
//
// Transitive import graph (unchanged):
//   core.js → operators.js → runtime.js   (no further imports)
//   core.js → runtime.js                  (no further imports)
//   core.js → types.js                    (no further imports)
//   core.js → classes.js                  (no further imports)

export * from "./runtime.js";
export * from "./operators.js";
export * from "./types.js";
export * from "./classes.js";

//# sourceMappingURL=core.js.map
