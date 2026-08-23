// Differential test corpus runner. For each entry in
// tests/differential/cpython_corpus.json, runs the same expression
// through CPython and PythScribe and asserts outputs match.
//
//   1. CPython:    `python -c "<setup>; print(repr(<expr>))"`
//   2. PythScribe: emit a tiny .ps file → compile → rewrite imports
//                  to absolute file:// URLs → run via Node
//
// Run:  node tests/differential/run.mjs

import { execFileSync, spawnSync } from "node:child_process";
import { promises as fs, mkdirSync, writeFileSync, readFileSync } from "node:fs";
import path from "node:path";
import { fileURLToPath, pathToFileURL } from "node:url";

const __filename = fileURLToPath(import.meta.url);
const __dirname = path.dirname(__filename);
const REPO_ROOT = path.resolve(__dirname, "..", "..");
const CORPUS_PATH = path.join(__dirname, "cpython_corpus.json");
const SCRATCH = path.join(REPO_ROOT, "target", "differential");
const PYTHS_BIN = process.env.PYTHS_BIN || path.join(REPO_ROOT, "target", "release",
    process.platform === "win32" ? "pyths.exe" : "pyths");
const RUNTIME_INDEX = path.resolve(REPO_ROOT, "runtime", "src", "index.js");
const RUNTIME_ASYNCIO = path.resolve(REPO_ROOT, "runtime", "asyncio.js");

mkdirSync(SCRATCH, { recursive: true });

let pythonOk = true;
try { execFileSync("python", ["-c", "print(1)"], { stdio: "pipe" }); }
catch { pythonOk = false; }

if (!pythonOk) {
    console.log("[diff] python not on PATH — skipping differential corpus");
    process.exit(0);
}

const corpus = JSON.parse(await fs.readFile(CORPUS_PATH, "utf8"));

function runPython(setup, expr) {
    // Newline-separated setup; CPython accepts both ; and \n. We use \n
    // so the same string also parses in PythScribe (which doesn't have
    // ; statement separators).
    const code = `${setup ? setup + "\n" : ""}import sys\nsys.stdout.write(repr(${expr}))`;
    // PYTHONIOENCODING: piped CPython stdout defaults to the ANSI codepage on
    // Windows (cp1252), so any expected value containing an astral char (the
    // wave-15/19 lstrip/rstrip/replace astral pins) crashed CPython with
    // UnicodeEncodeError before the comparison even ran. Node reads utf8.
    const r = spawnSync("python", ["-c", code], {
        encoding: "utf8",
        env: { ...process.env, PYTHONIOENCODING: "utf-8" },
    });
    if (r.status !== 0) throw new Error(`python failed: ${r.stderr}`);
    return r.stdout;
}

const RUNTIME_STDLIB_DIR = path.resolve(REPO_ROOT, "runtime", "src", "stdlib");

/** Rewrite bare-specifier imports in compiled JS to absolute file:// URLs
 *  so Node can load the .js without a package.json or import map. */
function rewireImports(jsSource) {
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

function runPythscribe(id, setup, expr) {
    const psSrc = `${setup ? setup + "\n" : ""}print(repr(${expr}))\n`;
    const psPath = path.join(SCRATCH, `${id}.ps`);
    writeFileSync(psPath, psSrc, "utf8");
    const compile = spawnSync(PYTHS_BIN, ["compile", psPath], {
        encoding: "utf8",
        env: { ...process.env, PYTHS_NO_CACHE: "1" },
    });
    if (compile.status !== 0) {
        throw new Error(`pyths compile failed: ${compile.stderr}`);
    }
    const jsPath = psPath.replace(/\.ps$/, ".js");
    const rewired = rewireImports(readFileSync(jsPath, "utf8"));
    const mjsPath = path.join(SCRATCH, `${id}.run.mjs`);
    writeFileSync(mjsPath, rewired, "utf8");
    // Harness gap fix: an auto-routed (WASM) program emits a co-located
    // `<id>.glue.js` whose own `pyths-runtime` imports were never rewired —
    // every WASM-routed corpus entry failed with ERR_MODULE_NOT_FOUND
    // regardless of behavior. Rewire the glue in place (the entry imports
    // it relatively, so same-dir resolution still works), and point the
    // entry's `./<id>.glue.js` import at it from the `.run.mjs` name.
    const gluePath = psPath.replace(/\.ps$/, ".glue.js");
    try {
        const glue = readFileSync(gluePath, "utf8");
        writeFileSync(gluePath, rewireImports(glue), "utf8");
    } catch { /* no glue emitted — pure-JS program */ }
    const node = spawnSync("node", [mjsPath], { encoding: "utf8" });
    if (node.status !== 0) {
        throw new Error(`node failed: ${node.stderr}`);
    }
    // print() → console.log() appends a trailing newline.
    return node.stdout.replace(/\r?\n$/, "");
}

let pass = 0, fail = 0;
const failures = [];
for (const entry of corpus) {
    const setup = entry._setup || "";
    const expr = entry.expr;
    let pyOut, psOut;
    try { pyOut = runPython(setup, expr); }
    catch (e) { failures.push({ id: entry.id, why: `python: ${e.message.split("\n")[0]}` }); fail++; continue; }
    try { psOut = runPythscribe(entry.id, setup, expr); }
    catch (e) { failures.push({ id: entry.id, why: `pythscribe: ${e.message}` }); fail++; continue; }
    if (psOut === pyOut) { pass++; }
    else { fail++; failures.push({ id: entry.id, expr, py: pyOut, ps: psOut }); }
}

console.log(`[diff] corpus: ${pass} pass / ${pass + fail} total`);
for (const f of failures) {
    if (f.why) console.log(`  - ${f.id}: ${f.why}`);
    else {
        console.log(`  - ${f.id}: ${f.expr}`);
        console.log(`      cpython:    ${JSON.stringify(f.py)}`);
        console.log(`      pythscribe: ${JSON.stringify(f.ps)}`);
    }
}
process.exit(fail === 0 ? 0 : 1);
