#!/usr/bin/env node
// Single-command version bump — the ONE place that enumerates EVERY version
// location, so a release bump can't silently miss a file. (Motivated by the
// 0.2.1 release, where the 5 npm/@pythscribe/cli-* platform TEMPLATES were left
// at 0.2.0 and re-publishing 0.2.0 was rejected by npm.)
//
// The git TAG stays the release source of truth (npm/publish.mjs derives npm
// versions from it at publish time); this script keeps the COMMITTED files
// consistent so `pyths --version` is right and `scripts/check-versions.mjs` /
// CI passes. Uses targeted string replacement — does NOT reformat the files.
//
//   node scripts/set-version.mjs 0.2.2

import { readFileSync, writeFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath, pathToFileURL } from "node:url";

const root = join(dirname(fileURLToPath(import.meta.url)), "..");

// The full enumeration of package.json version locations — shared with
// check-versions.mjs. Exported with NO import-time side effects.
export const PKG_JSON = [
  "runtime/package.json",
  "npm/pythscribe/package.json",
  "npm/@pythscribe/cli-win32-x64/package.json",
  "npm/@pythscribe/cli-linux-x64/package.json",
  "npm/@pythscribe/cli-linux-arm64/package.json",
  "npm/@pythscribe/cli-darwin-x64/package.json",
  "npm/@pythscribe/cli-darwin-arm64/package.json",
  "packages/create-pyths-app/package.json",
  "packages/next-plugin-pyths/package.json",
  "packages/vite-plugin-pyths/package.json",
];

const SEMVER = String.raw`\d+\.\d+\.\d+(?:[-+][\w.]+)?`;

function setPkgVersion(rel, version) {
  const p = join(root, rel);
  let s = readFileSync(p, "utf8");
  // the package's own `version` (first occurrence)
  s = s.replace(new RegExp(`("version":\\s*)"${SEMVER}"`), `$1"${version}"`);
  // any @pythscribe/cli-* dependency pins (exact-pinned)
  s = s.replace(new RegExp(`("@pythscribe/cli-[\\w-]+":\\s*)"${SEMVER}"`, "g"), `$1"${version}"`);
  // intra-distribution JS-dep pins the wrapper carries (caret-pinned) — keep in lockstep
  s = s.replace(
    new RegExp(`("(?:pyths-runtime|vite-plugin-pyths|next-plugin-pyths)":\\s*)"\\^?${SEMVER}"`, "g"),
    `$1"^${version}"`,
  );
  writeFileSync(p, s);
}

function setCargoVersion(version) {
  const p = join(root, "Cargo.toml");
  const s = readFileSync(p, "utf8").replace(new RegExp(`^version = "${SEMVER}"`, "m"), `version = "${version}"`);
  writeFileSync(p, s);
}

function setLockVersion(version) {
  const p = join(root, "npm/pythscribe/package-lock.json");
  const lines = readFileSync(p, "utf8").split("\n");
  let n = 0;
  for (let i = 0; i < lines.length && n < 2; i++) {
    if (new RegExp(`^\\s*"version": "${SEMVER}",?\\s*$`).test(lines[i])) {
      lines[i] = lines[i].replace(new RegExp(`"${SEMVER}"`), `"${version}"`);
      n++;
    }
  }
  writeFileSync(p, lines.join("\n"));
}

const isMain = process.argv[1] && import.meta.url === pathToFileURL(process.argv[1]).href;
if (isMain) {
  const version = process.argv[2];
  if (!version || !new RegExp(`^${SEMVER}$`).test(version)) {
    console.error("Usage: node scripts/set-version.mjs <x.y.z>");
    process.exit(1);
  }
  for (const rel of PKG_JSON) setPkgVersion(rel, version);
  setCargoVersion(version);
  setLockVersion(version);
  console.log(`Set version → ${version} across ${PKG_JSON.length} package.json + Cargo.toml + lockfile.`);
  console.log(`Next: git commit -am "release ${version}" && git tag v${version} && git push --follow-tags`);
}
