// Livermore-kernel differential (testing gap-closure §7.1, after Axon's
// LFK usage): the 24 Livermore Fortran Kernels ported to `.ps`, each run
// three ways and byte-compared:
//
//   1. CPython               — the semantic ground truth (same source),
//   2. compiled JS           — `pyths compile` (default JS routing),
//   3. compiled JS+WASM      — `pyths compile --target js+wasm`, where the
//                              auto-router must send the kernel to WASM.
//
// This is the compute-path analogue of ../run.mjs: it exercises numeric
// loop nests (loop-carried dependences, reductions, flattened 2-D index
// arithmetic, int-from-float indexing) at kernel scale — the workload the
// WASM path exists for. For the js+wasm run the harness ALSO asserts the
// kernel function actually landed in the .wasm artifact (otherwise the
// "WASM correctness" claim silently degrades to plain JS).
//
// Determinism note: kernels use only +,-,*,/,%,//,comparisons and int()
// — IEEE-754-deterministic on every backend. k22 carries its own exp
// series because libm exp() is not correctly rounded (see file comment).
//
// Run:  node tests/differential/livermore/run.mjs

import { spawnSync } from "node:child_process";
import { promises as fs, readFileSync, writeFileSync } from "node:fs";
import path from "node:path";
import { fileURLToPath, pathToFileURL } from "node:url";

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const REPO_ROOT = path.resolve(__dirname, "..", "..", "..");
const PYTHS_BIN = process.env.PYTHS_BIN ?? path.join(
    REPO_ROOT, "target", "release",
    process.platform === "win32" ? "pyths.exe" : "pyths");
const RUNTIME_INDEX = path.join(REPO_ROOT, "runtime", "src", "index.js");
const SCRATCH = path.join(__dirname, ".scratch");
await fs.mkdir(SCRATCH, { recursive: true });

let pythonOk = true;
try {
    const r = spawnSync("python", ["--version"], { encoding: "utf8" });
    if (r.status !== 0) pythonOk = false;
} catch { pythonOk = false; }
if (!pythonOk) {
    console.log("[livermore] python not on PATH — skipping");
    process.exit(0);
}

function rewireImports(js) {
    const runtimeUrl = pathToFileURL(RUNTIME_INDEX).href;
    return js.replace(/from\s+["']pyths-runtime["']/g, `from "${runtimeUrl}"`);
}

function runPython(file) {
    const r = spawnSync("python", [file], { encoding: "utf8" });
    if (r.status !== 0) throw new Error(`python failed: ${r.stderr}`);
    return r.stdout.replace(/\r?\n$/, "");
}

function runCompiled(file, id, target) {
    const outJs = path.join(SCRATCH, `${id}.${target.replace("+", "_")}.js`);
    const compile = spawnSync(PYTHS_BIN, [
        "compile", file, "--target", target, "-o", outJs, "--verbose",
    ], { encoding: "utf8", env: { ...process.env, PYTHS_NO_CACHE: "1" } });
    if (compile.status !== 0) {
        throw new Error(`pyths compile (${target}) failed: ${compile.stderr}`);
    }
    if (target === "js+wasm") {
        // The kernel must actually route to WASM — the point of this run.
        const verbose = compile.stderr + compile.stdout;
        const kernelName = id.replace(/^k0?/, "k");
        if (!/WASM: k\d+/.test(verbose)) {
            throw new Error(
                `auto-router did not send the ${kernelName} kernel to WASM:\n${verbose}`);
        }
    }
    const rewired = rewireImports(readFileSync(outJs, "utf8"));
    const mjs = outJs.replace(/\.js$/, ".run.mjs");
    writeFileSync(mjs, rewired, "utf8");
    // #358: the js+wasm glue now embeds exact JS twins which import the
    // runtime — rewire the glue's specifier in place too.
    const glue = outJs.replace(/\.js$/, ".glue.js");
    try {
        writeFileSync(glue, rewireImports(readFileSync(glue, "utf8")), "utf8");
    } catch {}
    const node = spawnSync("node", [mjs], { encoding: "utf8", cwd: SCRATCH });
    if (node.status !== 0) throw new Error(`node (${target}) failed: ${node.stderr}`);
    return node.stdout.replace(/\r?\n$/, "");
}

const files = (await fs.readdir(__dirname))
    .filter((f) => /^k\d\d_.*\.ps$/.test(f))
    .sort();
if (files.length !== 24) {
    console.error(`[livermore] expected 24 kernels, found ${files.length}`);
    process.exit(1);
}

let pass = 0, fail = 0;
const failures = [];
for (const f of files) {
    const id = f.replace(/\.ps$/, "");
    const full = path.join(__dirname, f);
    let py, js, wasm;
    try { py = runPython(full); }
    catch (e) { failures.push({ id, why: `python: ${e.message.split("\n")[0]}` }); fail++; continue; }
    try { js = runCompiled(full, id, "js"); }
    catch (e) { failures.push({ id, why: `js: ${e.message.split("\n")[0]}` }); fail++; continue; }
    try { wasm = runCompiled(full, id, "js+wasm"); }
    catch (e) { failures.push({ id, why: `wasm: ${e.message.split("\n")[0]}` }); fail++; continue; }
    if (js === py && wasm === py) { pass++; }
    else {
        fail++;
        failures.push({ id, py, js, wasm });
    }
}

console.log(`[livermore] ${pass} pass / ${pass + fail} kernels (each = cpython vs JS vs forced-WASM)`);
for (const f of failures) {
    if (f.why) console.log(`  - ${f.id}: ${f.why}`);
    else {
        console.log(`  - ${f.id}:`);
        console.log(`      cpython: ${JSON.stringify(f.py)}`);
        console.log(`      js:      ${JSON.stringify(f.js)}`);
        console.log(`      wasm:    ${JSON.stringify(f.wasm)}`);
    }
}
process.exit(fail === 0 ? 0 : 1);
