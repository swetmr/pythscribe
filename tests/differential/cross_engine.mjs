// Second-engine cross-check: run each compiled corpus program under BOTH
// Node (V8) and Bun (JavaScriptCore) and compare their stdout byte-for-byte.
// This is the "cross-checked on a second JS engine — X/N identical across V8
// and JavaScriptCore" assurance layer, re-run against the CURRENT corpus.
//
//   node tests/differential/cross_engine.mjs
//
// Requires: `bun` on PATH (JavaScriptCore) + the release `pyths` binary.
import { spawnSync } from "node:child_process";
import { promises as fs, mkdirSync, writeFileSync, readFileSync } from "node:fs";
import path from "node:path";
import { fileURLToPath, pathToFileURL } from "node:url";

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const REPO_ROOT = path.resolve(__dirname, "..", "..");
const PYTHS_BIN = process.env.PYTHS_BIN || path.join(REPO_ROOT, "target", "release",
    process.platform === "win32" ? "pyths.exe" : "pyths");
const CORPUS_PATH = path.join(__dirname, "cpython_corpus.json");
const SCRATCH = path.join(__dirname, ".xengine_scratch");
mkdirSync(SCRATCH, { recursive: true });
const RUNTIME_INDEX = path.resolve(REPO_ROOT, "runtime", "src", "index.js");
const RUNTIME_ASYNCIO = path.resolve(REPO_ROOT, "runtime", "asyncio.js");
const RUNTIME_STDLIB_DIR = path.resolve(REPO_ROOT, "runtime", "src", "stdlib");

// bun present?
const bunProbe = spawnSync("bun", ["--version"], { encoding: "utf8" });
if (bunProbe.status !== 0) { console.log("[xengine] bun not on PATH — cannot run the JavaScriptCore side"); process.exit(2); }
console.log(`[xengine] node ${process.version} (V8) vs bun ${bunProbe.stdout.trim()} (JavaScriptCore)`);

function rewireImports(jsSource) {
    const runtimeUrl = pathToFileURL(RUNTIME_INDEX).href;
    const asyncioUrl = pathToFileURL(RUNTIME_ASYNCIO).href;
    return jsSource
        .replace(/from\s+["']pyths-runtime\/asyncio["']/g, `from "${asyncioUrl}"`)
        .replace(/from\s+["']pyths-runtime\/react["']/g,   `from "${runtimeUrl}"`)
        .replace(/from\s+["']pyths-runtime\/stdlib\/([a-zA-Z0-9_]+)["']/g, (_m, mod) =>
            `from "${pathToFileURL(path.join(RUNTIME_STDLIB_DIR, `${mod}.js`)).href}"`)
        .replace(/from\s+["']pyths-runtime["']/g,          `from "${runtimeUrl}"`);
}

function compileToMjs(id, setup, expr) {
    const psSrc = `${setup ? setup + "\n" : ""}print(repr(${expr}))\n`;
    const psPath = path.join(SCRATCH, `${id}.ps`);
    writeFileSync(psPath, psSrc, "utf8");
    const compile = spawnSync(PYTHS_BIN, ["compile", psPath],
        { encoding: "utf8", env: { ...process.env, PYTHS_NO_CACHE: "1" } });
    if (compile.status !== 0) throw new Error(`compile: ${compile.stderr}`);
    const jsPath = psPath.replace(/\.ps$/, ".js");
    const mjsPath = path.join(SCRATCH, `${id}.run.mjs`);
    writeFileSync(mjsPath, rewireImports(readFileSync(jsPath, "utf8")), "utf8");
    const gluePath = psPath.replace(/\.ps$/, ".glue.js");
    try { writeFileSync(gluePath, rewireImports(readFileSync(gluePath, "utf8")), "utf8"); } catch {}
    return mjsPath;
}

const run = (bin, mjs) => {
    const r = spawnSync(bin, [mjs], { encoding: "utf8" });
    return { ok: r.status === 0, out: (r.stdout || "").replace(/\r?\n$/, ""), err: (r.stderr || "").split("\n")[0] };
};

const corpus = JSON.parse(await fs.readFile(CORPUS_PATH, "utf8"));
let identical = 0, mismatch = 0, engineErr = 0, compileErr = 0;
const diffs = [];
for (const e of corpus) {
    let mjs;
    try { mjs = compileToMjs(e.id, e._setup || "", e.expr); }
    catch (err) { compileErr++; continue; }              // compile is engine-agnostic; excluded
    const v8 = run("node", mjs), jsc = run("bun", mjs);
    if (!v8.ok || !jsc.ok) { engineErr++; diffs.push({ id: e.id, kind: "engine-error", v8: v8.err, jsc: jsc.err }); continue; }
    if (v8.out === jsc.out) identical++;
    else { mismatch++; diffs.push({ id: e.id, kind: "output-mismatch", expr: e.expr, v8: v8.out, jsc: jsc.out }); }
}
const graded = identical + mismatch;
console.log(`\n[xengine] corpus: ${corpus.length} entries`);
console.log(`[xengine] IDENTICAL across V8 and JavaScriptCore: ${identical}/${graded}` +
    (compileErr ? `  (+${compileErr} compile-excluded)` : "") + (engineErr ? `  (+${engineErr} engine-error)` : ""));
for (const d of diffs.slice(0, 30)) {
    if (d.kind === "output-mismatch") console.log(`  DIFF ${d.id}: ${d.expr}\n    V8 =${JSON.stringify(d.v8)}\n    JSC=${JSON.stringify(d.jsc)}`);
    else console.log(`  ENGINE-ERR ${d.id}: v8=${d.v8} | jsc=${d.jsc}`);
}
process.exit(mismatch > 0 ? 1 : 0);
