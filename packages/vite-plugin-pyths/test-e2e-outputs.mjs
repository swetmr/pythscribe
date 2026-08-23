// End-to-end guard for security findings #2 / #6 — drives the REAL `pyths`
// compiler through the plugin's compile path and asserts that nothing beside
// the user's source is created, overwritten, or deleted.
//
// Before the fix this test failed on the first assertion: the CLI wrote
// `Counter.js` beside the source (refused, because a hand-written file was
// there) and the `finally` block then unlinked the developer's own
// `Counter.js`, `Counter.js.map` and `Counter.d.ts`.
//
// Requires a `pyths` binary: set PYTHS_BIN (the repo's
// `target/release/pyths[.exe]` is picked up automatically). Skips otherwise.
//
// Run with: node --test test-e2e-outputs.mjs
import { test } from "node:test";
import assert from "node:assert";
import { existsSync, mkdtempSync, readFileSync, readdirSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { __compilePsFileForTests as compilePsFile } from "./index.js";
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
    const d = mkdtempSync(join(tmpdir(), `pyths-e2e-${tag}-`));
    writeFileSync(join(d, "Counter.ps"), SOURCE);
    return d;
}

test("#6 a hand-written same-stem .js / .js.map / .d.ts survives a compile", (t) => {
    if (!CMD) { t.skip("no pyths binary available"); return; }
    const d = project("keep");
    try {
        const handJs = "export const mine = 1; // hand-written, NOT generated\n";
        const handMap = '{"version":3,"sources":["mine"],"mappings":""}';
        const handDts = "export declare const mine: number; // hand-written\n";
        writeFileSync(join(d, "Counter.js"), handJs);
        writeFileSync(join(d, "Counter.js.map"), handMap);
        writeFileSync(join(d, "Counter.d.ts"), handDts);

        const { code } = compilePsFile(CMD, join(d, "Counter.ps"), { emitDts: true });
        assert.match(code, /function add/, "the compile itself must still succeed");

        // The three pre-existing files must be byte-identical and still present.
        assert.equal(readFileSync(join(d, "Counter.js"), "utf-8"), handJs);
        assert.equal(readFileSync(join(d, "Counter.js.map"), "utf-8"), handMap);
        assert.equal(readFileSync(join(d, "Counter.d.ts"), "utf-8"), handDts);
    } finally {
        rmSync(d, { recursive: true, force: true });
    }
});

test("#6 a clean compile leaves no transient artifacts beside the source", (t) => {
    if (!CMD) { t.skip("no pyths binary available"); return; }
    const d = project("clean");
    try {
        compilePsFile(CMD, join(d, "Counter.ps"), { emitDts: true });
        const left = readdirSync(d).sort();
        // Only the source and the intentional declaration sibling.
        assert.deepEqual(left, ["Counter.d.ps.ts", "Counter.ps"], `left behind: ${left}`);
        assert.ok(
            readFileSync(join(d, "Counter.d.ps.ts"), "utf-8").includes(GENERATED_MARKER),
            "the emitted declaration must carry the generated marker",
        );
    } finally {
        rmSync(d, { recursive: true, force: true });
    }
});

test("#2 a hand-written .d.ps.ts is never overwritten by the declaration emit", (t) => {
    if (!CMD) { t.skip("no pyths binary available"); return; }
    const d = project("decl");
    try {
        const hand = "export declare const mine: number; // hand-written\n";
        writeFileSync(join(d, "Counter.d.ps.ts"), hand);
        const { code } = compilePsFile(CMD, join(d, "Counter.ps"), { emitDts: true });
        assert.match(code, /function add/, "the build must still succeed");
        assert.equal(
            readFileSync(join(d, "Counter.d.ps.ts"), "utf-8"),
            hand,
            "the unmarked declaration must be left alone",
        );
    } finally {
        rmSync(d, { recursive: true, force: true });
    }
});

test("emitDts:false writes no declaration at all", (t) => {
    if (!CMD) { t.skip("no pyths binary available"); return; }
    const d = project("nodts");
    try {
        compilePsFile(CMD, join(d, "Counter.ps"), { emitDts: false });
        assert.deepEqual(readdirSync(d), ["Counter.ps"]);
    } finally {
        rmSync(d, { recursive: true, force: true });
    }
});

test("source maps still resolve back to the .ps source", (t) => {
    if (!CMD) { t.skip("no pyths binary available"); return; }
    const d = project("map");
    try {
        const { map } = compilePsFile(CMD, join(d, "Counter.ps"), { emitDts: false });
        assert.ok(map, "a source map must be produced");
        assert.deepEqual(map.sources, ["Counter.ps"], "sources must name the .ps file");
        assert.ok(map.mappings.length > 0, "mappings must be non-empty");
        assert.ok(
            Array.isArray(map.sourcesContent) && map.sourcesContent[0].includes("def add"),
            "sourcesContent must inline the original .ps text",
        );
    } finally {
        rmSync(d, { recursive: true, force: true });
    }
});
