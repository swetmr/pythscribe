#!/usr/bin/env node
// Version-drift guard (run in CI). Asserts EVERY version location agrees with the
// Cargo workspace version (the canonical committed source). Catches a manual bump
// that missed a file — BEFORE a release run wastes CI minutes on the resulting
// npm EPUBLISHCONFLICT (the 0.2.1 platform-template drift).
//
//   node scripts/check-versions.mjs

import { readFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
import { PKG_JSON } from "./set-version.mjs";

const root = join(dirname(fileURLToPath(import.meta.url)), "..");

const cargo = readFileSync(join(root, "Cargo.toml"), "utf8").match(/^version = "([^"]+)"/m);
if (!cargo) {
  console.error("check-versions: could not find the workspace version in Cargo.toml");
  process.exit(1);
}
const expected = cargo[1];

const seen = [["Cargo.toml", expected]];
for (const rel of PKG_JSON) {
  const j = JSON.parse(readFileSync(join(root, rel), "utf8"));
  seen.push([rel, j.version]);
  if (j.optionalDependencies) {
    for (const [k, v] of Object.entries(j.optionalDependencies)) {
      if (k.startsWith("@pythscribe/cli-")) seen.push([`${rel} → ${k}`, v]);
    }
  }
}
// lockfile own version (top field)
{
  const lf = readFileSync(join(root, "npm/pythscribe/package-lock.json"), "utf8").match(/"version": "([^"]+)"/);
  if (lf) seen.push(["npm/pythscribe/package-lock.json", lf[1]]);
}

const drift = seen.filter(([, v]) => v !== expected);
if (drift.length) {
  console.error(`Version DRIFT — these disagree with Cargo.toml (${expected}):`);
  for (const [loc, v] of drift) console.error(`  ${loc} = ${v}`);
  console.error(`\nFix with:  node scripts/set-version.mjs ${expected}`);
  process.exit(1);
}
console.log(`✓ all ${seen.length} version locations == ${expected}`);
