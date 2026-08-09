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

function stampVersion(pkgRelDir) {
  const p = join(__dirname, pkgRelDir, "package.json");
  const j = JSON.parse(readFileSync(p, "utf8"));
  j.version = VERSION;
  if (j.optionalDependencies) {
    for (const k of Object.keys(j.optionalDependencies)) {
      if (k.startsWith("@pythscribe/cli-")) j.optionalDependencies[k] = VERSION;
    }
  }
  writeFileSync(p, JSON.stringify(j, null, 2) + "\n");
}
for (const pkg of [...PLATFORM_PKGS, "pythscribe"]) stampVersion(pkg);
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
for (const pkg of [...PLATFORM_PKGS, "pythscribe"]) {  // wrapper LAST
  const cwd = join(__dirname, pkg);
  console.log(`\n=== ${doPublish ? "PUBLISH" : "dry-run"} ${pkg} ===`);
  execFileSync("npm", args, { cwd, stdio: "inherit", shell: process.platform === "win32" });
}
console.log(doPublish ? "\nPublished all packages." : "\nDry run complete. Re-run with --yes to publish.");
