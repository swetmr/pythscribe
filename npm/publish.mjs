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
import { verifyPreparedTarball } from "./payload-check.mjs";

const __dirname = dirname(fileURLToPath(import.meta.url));
const doPublish = process.argv.includes("--yes");

// S9: the publish set derives from the ONE structural authority npm/packages.json (also loaded by
// scripts/require_evidence.py -> NPM_PACKAGES, the R-NI evidence set) -- not a regex-linked duplicate.
const repoRoot = join(__dirname, "..");
const PKGS = JSON.parse(readFileSync(join(__dirname, "packages.json"), "utf8"));
const PLATFORM_PKGS = PKGS.platform;

// Pure-JS packages the wrapper depends on (`pythscribe`'s regular `dependencies`),
// so they MUST be published at the same version or `npm install pythscribe` fails
// with ETARGET. They live OUTSIDE npm/ (repo-root-relative), unlike the platform
// packages. Published BEFORE the wrapper (like the platform packages) so the whole
// dependency closure resolves at install time. Directories come from packages.json.
const EXTRA_DIRS = Object.fromEntries(
  Object.entries({ ...PKGS.payload, ...PKGS.plugins }).map(([name, rel]) => [name, join(repoRoot, ...rel.split("/"))]),
);
function pkgDir(name) {
  return EXTRA_DIRS[name] || join(__dirname, name);
}
// M0.1 (spec 13-09-26-lib-selfcontained-pip-wheel): `pyths-runtime` and `create-pyths-app` are
// published from the PREPARED payload tarballs written by `scripts/prepare_release_payloads.py`
// (`npm pack` of the checkout + the membership/LF/byte-identity gate) -- NEVER from the checkout
// directory. The same payload is vendored into the pip wheel, so the npm tarball and the wheel
// carry the same bytes by construction (raw-byte identity, no normalization).
const PAYLOADS = {
  "pyths-runtime": { tgz: join(repoRoot, "dist", "runtime-payload.tgz"), files: join(repoRoot, "dist", "runtime-payload.files.json") },
  "create-pyths-app": { tgz: join(repoRoot, "dist", "scaffolder-payload.tgz"), files: join(repoRoot, "dist", "scaffolder-payload.files.json") },
};
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

// M6.1 (spec 13-09-26, requirements §5.4 "stamp idempotence, not stamping"): on a TAG ref the committed
// package.json files MUST already be in stamped form (`scripts/set_version.py` writes them exactly as
// `stampVersion` would emit them: `JSON.stringify(j, null, 2) + "\n"`, literal em dash, tag version,
// intra-distribution pins). So on a tag ref -- or with an explicit `--check` -- publish.mjs only CHECKS:
// if stamping would change ANY byte of ANY package.json it exits 1 ("run set_version.py and commit").
// Rewrite mode survives ONLY for a non-tag `workflow_dispatch` backfill. Consequence: checkout bytes ==
// prepared payload bytes == compiler-embedded bytes == npm tarball bytes (validation §C4).
const onTagRef = /^refs\/tags\//.test(process.env.GITHUB_REF || "");
const checkOnly = process.argv.includes("--check");
const checkMode = checkOnly || onTagRef;

// B1: actually publishing (--yes) is reachable ONLY from an exact vX.Y.Z tag ref, and the version
// MUST equal the release manifest's -- npm must never publish from a branch dispatch, nor at a version
// the manifest did not bind. Because a tag ref forces checkMode, stampVersion (rewrite mode) never
// runs under --yes: CI publication is check-only by construction.
if (doPublish) {
  if (!onTagRef) {
    console.error(`Refusing to publish: --yes requires a tag ref (refs/tags/vX.Y.Z); GITHUB_REF=${process.env.GITHUB_REF || "(unset)"} (B1). npm publication is only reachable from a tag build.`);
    process.exit(1);
  }
  const mi = process.argv.indexOf("--manifest");
  if (mi === -1 || !process.argv[mi + 1]) {
    console.error("Refusing to publish: --yes requires --manifest <release_manifest.json> so the publish version is bound to the manifest (B1).");
    process.exit(1);
  }
  const mver = JSON.parse(readFileSync(process.argv[mi + 1], "utf8")).version;
  if (mver !== VERSION) {
    console.error(`Refusing to publish: publish version ${VERSION} != release_manifest.json version ${mver} (B1) -- the tag/manifest disagree; run scripts/set_version.py.`);
    process.exit(1);
  }
}

function stampedJson(name) {
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
  return { path: p, text: JSON.stringify(j, null, 2) + "\n" };
}
function stampVersion(name) {
  const { path, text } = stampedJson(name);
  writeFileSync(path, text);
}
/** `--check`: the committed bytes must already equal the stamped form. Returns a problem line or null. */
function checkStamped(name) {
  const { path, text } = stampedJson(name);
  const current = readFileSync(path, "utf8");
  if (current === text) return null;
  const a = current.split("\n"), b = text.split("\n");
  let i = 0;
  while (i < a.length && i < b.length && a[i] === b[i]) i++;
  return `${path}: stamping would change it (first differing line ${i + 1}: committed ${JSON.stringify(a[i] ?? "<EOF>")} vs stamped ${JSON.stringify(b[i] ?? "<EOF>")})`;
}
// The package.json files REWRITTEN in backfill mode (payload packages are fixed by the prepare step and never
// re-stamped) ...
const STAMPED = [...PRE_WRAPPER, "pythscribe"].filter((n) => !Object.hasOwn(PAYLOADS, n));
// ... but the CHECK covers EVERY package.json, payload packages included (validation §C4: a runtime/package.json
// still carrying `—` or a stale version is RED -- checkout bytes must equal the prepared payload == the
// compiler-embedded include_str! bytes == the npm tarball).
const CHECKED = [...PRE_WRAPPER, "pythscribe"];
if (checkMode) {
  const problems = CHECKED.map(checkStamped).filter(Boolean);
  if (problems.length) {
    console.error(`stampVersion --check: RED at ${VERSION}${onTagRef ? ` (tag ref ${process.env.GITHUB_REF})` : ""} -- the committed package.json files are not in stamped form:\n  ${problems.join("\n  ")}\n` +
      `Run \`python scripts/set_version.py ${VERSION}\` and commit (a tag build never rewrites; checkout bytes must already equal what npm ships).`);
    process.exit(1);
  }
  console.log(`stampVersion --check: GREEN -- ${CHECKED.length} package.json files already in stamped form at ${VERSION}.`);
  if (checkOnly) process.exit(0);
}
// A payload package is NOT re-stamped (its bytes are fixed by the prepare step). The artifact
// that is PUBLISHED -- the .tgz itself -- is verified against the prepared payload's files.json
// (member set + per-file sha256 + name@VERSION; npm/payload-check.mjs), so a stale or divergent
// tarball is refused here, not caught only by the downstream M6.2 identity gate (codex M0 SF1).
// The M6.1 `--check` idempotence gate generalizes the version check to every package.json.
function checkPayload(name) {
  const { tgz, files } = PAYLOADS[name];
  if (!existsSync(tgz) || !existsSync(files)) {
    console.error(`Refusing to publish ${name}: prepared payload missing (${tgz} / ${files}).\n` +
      `Run \`python scripts/prepare_release_payloads.py\` on this checkout first (publish.mjs never packs the checkout directly).`);
    process.exit(1);
  }
  const problems = verifyPreparedTarball(tgz, files, name, VERSION);
  if (problems.length) {
    console.error(`Refusing to publish ${name}: ${tgz} is not the prepared payload at ${VERSION}:\n  ${problems.join("\n  ")}\n` +
      `Run \`python scripts/set_version.py ${VERSION}\` (commit) if the version is stale, then re-run \`python scripts/prepare_release_payloads.py\`.`);
    process.exit(1);
  }
}
for (const pkg of [...PRE_WRAPPER, "pythscribe"]) {
  // B1: never rewrite in CI or under checkMode (tag ref / --check). Rewrite survives only for a
  // LOCAL, non-tag, non-CI dry-run convenience.
  if (Object.hasOwn(PAYLOADS, pkg)) checkPayload(pkg); else if (!checkMode && !process.env.CI) stampVersion(pkg);
}
console.log(`Publishing version ${VERSION} (from ${process.env.GITHUB_REF_NAME ? `tag ${process.env.GITHUB_REF_NAME}` : "wrapper package.json"}${checkMode ? "; stamp-checked, not rewritten" : "; package.json files re-stamped (non-tag backfill)"}).`);

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
  if (doPublish && alreadyPublished(pkg, VERSION)) {
    console.log(`\n=== SKIP ${pkg}@${VERSION} (already on registry) ===`);
    continue;
  }
  // Payload packages: `npm publish <prepared .tgz>` from the repo root; everything else: the
  // package directory (npm packs it in place).
  const payload = PAYLOADS[pkg];
  const cwd = payload ? repoRoot : pkgDir(pkg);
  const pubArgs = payload ? [...args, payload.tgz] : args;
  console.log(`\n=== ${doPublish ? "PUBLISH" : "dry-run"} ${pkg}${payload ? ` (prepared payload ${payload.tgz})` : ""} ===`);
  execFileSync("npm", pubArgs, { cwd, stdio: "inherit", shell: process.platform === "win32" });
}
console.log(doPublish ? "\nPublished all packages." : "\nDry run complete. Re-run with --yes to publish.");
