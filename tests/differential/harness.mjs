// Shared differential-testing harness: CPython (oracle) vs compiled-and-run
// PythScribe, byte-for-byte stdout comparison. Extracted from the pattern in
// `run.mjs` so `gen_identifier_cases.mjs` (S1) and `fuzz_gen.mjs` (S2) reuse
// the exact same invocation strategy without duplicating logic. `run.mjs`
// itself is left untouched (its own copy of this logic still works fine).
//
//   1. CPython:    `python -c "<setup>; sys.stdout.write(repr(<expr>))"`
//   2. PythScribe: emit a tiny .ps file → compile (PYTHS_NO_CACHE=1) → rewrite
//                  bare-specifier imports to absolute file:// URLs → run via Node

import { execFileSync, spawnSync } from "node:child_process";
import { mkdirSync, writeFileSync, readFileSync } from "node:fs";
import path from "node:path";
import { fileURLToPath, pathToFileURL } from "node:url";

const __filename = fileURLToPath(import.meta.url);
const __dirname = path.dirname(__filename);
export const REPO_ROOT = path.resolve(__dirname, "..", "..");
export const SCRATCH = path.join(REPO_ROOT, "target", "differential");
export const PYTHS_BIN = path.join(REPO_ROOT, "target", "release",
    process.platform === "win32" ? "pyths.exe" : "pyths");
const RUNTIME_INDEX = path.resolve(REPO_ROOT, "runtime", "src", "index.js");
const RUNTIME_ASYNCIO = path.resolve(REPO_ROOT, "runtime", "asyncio.js");
const RUNTIME_STDLIB_DIR = path.resolve(REPO_ROOT, "runtime", "src", "stdlib");

mkdirSync(SCRATCH, { recursive: true });

export function checkPythonAvailable() {
    try { execFileSync("python", ["-c", "print(1)"], { stdio: "pipe" }); return true; }
    catch { return false; }
}

export function runPython(setup, expr) {
    const code = `${setup ? setup + "\n" : ""}import sys\nsys.stdout.write(repr(${expr}))`;
    const r = spawnSync("python", ["-c", code], { encoding: "utf8" });
    if (r.status !== 0) throw new Error(`python failed: ${r.stderr}`);
    return r.stdout;
}

/** Rewrite bare-specifier imports in compiled JS to absolute file:// URLs
 *  so Node can load the .js without a package.json or import map. */
export function rewireImports(jsSource) {
    const runtimeUrl = pathToFileURL(RUNTIME_INDEX).href;
    const asyncioUrl = pathToFileURL(RUNTIME_ASYNCIO).href;
    return jsSource
        .replace(/from\s+["']pyths-runtime\/asyncio["']/g, `from "${asyncioUrl}"`)
        .replace(/from\s+["']pyths-runtime\/react["']/g,   `from "${runtimeUrl}"`)
        .replace(/from\s+["']pyths-runtime\/stdlib\/([a-zA-Z0-9_]+)["']/g, (_m, mod) => {
            const url = pathToFileURL(path.join(RUNTIME_STDLIB_DIR, `${mod}.js`)).href;
            return `from "${url}"`;
        })
        .replace(/from\s+["']pyths-runtime["']/g,          `from "${runtimeUrl}"`);
}

export function runPythscribe(id, setup, expr) {
    const psSrc = `${setup ? setup + "\n" : ""}print(repr(${expr}))\n`;
    const psPath = path.join(SCRATCH, `${id}.ps`);
    writeFileSync(psPath, psSrc, "utf8");
    const compile = spawnSync(PYTHS_BIN, ["compile", psPath], {
        encoding: "utf8",
        env: { ...process.env, PYTHS_NO_CACHE: "1" },
    });
    if (compile.status !== 0) {
        throw new Error(`pyths compile failed: ${compile.stderr || compile.stdout}`);
    }
    const jsPath = psPath.replace(/\.ps$/, ".js");
    const rewired = rewireImports(readFileSync(jsPath, "utf8"));
    const mjsPath = path.join(SCRATCH, `${id}.run.mjs`);
    writeFileSync(mjsPath, rewired, "utf8");
    const node = spawnSync("node", [mjsPath], { encoding: "utf8" });
    if (node.status !== 0) {
        throw new Error(`node failed: ${node.stderr}`);
    }
    // print() → console.log() appends a trailing newline.
    return node.stdout.replace(/\r?\n$/, "");
}

/** Run one differential case: { id, _setup?, expr }. Returns
 *  { pass, py, ps, why } — `why` set only on a harness-level error
 *  (python/pythscribe invocation failure), not a genuine mismatch. */
export function runCase({ id, _setup, expr }) {
    const setup = _setup || "";
    let pyOut, psOut;
    try { pyOut = runPython(setup, expr); }
    catch (e) { return { pass: false, why: `python: ${e.message.split("\n")[0]}` }; }
    try { psOut = runPythscribe(id, setup, expr); }
    catch (e) { return { pass: false, why: `pythscribe: ${e.message}` }; }
    return { pass: psOut === pyOut, py: pyOut, ps: psOut };
}
