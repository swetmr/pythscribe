// Auto-routing E2E test: a single .ps file mixes React-DOM components
// (which must stay in JS) with pure numeric functions (which must
// auto-route to WASM). This test compiles with --target js+wasm and
// asserts:
//
//   1. Numeric functions land in the .wasm artifact (eligibility analysis).
//   2. DOM/string functions stay in the .js artifact.
//   3. The .js re-exports the WASM functions via the glue module.
//   4. Calling the JS module from Node correctly invokes WASM and
//      returns the right numeric results.
//
// Run:  node tests/differential/auto_route_test.mjs

import { spawnSync } from "node:child_process";
import { promises as fs, readFileSync, writeFileSync } from "node:fs";
import path from "node:path";
import { fileURLToPath, pathToFileURL } from "node:url";
import { test } from "node:test";
import assert from "node:assert/strict";

const __filename = fileURLToPath(import.meta.url);
const __dirname = path.dirname(__filename);
const REPO_ROOT = path.resolve(__dirname, "..", "..");
const PYTHS_BIN = path.join(REPO_ROOT, "target", "release",
    process.platform === "win32" ? "pyths.exe" : "pyths");
const FIXTURE_PS = path.join(__dirname, "auto_route_demo.ps");
const FIXTURE_JS = FIXTURE_PS.replace(/\.ps$/, ".js");
const FIXTURE_WASM = FIXTURE_PS.replace(/\.ps$/, ".wasm");
const FIXTURE_GLUE = FIXTURE_PS.replace(/\.ps$/, ".glue.js");

// Compile fresh — we want to test the current codegen, not a stale artifact.
const compile = spawnSync(PYTHS_BIN, [
    "compile", "--target", "js+wasm", "--verbose", FIXTURE_PS
], {
    encoding: "utf8",
    env: { ...process.env, PYTHS_NO_CACHE: "1" },
});
if (compile.status !== 0) {
    console.error(compile.stderr);
    process.exit(1);
}

const verboseOut = (compile.stderr + compile.stdout);

test("WASM artifact contains both numeric exports", async () => {
    const bytes = readFileSync(FIXTURE_WASM);
    const mod = await WebAssembly.compile(bytes);
    const exports = WebAssembly.Module.exports(mod).map(e => e.name).sort();
    assert.ok(exports.includes("fibonacci"), `fibonacci missing: ${exports}`);
    assert.ok(exports.includes("prime_count"), `prime_count missing: ${exports}`);
});

test("WASM artifact does NOT contain DOM/string functions", async () => {
    const bytes = readFileSync(FIXTURE_WASM);
    const mod = await WebAssembly.compile(bytes);
    const exports = WebAssembly.Module.exports(mod).map(e => e.name);
    assert.ok(!exports.includes("Greeting"),
        `Greeting (decorator @component) should not be in WASM: ${exports}`);
    assert.ok(!exports.includes("format_result"),
        `format_result (untyped string param) should not be in WASM: ${exports}`);
});

test("verbose output reports the routing decisions", () => {
    // Sanity: the compiler should have logged the routing choices.
    assert.ok(verboseOut.includes("WASM: fibonacci"),
        "verbose missing 'WASM: fibonacci': " + verboseOut.slice(-500));
    assert.ok(verboseOut.includes("WASM: prime_count"),
        "verbose missing 'WASM: prime_count': " + verboseOut.slice(-500));
    assert.ok(verboseOut.includes("Skipped Greeting"),
        "verbose missing 'Skipped Greeting': " + verboseOut.slice(-500));
    assert.ok(verboseOut.includes("Skipped format_result"),
        "verbose missing 'Skipped format_result': " + verboseOut.slice(-500));
});

test("JS artifact keeps DOM and string functions inline", () => {
    const js = readFileSync(FIXTURE_JS, "utf8");
    assert.ok(js.includes("function format_result("),
        `format_result inline missing: ${js}`);
    assert.ok(js.includes("export function Greeting("),
        `Greeting inline missing: ${js}`);
});

test("JS artifact imports + re-exports WASM functions from glue", () => {
    const js = readFileSync(FIXTURE_JS, "utf8");
    // Both the local import (so component code can reference the WASM
    // function) and the public re-export must appear.
    assert.ok(js.includes("import { fibonacci, prime_count } from"),
        `local import from glue missing: ${js}`);
    assert.ok(js.includes("export { fibonacci, prime_count };"),
        `public re-export missing: ${js}`);
    assert.ok(js.includes(".glue.js"),
        `import should reference glue file: ${js}`);
});

test("Glue module loads WASM and wraps with type marshalling", () => {
    const glue = readFileSync(FIXTURE_GLUE, "utf8");
    assert.ok(glue.includes("WebAssembly.instantiateStreaming") ||
              glue.includes("WebAssembly.instantiate"),
        `glue lacks WebAssembly init: ${glue}`);
    assert.ok(glue.includes("BigInt(") && glue.includes("Number("),
        `glue lacks i64 type marshalling: ${glue}`);
});

// Set up the test stubs once, before any test imports. Both end-to-end
// tests share these stubs because Node caches dynamic imports by URL.
const reactStubPath = path.join(__dirname, ".scratch_react_stub.mjs");
const runtimeReactStubPath = path.join(__dirname, ".scratch_runtime_react_stub.mjs");
await fs.mkdir(path.dirname(reactStubPath), { recursive: true });
writeFileSync(reactStubPath, `
let __captured = [];
export function createElement(tag, props, ...children) {
    __captured.push({ tag, children });
    return { tag, children };
}
export function __getCaptured() { return __captured; }
export function __resetCaptured() { __captured = []; }
export const Fragment = null;
`);
writeFileSync(runtimeReactStubPath,
    `export function component(fn) { return fn; }\n`);

const reactUrl = pathToFileURL(reactStubPath).href;
const runtimeReactUrl = pathToFileURL(runtimeReactStubPath).href;
// A4: format_result's plain (no-format-spec) f-string interpolation now
// routes through pyStr (see crates/pyths_codegen_js/src/emit.rs's
// FStringPart::Expr case), which requires importing pyStr from bare
// "pyths-runtime" — a specifier this fixture didn't need pre-A4. Rewire
// it the same way tests/differential/run.mjs does, so this test isn't
// coupled to whether the fixture happens to need runtime helpers.
const runtimeUrl = pathToFileURL(path.resolve(REPO_ROOT, "runtime", "src", "index.js")).href;

// Rewire the fixture's bare specifiers to absolute file:// URLs so
// Node's loader can resolve them without a package.json or import map.
// This exercises the same compiled .js the browser sees — only the
// import targets change.
const orig = readFileSync(FIXTURE_JS, "utf8");
const rewired = orig
    .replace(/from\s+["']react["']/g,                `from "${reactUrl}"`)
    .replace(/from\s+["']pyths-runtime\/react["']/g, `from "${runtimeReactUrl}"`)
    .replace(/from\s+["']pyths-runtime["']/g,        `from "${runtimeUrl}"`);
const mjsPath = path.join(__dirname, ".scratch_auto_route.run.mjs");
writeFileSync(mjsPath, rewired);

// #358: the glue now embeds the exact JS twins of the WASM functions, which
// import arbitrary-precision arithmetic helpers (pyAdd/pyMod/pyMul) from bare
// "pyths-runtime". The rewired main module imports the glue by its on-disk
// relative path, so rewire the glue's specifier in place too — otherwise
// Node cannot resolve the twin's runtime import.
writeFileSync(FIXTURE_GLUE, readFileSync(FIXTURE_GLUE, "utf8")
    .replace(/from\s+["']pyths-runtime["']/g, `from "${runtimeUrl}"`));

const fixtureMod = await import(pathToFileURL(mjsPath).href);
const reactStub = await import(reactUrl);

test("end-to-end: importing the JS module calls WASM and returns correct results", () => {
    // fibonacci(20): with the loop semantics in the fixture
    // (a starts at 0, b at 1; runs n iterations), a = fib(n) for the
    // standard 0,1,1,2,3,5,... sequence — fib(20) = 6765.
    assert.equal(fixtureMod.fibonacci(20), 6765, "fibonacci(20) via WASM");

    // prime_count(100): there are 25 primes less than 100.
    assert.equal(fixtureMod.prime_count(100), 25, "prime_count(100) via WASM");

    // Round-trip more values to confirm the WASM module isn't returning
    // a cached result.
    assert.equal(fixtureMod.fibonacci(10), 55, "fibonacci(10)");
    assert.equal(fixtureMod.fibonacci(0), 0,  "fibonacci(0)");
    assert.equal(fixtureMod.prime_count(20), 8, "prime_count(20)"); // 2,3,5,7,11,13,17,19
});

test("end-to-end: Greeting component calls into WASM at render time", () => {
    // Greeting() invokes fibonacci(20) and prime_count(100) in its
    // body and feeds the results into format_result (a JS function),
    // proving the JS→WASM call path works inside React-style render
    // code, not just on standalone exports.
    reactStub.__resetCaptured();
    const tree = fixtureMod.Greeting();
    const flat = JSON.stringify(reactStub.__getCaptured());
    assert.ok(flat.includes("fib(20) = 6765"),
        "Greeting should embed WASM-computed fib(20)=6765: " + flat);
    assert.ok(flat.includes("primes < 100 = 25"),
        "Greeting should embed WASM-computed primes<100=25: " + flat);
    assert.equal(tree.tag, "div", "outer createElement is div");
});
