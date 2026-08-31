// #358 i64-boundary differential — the committed regression guard for the
// "WASM path silently wraps int arithmetic at i64" miscompile.
//
// boundary.ps is pure typed-int arithmetic, so the auto-router sends every
// function to WASM. We run the SAME source three ways and byte-compare:
//
//   1. CPython               — arbitrary-precision ground truth,
//   2. compiled JS           — `pyths compile` (BigInt-exact by construction),
//   3. compiled JS+WASM      — `pyths compile --target js+wasm`, where the
//                              checked ops + `__ovf` JS-twin fallback (#358)
//                              must reproduce (1) exactly for products, sums,
//                              shifts, pow, floordiv, mod, neg and abs that
//                              cross the i64 boundary.
//
// Pre-fix, run #3 printed wrapped values (e.g. 4294967296 instead of
// 18446744078004518912) and this harness failed; post-fix all three agree.
// The js+wasm run also asserts each function actually landed in the .wasm
// artifact — otherwise "WASM correctness" would silently degrade to plain JS.
//
// Run:  node tests/differential/i64_boundary/run.mjs

import { spawnSync } from "node:child_process";
import { promises as fs, readFileSync, writeFileSync } from "node:fs";
import path from "node:path";
import { fileURLToPath, pathToFileURL } from "node:url";
import { ORACLE_BIN, ORACLE_DISPLAY, oracleArgs } from "../oracle_python.mjs";

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const REPO_ROOT = path.resolve(__dirname, "..", "..", "..");
const PYTHS_BIN = process.env.PYTHS_BIN ?? path.join(
    REPO_ROOT, "target", "release",
    process.platform === "win32" ? "pyths.exe" : "pyths");
const RUNTIME_INDEX = path.join(REPO_ROOT, "runtime", "src", "index.js");
const SCRATCH = path.join(__dirname, ".scratch");
const FIXTURE = path.join(__dirname, "boundary.ps");
await fs.mkdir(SCRATCH, { recursive: true });

// The functions that MUST route to WASM (asserted present in the .wasm).
const EXPECTED_WASM = [
    "prod", "summ", "diff", "shl", "shr", "powr", "modr", "fdiv",
    "negr", "absr", "chain",
];

let pythonOk = true;
try {
    const r = spawnSync(ORACLE_BIN, oracleArgs(["--version"]), { encoding: "utf8" });
    if (r.status !== 0) pythonOk = false;
} catch { pythonOk = false; }
if (!pythonOk) {
    console.log(`[i64] oracle CPython (${ORACLE_DISPLAY}) not runnable — skipping`);
    process.exit(0);
}

function rewireImports(js) {
    const runtimeUrl = pathToFileURL(RUNTIME_INDEX).href;
    return js.replace(/from\s+["']pyths-runtime["']/g, `from "${runtimeUrl}"`);
}

function runPython(file) {
    const r = spawnSync(ORACLE_BIN, oracleArgs([file]), { encoding: "utf8" });
    if (r.status !== 0) throw new Error(`python failed: ${r.stderr}`);
    return r.stdout.replace(/\r?\n$/, "");
}

async function assertInWasm(wasmPath) {
    const bytes = readFileSync(wasmPath);
    const mod = await WebAssembly.compile(bytes);
    const exports = WebAssembly.Module.exports(mod).map((e) => e.name);
    for (const name of EXPECTED_WASM) {
        if (!exports.includes(name)) {
            throw new Error(
                `function '${name}' did NOT route to WASM (exports: ${exports.join(",")}) `
                + `— the differential would silently degrade to plain JS`);
        }
    }
}

async function runCompiled(target) {
    const outJs = path.join(SCRATCH, `boundary.${target.replace("+", "_")}.js`);
    const compile = spawnSync(PYTHS_BIN, [
        "compile", FIXTURE, "--target", target, "-o", outJs, "--verbose",
    ], { encoding: "utf8", env: { ...process.env, PYTHS_NO_CACHE: "1" } });
    if (compile.status !== 0) {
        throw new Error(`pyths compile (${target}) failed: ${compile.stderr}`);
    }
    if (target === "js+wasm") {
        await assertInWasm(outJs.replace(/\.js$/, ".wasm"));
    }
    const mjs = outJs.replace(/\.js$/, ".run.mjs");
    writeFileSync(mjs, rewireImports(readFileSync(outJs, "utf8")), "utf8");
    // The js+wasm glue embeds exact JS twins importing the runtime — rewire
    // the glue's specifier in place too.
    const glue = outJs.replace(/\.js$/, ".glue.js");
    try {
        writeFileSync(glue, rewireImports(readFileSync(glue, "utf8")), "utf8");
    } catch {}
    const node = spawnSync("node", [mjs], { encoding: "utf8", cwd: SCRATCH });
    if (node.status !== 0) throw new Error(`node (${target}) failed: ${node.stderr}`);
    return node.stdout.replace(/\r?\n$/, "");
}

const py = runPython(FIXTURE);
const js = await runCompiled("js");
const wasm = await runCompiled("js+wasm");

const pyLines = py.split(/\r?\n/);
const jsLines = js.split(/\r?\n/);
const wasmLines = wasm.split(/\r?\n/);

let fail = 0;
const n = Math.max(pyLines.length, jsLines.length, wasmLines.length);
for (let i = 0; i < n; i++) {
    const p = pyLines[i], j = jsLines[i], w = wasmLines[i];
    if (p !== j || p !== w) {
        fail++;
        console.log(`  line ${i + 1} MISMATCH:`);
        console.log(`    cpython:  ${JSON.stringify(p)}`);
        console.log(`    js:       ${JSON.stringify(j)}`);
        console.log(`    js+wasm:  ${JSON.stringify(w)}`);
    }
}

const total = pyLines.length;
console.log(`[i64] ${total - fail} pass / ${total} lines (cpython vs JS vs forced-WASM)`);
if (fail > 0) {
    console.error(`[i64] ${fail} mismatch(es) — #358 regression`);
    process.exit(1);
}
