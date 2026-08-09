// JS-interop pinned-expectation suite (Promise / async-JS semantics).
//
// CPython cannot oracle raw-Promise interop (`Promise.all`, `.then` chains,
// microtask ordering, thenables, allSettled shapes, ...), so unlike
// tests/differential/run.mjs the expected stdout is pinned IN the case file:
// tests/jsinterop/cases.json — [{ id, src, expected, note? }].
//   - `src`: full .ps module source (string, or array of lines joined by \n)
//   - `expected`: exact stdout (trailing newline normalized away)
//   - `expect_compile_error`: true → the case PASSES iff compilation fails
//     (pins grammar-level findings, e.g. "no async lambda")
//
// Compilation/rewiring mirrors tests/differential/run.mjs exactly: compile
// with the release binary (PYTHS_NO_CACHE=1), rewrite bare pyths-runtime
// imports to absolute file:// URLs, run under plain Node, byte-compare stdout.
//
// Run (from repo root):
//   node tests/jsinterop/run.mjs            # all cases
//   node tests/jsinterop/run.mjs <id> ...   # only the named cases
//   node tests/jsinterop/run.mjs --show <id>  # print actual output (probe mode)
//
// This suite is a local merge gate alongside `cargo test --workspace` and
// `node tests/differential/run.mjs`.

import { spawnSync } from "node:child_process";
import { promises as fs, mkdirSync, writeFileSync, readFileSync } from "node:fs";
import path from "node:path";
import { fileURLToPath, pathToFileURL } from "node:url";

const __filename = fileURLToPath(import.meta.url);
const __dirname = path.dirname(__filename);
const REPO_ROOT = path.resolve(__dirname, "..", "..");
const CASES_PATH = path.join(__dirname, "cases.json");
const SCRATCH = path.join(REPO_ROOT, "target", "jsinterop");
const PYTHS_BIN = path.join(REPO_ROOT, "target", "release",
    process.platform === "win32" ? "pyths.exe" : "pyths");
const RUNTIME_INDEX = path.resolve(REPO_ROOT, "runtime", "src", "index.js");
const RUNTIME_ASYNCIO = path.resolve(REPO_ROOT, "runtime", "asyncio.js");
const RUNTIME_STDLIB_DIR = path.resolve(REPO_ROOT, "runtime", "src", "stdlib");

mkdirSync(SCRATCH, { recursive: true });

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

/** Compile + run one case. Returns { ok, out?, err?, compileErr? }. */
function runCase(id, src) {
    const psPath = path.join(SCRATCH, `${id}.ps`);
    writeFileSync(psPath, src.endsWith("\n") ? src : src + "\n", "utf8");
    const compile = spawnSync(PYTHS_BIN, ["compile", psPath], {
        encoding: "utf8",
        env: { ...process.env, PYTHS_NO_CACHE: "1" },
    });
    if (compile.status !== 0) {
        return { ok: false, compileErr: compile.stderr.trim() };
    }
    const jsPath = psPath.replace(/\.ps$/, ".js");
    const rewired = rewireImports(readFileSync(jsPath, "utf8"));
    const mjsPath = path.join(SCRATCH, `${id}.run.mjs`);
    writeFileSync(mjsPath, rewired, "utf8");
    const node = spawnSync("node", [mjsPath], { encoding: "utf8" });
    if (node.status !== 0) {
        return { ok: false, err: node.stderr.trim() };
    }
    return { ok: true, out: node.stdout.replace(/\r\n/g, "\n").replace(/\n$/, "") };
}

const args = process.argv.slice(2);
const showMode = args[0] === "--show";
const filter = new Set(showMode ? args.slice(1) : args);

const cases = JSON.parse(await fs.readFile(CASES_PATH, "utf8"));
let pass = 0, fail = 0;
const failures = [];

for (const c of cases) {
    if (filter.size && !filter.has(c.id)) continue;
    const src = Array.isArray(c.src) ? c.src.join("\n") : c.src;
    const r = runCase(c.id, src);
    if (showMode) {
        console.log(`=== ${c.id} ===`);
        if (r.ok) console.log(r.out);
        else console.log(`[FAILED] ${r.compileErr || r.err}`);
        continue;
    }
    if (c.expect_compile_error) {
        if (!r.ok && r.compileErr !== undefined) { pass++; }
        else { fail++; failures.push({ id: c.id, why: "expected a compile error but compilation succeeded" }); }
        continue;
    }
    if (!r.ok) {
        fail++;
        failures.push({ id: c.id, why: r.compileErr ? `compile: ${r.compileErr}` : `runtime: ${r.err}` });
        continue;
    }
    const expected = (Array.isArray(c.expected) ? c.expected.join("\n") : c.expected).replace(/\n$/, "");
    if (r.out === expected) { pass++; }
    else { fail++; failures.push({ id: c.id, expected, actual: r.out }); }
}

console.log(`[jsinterop] ${pass} pass / ${pass + fail} total`);
for (const f of failures) {
    if (f.why) console.log(`  - ${f.id}: ${f.why.split("\n")[0]}`);
    else {
        console.log(`  - ${f.id}:`);
        console.log(`      expected: ${JSON.stringify(f.expected)}`);
        console.log(`      actual:   ${JSON.stringify(f.actual)}`);
    }
}
process.exit(fail === 0 ? 0 : 1);
