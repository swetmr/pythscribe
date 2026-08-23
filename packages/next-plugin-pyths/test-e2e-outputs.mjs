// End-to-end guard for security findings #2 / #6 in the Next.js loader —
// drives the REAL `pyths` compiler through the loader and asserts that nothing
// beside the user's source is created, overwritten, or deleted.
//
// Before the fix the loader let the CLI write `<stem>.js` / `.js.map` /
// `<stem>.d.ts` beside the SOURCE and then unlinked them "if they exist" in a
// `finally` block, deleting hand-written project files of the same stem (#6);
// the `.d.ps.ts` sibling was written with a bare `writeFileSync`, following
// symlinks and clobbering unmarked files (#2).
//
// Requires a `pyths` binary (repo `target/release/pyths[.exe]` or PYTHS_BIN).
//
// Run with: node --test test-e2e-outputs.mjs
import { test } from "node:test";
import assert from "node:assert";
import { existsSync, mkdtempSync, readFileSync, readdirSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import pythsLoader from "./loader.js";
import { GENERATED_MARKER, resolvePythsCommand } from "./pyths-safe.js";

const here = dirname(fileURLToPath(import.meta.url));
const repoRoot = resolve(here, "..", "..");

function findCompiler() {
    for (const rel of ["target/release/pyths.exe", "target/release/pyths",
                       "target/debug/pyths.exe", "target/debug/pyths"]) {
        const p = join(repoRoot, rel);
        if (existsSync(p)) return resolvePythsCommand({ pythsBin: p });
    }
    try {
        return resolvePythsCommand({});
    } catch {
        return null;
    }
}

const CMD = findCompiler();
const SOURCE = "def add(a: int, b: int) -> int:\n    return a + b\n";

function project(tag) {
    const d = mkdtempSync(join(tmpdir(), `pyths-next-e2e-${tag}-`));
    writeFileSync(join(d, "Counter.ps"), SOURCE);
    return d;
}

/** Minimal webpack-loader context, enough to drive `pythsLoader`. */
function runLoader(dir, { emitDts = true } = {}) {
    const result = { code: null, map: null, error: null };
    const ctx = {
        mode: "production",
        hot: false,
        resourcePath: join(dir, "Counter.ps"),
        getOptions: () => ({
            pythsBin: CMD.command,
            pythsPrefixArgs: CMD.prefixArgs,
            reactRefresh: false,
            emitDts,
        }),
        callback: (err, code, map) => {
            result.error = err;
            result.code = code;
            result.map = map;
        },
        emitError: (err) => { result.error = err; },
    };
    const ret = pythsLoader.call(ctx, SOURCE);
    if (typeof ret === "string" && result.code === null) result.code = ret;
    return result;
}

test("#6 a hand-written same-stem .js / .js.map / .d.ts survives a loader run", (t) => {
    if (!CMD) { t.skip("no pyths binary available"); return; }
    const d = project("keep");
    try {
        const handJs = "export const mine = 1; // hand-written, NOT generated\n";
        const handMap = '{"version":3,"sources":["mine"],"mappings":""}';
        const handDts = "export declare const mine: number; // hand-written\n";
        writeFileSync(join(d, "Counter.js"), handJs);
        writeFileSync(join(d, "Counter.js.map"), handMap);
        writeFileSync(join(d, "Counter.d.ts"), handDts);

        const r = runLoader(d);
        assert.equal(r.error, null, `loader errored: ${r.error && r.error.message}`);
        assert.match(r.code, /function add/, "the compile itself must still succeed");

        assert.equal(readFileSync(join(d, "Counter.js"), "utf-8"), handJs);
        assert.equal(readFileSync(join(d, "Counter.js.map"), "utf-8"), handMap);
        assert.equal(readFileSync(join(d, "Counter.d.ts"), "utf-8"), handDts);
    } finally {
        rmSync(d, { recursive: true, force: true });
    }
});

test("#6 a clean loader run leaves no transient artifacts beside the source", (t) => {
    if (!CMD) { t.skip("no pyths binary available"); return; }
    const d = project("clean");
    try {
        const r = runLoader(d);
        assert.equal(r.error, null);
        const left = readdirSync(d).sort();
        assert.deepEqual(left, ["Counter.d.ps.ts", "Counter.ps"], `left behind: ${left}`);
        assert.ok(readFileSync(join(d, "Counter.d.ps.ts"), "utf-8").includes(GENERATED_MARKER));
    } finally {
        rmSync(d, { recursive: true, force: true });
    }
});

test("#2 a hand-written .d.ps.ts is never overwritten by the loader", (t) => {
    if (!CMD) { t.skip("no pyths binary available"); return; }
    const d = project("decl");
    try {
        const hand = "export declare const mine: number; // hand-written\n";
        writeFileSync(join(d, "Counter.d.ps.ts"), hand);
        const r = runLoader(d);
        assert.equal(r.error, null, "the build must still succeed");
        assert.match(r.code, /function add/);
        assert.equal(readFileSync(join(d, "Counter.d.ps.ts"), "utf-8"), hand);
    } finally {
        rmSync(d, { recursive: true, force: true });
    }
});

test("the loader still returns a usable source map", (t) => {
    if (!CMD) { t.skip("no pyths binary available"); return; }
    const d = project("map");
    try {
        const r = runLoader(d, { emitDts: false });
        assert.equal(r.error, null);
        assert.ok(r.map, "a source map must be produced");
        assert.deepEqual(r.map.sources, ["Counter.ps"]);
        assert.ok(
            Array.isArray(r.map.sourcesContent) && r.map.sourcesContent[0].includes("def add"),
        );
        assert.deepEqual(readdirSync(d), ["Counter.ps"], "emitDts:false writes nothing");
    } finally {
        rmSync(d, { recursive: true, force: true });
    }
});
