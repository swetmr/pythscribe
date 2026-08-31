// Differential test corpus runner. For each entry in
// tests/differential/cpython_corpus.json PLUS every standing corpus in
// tests/differential/corpus.d/*.json (E7: the blindspot probe corpus and
// future class-guard corpora), runs the same expression through CPython
// and PythScribe and asserts outputs match.
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
import { ORACLE_BIN, ORACLE_DISPLAY, oracleArgs } from "./oracle_python.mjs";

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

// E7 multi-corpus loader, r2 (review blocker 4): the main corpus + every
// corpus.d arm PINNED by corpus.d/MANIFEST.json — the corpus.d RATCHET.
// NOTHING about corpus.d is best-effort: a missing directory, a missing
// manifest, a read/parse error, a corpus file absent from the manifest, a
// row-count drift, an unlisted `_skip` row, or a stale listed skip ALL fail
// RED. A standing corpus committed today cannot silently disappear or
// shrink tomorrow (r1 swallowed every corpus.d read error as "no dir" and
// let arbitrary `_skip` shrink the denominator).
const CORPUS_D = path.join(__dirname, "corpus.d");
const MANIFEST_PATH = path.join(CORPUS_D, "MANIFEST.json");
function corpusFail(msg) {
    console.error(`[diff] corpus.d ratchet: ${msg}`);
    process.exit(1);
}
let manifest;
try {
    manifest = JSON.parse(await fs.readFile(MANIFEST_PATH, "utf8"));
} catch (e) {
    corpusFail(`cannot read ${MANIFEST_PATH} (${e.message}) — the manifest is REQUIRED`);
}
const manifestCorpora = manifest.corpora ?? corpusFail("MANIFEST.json has no `corpora` object");

// Every .json in corpus.d (other than the manifest) must be listed, and
// every listed corpus must exist — both directions of the ratchet.
const onDisk = (await fs.readdir(CORPUS_D)).filter(n => n.endsWith(".json") && n !== "MANIFEST.json");
for (const f of onDisk) {
    if (!(f in manifestCorpora)) corpusFail(`corpus.d/${f} is not listed in MANIFEST.json`);
}
for (const f of Object.keys(manifestCorpora)) {
    if (!onDisk.includes(f)) corpusFail(`MANIFEST.json lists corpus.d/${f}, which does not exist`);
}

const corpusFiles = [["cpython_corpus.json", CORPUS_PATH, null]];
for (const f of Object.keys(manifestCorpora).sort()) {
    corpusFiles.push([`corpus.d/${f}`, path.join(CORPUS_D, f), manifestCorpora[f]]);
}

const corpus = [];
const seenIds = new Set();
const perCorpus = new Map();
for (const [label, file, pin] of corpusFiles) {
    const entries = JSON.parse(await fs.readFile(file, "utf8"));
    if (!Array.isArray(entries) || entries.length === 0) {
        corpusFail(`${label} is empty or not an array`);
    }
    if (pin && entries.length !== pin.rows) {
        corpusFail(`${label} has ${entries.length} rows, MANIFEST.json pins ${pin.rows} — `
            + `update the manifest ONLY with a deliberate corpus change`);
    }
    const allowedSkips = new Set(pin?.skips ?? []);
    const seenSkips = new Set();
    for (const e of entries) {
        if (!e.id || seenIds.has(e.id)) {
            corpusFail(`missing/duplicate id ${JSON.stringify(e.id)} in ${label}`);
        }
        if (e._skip) {
            if (!allowedSkips.has(e.id)) {
                corpusFail(`${label} row ${e.id} carries _skip but is not in the manifest's `
                    + `skip list — a skip is a ratchet edit, not a row-local annotation`);
            }
            seenSkips.add(e.id);
        }
        seenIds.add(e.id);
        e._corpus = label;
        corpus.push(e);
    }
    for (const s of allowedSkips) {
        if (!seenSkips.has(s)) {
            corpusFail(`${label}: manifest skip ${s} is stale (row missing or no longer skipped) — remove it`);
        }
    }
    perCorpus.set(label, { pass: 0, fail: 0, total: entries.length });
}

// The CPython differential ORACLE — resolved by the ONE shared module
// (oracle_python.mjs; policy in docs/python-oracle-policy.md). CI installs
// the pinned version via actions/setup-python so plain `python` there IS the
// oracle; locally set PYTHS_ORACLE_PYTHON (e.g. "py -3.14").
//
// E7 r3 (review blocker): this availability check runs AFTER the corpus.d
// manifest ratchet above, deliberately — the corpus STRUCTURE (manifest
// presence, exact row counts, allowed-skip sets, both-direction file
// listing) is validated on EVERY invocation, oracle or no oracle. Only the
// differential EXECUTION may skip when the oracle is absent; a structural
// drift must fail RED even on a machine with no CPython 3.14 (r2 exited 0
// here first, so a mutated manifest stayed green oracle-less).
let pythonOk = true;
try { execFileSync(ORACLE_BIN, oracleArgs(["-c", "print(1)"]), { stdio: "pipe" }); }
catch { pythonOk = false; }

if (!pythonOk) {
    console.log(`[diff] corpus.d ratchet validated (${corpus.length} rows across ${corpusFiles.length} corpora)`);
    console.log(`[diff] oracle CPython (${ORACLE_DISPLAY}) not runnable — skipping differential EXECUTION`);
    process.exit(0);
}

function runPython(setup, expr) {
    // Newline-separated setup; CPython accepts both ; and \n. We use \n
    // so the same string also parses in PythScribe (which doesn't have
    // ; statement separators).
    const code = `${setup ? setup + "\n" : ""}import sys\nsys.stdout.write(repr(${expr}))`;
    // PYTHONIOENCODING: piped CPython stdout defaults to the ANSI codepage on
    // Windows (cp1252), so any expected value containing an astral char (the
    // wave-15/19 lstrip/rstrip/replace astral pins) crashed CPython with
    // UnicodeEncodeError before the comparison even ran. Node reads utf8.
    const r = spawnSync(ORACLE_BIN, oracleArgs(["-c", code]), {
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

// E3 r2 (codex): per-row scratch filenames must be CASE-SAFE — Windows'
// case-insensitive FS made case-only-differing row ids (pf_e/pf_E,
// pf_hex/pf_HEX) OVERWRITE each other's .ps/.js/.run.mjs artifacts, silently
// racing/losing rows. Append a short case-sensitive hash of the RAW id so
// distinct ids always map to distinct files on every FS.
function caseSafeName(id) {
    let h = 5381;
    for (let i = 0; i < id.length; i++) h = ((h * 33) ^ id.charCodeAt(i)) >>> 0;
    return `${id}_${h.toString(16).padStart(8, "0")}`;
}

function runPythscribe(id, setup, expr) {
    const psSrc = `${setup ? setup + "\n" : ""}print(repr(${expr}))\n`;
    const psPath = path.join(SCRATCH, `${caseSafeName(id)}.ps`);
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
    const mjsPath = path.join(SCRATCH, `${caseSafeName(id)}.run.mjs`);
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

let pass = 0, fail = 0, skipped = 0;
let executed = 0;
const skips = [];
const failures = [];
for (const entry of corpus) {
    const setup = entry._setup || "";
    const expr = entry.expr;
    const stat = perCorpus.get(entry._corpus);
    // Documented exclusion (visible, never silent): a row may carry
    // `_skip: "<reason + issue ref>"` while its class fix is pending.
    if (entry._skip) {
        skipped++; stat.total--;
        skips.push(`${entry.id}: ${entry._skip}`);
        continue;
    }
    executed++;
    let pyOut, psOut;
    try { pyOut = runPython(setup, expr); }
    catch (e) { failures.push({ id: entry.id, why: `python: ${e.message.split("\n")[0]}` }); fail++; stat.fail++; continue; }
    try { psOut = runPythscribe(entry.id, setup, expr); }
    catch (e) { failures.push({ id: entry.id, why: `pythscribe: ${e.message}` }); fail++; stat.fail++; continue; }
    if (psOut === pyOut) { pass++; stat.pass++; }
    else { fail++; stat.fail++; failures.push({ id: entry.id, expr, py: pyOut, ps: psOut }); }
}

// Harness-integrity: every loaded row was executed and scored, or is a
// VISIBLE documented skip (E7 sub-part 4).
if (executed + skipped !== corpus.length || pass + fail !== executed) {
    console.error(`[diff] HARNESS INTEGRITY FAILURE: loaded=${corpus.length} executed=${executed} skipped=${skipped} scored=${pass + fail}`);
    process.exit(1);
}

console.log(`[diff] corpus: ${pass} pass / ${pass + fail} total`);
for (const [label, s] of perCorpus) {
    console.log(`[diff]   ${label}: ${s.pass}/${s.total}`);
}
for (const s of skips) console.log(`[diff]   skip (documented): ${s}`);
for (const f of failures) {
    if (f.why) console.log(`  - ${f.id}: ${f.why}`);
    else {
        console.log(`  - ${f.id}: ${f.expr}`);
        console.log(`      cpython:    ${JSON.stringify(f.py)}`);
        console.log(`      pythscribe: ${JSON.stringify(f.ps)}`);
    }
}
process.exit(fail === 0 ? 0 : 1);
