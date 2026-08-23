#!/usr/bin/env node
// Publishes the pyths npm distribution. Platform packages FIRST (so the wrapper's
// optionalDependencies resolve at install time), then the `pyths` wrapper last.
// Refuses to publish a platform package whose binary is missing. Dry-run by default.
//
//   node npm/publish.mjs            # dry run: `npm publish --dry-run` for each
//   node npm/publish.mjs --yes      # actually publish (needs `npm login`)

import { execFileSync } from "node:child_process";
import { existsSync, readdirSync, readFileSync, writeFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const __dirname = dirname(fileURLToPath(import.meta.url));
const doPublish = process.argv.includes("--yes");

const PLATFORM_PKGS = [
  "@pythscribe/cli-win32-x64", "@pythscribe/cli-linux-x64", "@pythscribe/cli-linux-arm64",
  "@pythscribe/cli-darwin-x64", "@pythscribe/cli-darwin-arm64",
];

// Pure-JS packages the wrapper depends on (`pythscribe`'s regular `dependencies`),
// so they MUST be published at the same version or `npm install pythscribe` fails
// with ETARGET. They live OUTSIDE npm/ (repo-root-relative), unlike the platform
// packages. Published BEFORE the wrapper (like the platform packages) so the whole
// dependency closure resolves at install time.
const repoRoot = join(__dirname, "..");
const EXTRA_DIRS = {
  "pyths-runtime": join(repoRoot, "runtime"),
  "vite-plugin-pyths": join(repoRoot, "packages", "vite-plugin-pyths"),
  "next-plugin-pyths": join(repoRoot, "packages", "next-plugin-pyths"),
  "create-pyths-app": join(repoRoot, "packages", "create-pyths-app"),
};
function pkgDir(name) {
  return EXTRA_DIRS[name] || join(__dirname, name);
}
// Everything published before the wrapper (wrapper LAST so its deps exist first).
const PRE_WRAPPER = [...PLATFORM_PKGS, ...Object.keys(EXTRA_DIRS)];

// ── Single-source-of-truth version ────────────────────────────────────────
// The RELEASE VERSION comes from the git TAG. `release.yml` runs `on: push: tags:
// v*`, so `GITHUB_REF_NAME` is the tag (e.g. `v0.2.1`); strip the leading `v`.
// Locally (no tag), fall back to the committed wrapper version. Then STAMP that
// version into EVERY package.json we publish (the 5 platform packages, the
// wrapper, and the wrapper's `@pythscribe/cli-*` optionalDependency pins), so a
// stale committed template version can NEVER cause an EPUBLISHCONFLICT — the tag
// is authoritative. (This is the fix for the 0.2.1 release, where the platform
// templates were left at 0.2.0 and re-publishing 0.2.0 was rejected.)
function releaseVersion() {
  const ref = process.env.GITHUB_REF_NAME || "";
  const m = ref.match(/^v?(\d+\.\d+\.\d+(?:[-+].+)?)$/);
  if (m) return m[1];
  const wrapper = JSON.parse(readFileSync(join(__dirname, "pythscribe", "package.json"), "utf8"));
  return wrapper.version;
}
const VERSION = releaseVersion();

function stampVersion(name) {
  const p = join(pkgDir(name), "package.json");
  const j = JSON.parse(readFileSync(p, "utf8"));
  j.version = VERSION;
  // Stamp the tag version into every intra-distribution pin so a stale committed
  // template can never cause an ETARGET (wrapper deps) or EPUBLISHCONFLICT.
  for (const field of ["dependencies", "optionalDependencies"]) {
    if (!j[field]) continue;
    for (const k of Object.keys(j[field])) {
      if (k.startsWith("@pythscribe/cli-")) j[field][k] = VERSION;         // exact-pin the binaries
      else if (Object.hasOwn(EXTRA_DIRS, k)) j[field][k] = `^${VERSION}`;  // caret-pin the JS deps
    }
  }
  writeFileSync(p, JSON.stringify(j, null, 2) + "\n");
}
for (const pkg of [...PRE_WRAPPER, "pythscribe"]) stampVersion(pkg);
console.log(`Publishing version ${VERSION} (from ${process.env.GITHUB_REF_NAME ? `tag ${process.env.GITHUB_REF_NAME}` : "wrapper package.json"}).`);

function hasBinary(pkg) {
  const binDir = join(__dirname, pkg, "bin");
  return existsSync(binDir) && readdirSync(binDir).some((f) => f === "pyths" || f === "pyths.exe");
}

const missing = PLATFORM_PKGS.filter((p) => !hasBinary(p));
if (missing.length) {
  console.error(`Refusing to publish: platform binaries missing for:\n  ${missing.join("\n  ")}\n` +
    `Build them (CI matrix) and run npm/build-platform-packages.mjs first.`);
  process.exit(1);
}

// DEEP GUARD (regression fence for the 0.2.2 ETARGET): every intra-distribution
// package the wrapper DEPENDS on must be in the publish set, or `npm install
// pythscribe` fails with ETARGET (the wrapper resolves a dep version we never
// published). This makes it structurally impossible to add a wrapper dep without
// also publishing it — it would have failed the release BEFORE 0.2.2 shipped.
{
  const wrapper = JSON.parse(readFileSync(join(__dirname, "pythscribe", "package.json"), "utf8"));
  const deps = { ...(wrapper.dependencies || {}), ...(wrapper.optionalDependencies || {}) };
  const isOurs = (n) =>
    n.startsWith("@pythscribe/") || ["pyths-runtime", "vite-plugin-pyths", "next-plugin-pyths"].includes(n);
  const published = new Set([...PRE_WRAPPER, "pythscribe"]);
  const orphans = Object.keys(deps).filter((n) => isOurs(n) && !published.has(n));
  if (orphans.length) {
    console.error(
      `Refusing to publish: the wrapper depends on intra-distribution package(s) NOT in the publish set:\n  ${orphans.join("\n  ")}\n` +
        `Add each to PRE_WRAPPER (and EXTRA_DIRS if it lives outside npm/) so the whole closure publishes together.`);
    process.exit(1);
  }
}

// DEEP GUARD 2 (distribution completeness): EVERY publishable package in the
// distribution dirs must be in the publish set — even ones nothing depends on
// (create-pyths-app, next-plugin-pyths). Guard 1 only covers wrapper deps; this
// catches a standalone package left stale on npm (the 0.2.2 create-pyths-app /
// next-plugin-pyths gap). Add a new package to EXTRA_DIRS/PLATFORM_PKGS, or mark
// it `"private": true`, or the release fails here.
{
  const manifests = [join(__dirname, "pythscribe", "package.json"), join(repoRoot, "runtime", "package.json")];
  for (const parent of [join(__dirname, "@pythscribe"), join(repoRoot, "packages")]) {
    if (!existsSync(parent)) continue;
    for (const name of readdirSync(parent)) {
      const f = join(parent, name, "package.json");
      if (existsSync(f)) manifests.push(f);
    }
  }
  const published = new Set([...PRE_WRAPPER, "pythscribe"]);
  const orphans = [];
  for (const f of manifests) {
    const j = JSON.parse(readFileSync(f, "utf8"));
    if (j.name && !j.private && !published.has(j.name)) orphans.push(`${j.name}  (${f})`);
  }
  if (orphans.length) {
    console.error(
      `Refusing to publish: publishable package(s) in the distribution are NOT in the publish set:\n  ${orphans.join("\n  ")}\n` +
        `Add each to PLATFORM_PKGS or EXTRA_DIRS, or set "private": true if it must not publish.`);
    process.exit(1);
  }
}

// Provenance is DISABLED: npm --provenance requires a PUBLIC GitHub source repo, but this
// repo is private, so it fails with E422 "Unsupported ... source repository visibility:
// private". Re-enable with `doPublish && process.env.CI ? ["--provenance"] : []` only if the
// source repository is ever made public.
const provenance = [];
// 2FA: if the npm account requires a one-time password on publish, pass it via
// NPM_OTP so a LOCAL publish (not the CI token path) works. npm caches the OTP
// for the short burst of sequential publishes, so one fresh code covers all six.
const otp = process.env.NPM_OTP ? [`--otp=${process.env.NPM_OTP}`] : [];
const args = ["publish", ...(doPublish ? [] : ["--dry-run"]), "--access", "public", ...provenance, ...otp];

// Idempotent: an already-published version is IMMUTABLE on npm, so re-running
// (e.g. after a partial publish, or to add a missing dep to an existing release)
// must SKIP what already exists rather than 409/EPUBLISHCONFLICT. This is what
// lets a re-run publish only the missing packages of an existing version.
function alreadyPublished(name, version) {
  try {
    const out = execFileSync("npm", ["view", `${name}@${version}`, "version"],
      { encoding: "utf8", stdio: ["ignore", "pipe", "ignore"], shell: process.platform === "win32" });
    return out.trim() === version;
  } catch {
    return false;  // E404 (no such version) -> not published yet
  }
}

for (const pkg of [...PRE_WRAPPER, "pythscribe"]) {  // wrapper LAST
  const cwd = pkgDir(pkg);
  if (doPublish && alreadyPublished(pkg, VERSION)) {
    console.log(`\n=== SKIP ${pkg}@${VERSION} (already on registry) ===`);
    continue;
  }
  console.log(`\n=== ${doPublish ? "PUBLISH" : "dry-run"} ${pkg} ===`);
  execFileSync("npm", args, { cwd, stdio: "inherit", shell: process.platform === "win32" });
}
console.log(doPublish ? "\nPublished all packages." : "\nDry run complete. Re-run with --yes to publish.");
