// `pyths-safe.js` is duplicated byte-for-byte into every plugin package
// (each is published to npm standalone and cannot import from a sibling).
// Same discipline as the two runtime copies: the copies must never drift, or
// one plugin silently keeps a defect the other one fixed.
//
// Run with: node --test test-safe-parity.mjs
import { test } from "node:test";
import assert from "node:assert";
import { createHash } from "node:crypto";
import { readFileSync } from "node:fs";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const here = dirname(fileURLToPath(import.meta.url));
const packagesDir = resolve(here, "..");

const COPIES = [
    join(packagesDir, "vite-plugin-pyths", "pyths-safe.js"),
    join(packagesDir, "next-plugin-pyths", "pyths-safe.js"),
];

test("every plugin package ships an identical pyths-safe.js", () => {
    const digests = COPIES.map((p) => {
        const bytes = readFileSync(p);
        return { p, sha: createHash("sha256").update(bytes).digest("hex"), len: bytes.length };
    });
    const first = digests[0];
    for (const d of digests.slice(1)) {
        assert.equal(
            d.sha,
            first.sha,
            `pyths-safe.js copies have DRIFTED:\n  ${first.p} (${first.len} bytes, ${first.sha.slice(0, 16)})\n` +
            `  ${d.p} (${d.len} bytes, ${d.sha.slice(0, 16)})\n` +
            `  Copy one over the other and re-run.`,
        );
    }
});

test("every plugin package lists pyths-safe.js in its published files", () => {
    for (const pkgName of ["vite-plugin-pyths", "next-plugin-pyths"]) {
        const pkg = JSON.parse(readFileSync(join(packagesDir, pkgName, "package.json"), "utf-8"));
        assert.ok(
            pkg.files.includes("pyths-safe.js"),
            `${pkgName}/package.json "files" must include pyths-safe.js or the published ` +
            `package will fail to import it at build time`,
        );
    }
});
