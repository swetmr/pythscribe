// A1 single-source-of-truth shim: the canonical runtime lives in the npm
// package at ../../../runtime/src/. This crate no longer keeps a second copy;
// it re-exports the canonical implementation so the compiler embed path
// (runtime_package_files / inliner) and the JS unit tests (runtime.test.js)
// exercise exactly one runtime. Do not add divergent code here.
export * from "../../../runtime/src/runtime.js";

//# sourceMappingURL=runtime.js.map
