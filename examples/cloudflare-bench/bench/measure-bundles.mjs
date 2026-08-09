#!/usr/bin/env node
// Report gzipped sizes of the deployable artifacts for both workers.

import { readFile, stat } from "node:fs/promises";
import { gzipSync } from "node:zlib";
import { dirname, resolve, join } from "node:path";
import { fileURLToPath } from "node:url";

const here = dirname(fileURLToPath(import.meta.url));
const root = resolve(here, "..");

async function tryStat(path) {
  try {
    return await stat(path);
  } catch {
    return null;
  }
}

async function measure(label, paths) {
  console.log(`\n=== ${label} ===`);
  let totalRaw = 0;
  let totalGz = 0;
  for (const p of paths) {
    const s = await tryStat(p);
    if (!s) {
      console.log(`  ${p} — NOT FOUND (skipping)`);
      continue;
    }
    const buf = await readFile(p);
    const gz = gzipSync(buf, { level: 9 });
    totalRaw += buf.length;
    totalGz += gz.length;
    console.log(
      `  ${p.replace(root + "\\", "").replace(root + "/", "")}: ${buf.length} bytes (gzip: ${gz.length})`,
    );
  }
  console.log(`  TOTAL: ${totalRaw} bytes raw, ${totalGz} bytes gzipped`);
  return { raw: totalRaw, gz: totalGz };
}

const psPaths = [
  join(root, "pythscribe-worker", "src", "worker.js"),
  join(root, "pythscribe-worker", "src", "worker.wasm"),
];

// Pyodide's actual deployed bundle is the entry script — but the bulk of the
// runtime is downloaded from CDN at runtime. For an honest comparison, we
// list both the script AND a note about the runtime download size.
const pyPaths = [join(root, "pyodide-worker", "src", "index.js")];

const ps = await measure("PythScribe Worker", psPaths);
const py = await measure("Pyodide Worker (script only)", pyPaths);

console.log("\n=== Pyodide runtime download (informational) ===");
console.log("  Pyodide loads ~6.5 MB compressed at runtime from CDN.");
console.log("  Reference: pyodide.asm.wasm + pyodide.asm.js + python_stdlib.zip");

console.log("\n=== Comparison ===");
console.log(
  `  Bundle ratio (gzipped): Pyodide is ~${(((py.gz + 6_500_000) / (ps.gz || 1)) * 1).toFixed(0)}x larger when runtime is included.`,
);
