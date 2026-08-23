#!/usr/bin/env node
// Generates the per-platform npm packages (@pythscribe/cli-<os>-<arch>) and, for any
// target whose release binary is already built in ../target/<rust-triple>/release/,
// copies the binary into that package's bin/. Idempotent. Run once per platform
// (typically a CI build matrix, one job per triple), or locally to populate the host.
//
//   node npm/build-platform-packages.mjs            # populate whatever host binary exists
//   node npm/build-platform-packages.mjs --all      # write all package.json, copy any present binaries
//
// After all five bins are populated (across the CI matrix), run npm/publish.mjs.

import { readFileSync, writeFileSync, mkdirSync, copyFileSync, existsSync, chmodSync, statSync } from "node:fs";
import { createHash } from "node:crypto";
// The compiler binary is FSL-1.1-ALv2 (repo LICENSE.md), so the packages that SHIP it are too.
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const __dirname = dirname(fileURLToPath(import.meta.url));
const repoRoot = resolve(__dirname, "..");
// Read VERSION from the canonical wrapper package.json — NOT a hardcoded constant.
// A hardcoded "0.2.0" here is invisible to set-version.mjs/check-versions.mjs (which
// only manage the enumerated package.json "version" fields), so it silently drifted
// and, though publish.mjs's tag-derivation overrode it at publish time, it stood as a
// smell (issue #1). Single source of truth = one fewer thing to keep in lockstep.
const VERSION = JSON.parse(
  readFileSync(join(repoRoot, "npm/pythscribe/package.json"), "utf8"),
).version;

const TARGETS = [
  { pkg: "@pythscribe/cli-win32-x64",    os: "win32",  cpu: "x64",   rust: "x86_64-pc-windows-msvc",    exe: "pyths.exe" },
  { pkg: "@pythscribe/cli-linux-x64",    os: "linux",  cpu: "x64",   rust: "x86_64-unknown-linux-gnu",  exe: "pyths" },
  { pkg: "@pythscribe/cli-linux-arm64",  os: "linux",  cpu: "arm64", rust: "aarch64-unknown-linux-gnu", exe: "pyths" },
  { pkg: "@pythscribe/cli-darwin-x64",   os: "darwin", cpu: "x64",   rust: "x86_64-apple-darwin",       exe: "pyths" },
  { pkg: "@pythscribe/cli-darwin-arm64", os: "darwin", cpu: "arm64", rust: "aarch64-apple-darwin",      exe: "pyths" },
];

function platformPackageJson(t) {
  return {
    name: t.pkg,
    version: VERSION,
    description: `The ${t.os}-${t.cpu} native binary for the pyths (PythScribe) compiler. Installed automatically as an optional dependency of \`pyths\`.`,
    repository: { type: "git", url: "git+https://github.com/swetmr/pythscribe.git", directory: `npm/${t.pkg}` },
    license: "FSL-1.1-ALv2",
    os: [t.os],
    cpu: [t.cpu],
    files: ["bin/", "LICENSE.md"],
    preferUnplugged: true,
    publishConfig: { access: "public" },
  };
}

let wrote = 0, copied = 0;
const shasums = [];
for (const t of TARGETS) {
  const pkgDir = join(__dirname, t.pkg);
  const binDir = join(pkgDir, "bin");
  mkdirSync(binDir, { recursive: true });
  writeFileSync(join(pkgDir, "package.json"), JSON.stringify(platformPackageJson(t), null, 2) + "\n");
  copyFileSync(join(repoRoot, "LICENSE.md"), join(pkgDir, "LICENSE.md")); // FSL, ships with the binary
  wrote++;

  // copy the release binary if this host already built that triple
  const candidates = [
    join(repoRoot, "target", t.rust, "release", t.exe),
    // host-native build (no --target) lands in target/release
    ...(process.platform === t.os ? [join(repoRoot, "target", "release", t.exe)] : []),
  ];
  const src = candidates.find(existsSync);
  if (src) {
    const dest = join(binDir, t.exe);
    copyFileSync(src, dest);
    if (t.os !== "win32") {
      // Issue #1: chmodSync is a NO-OP for the unix executable bit on a Windows
      // host, and the old `try{}catch{}` swallowed that — so packing the mac/linux
      // binaries on Windows shipped them 0644 and `pyths` failed with EACCES.
      // FAIL LOUD instead of publishing a broken package: a unix platform package
      // MUST be built on a unix host (the CI release.yml matrix), where chmod takes.
      chmodSync(dest, 0o755);
      const mode = statSync(dest).mode & 0o777;
      if ((mode & 0o111) === 0) {
        throw new Error(
          `${t.pkg}: ${dest} is not executable (mode ${mode.toString(8)}). ` +
            `chmod did not take on this host (${process.platform}) — build unix platform packages ` +
            `on a unix runner via .github/workflows/release.yml, not the local shortcut.`,
        );
      }
    }
    // record the binary's sha256 for out-of-band integrity verification
    shasums.push(`${createHash("sha256").update(readFileSync(dest)).digest("hex")}  ${t.pkg}/bin/${t.exe}`);
    copied++;
    console.log(`  ${t.pkg}: package.json + binary (${src})`);
  } else {
    console.log(`  ${t.pkg}: package.json only (no binary yet — build target ${t.rust})`);
  }
}

// SHASUMS256.txt over the populated binaries (out-of-band integrity; npm's own package
// integrity hashes cover the install path — this is the belt-and-suspenders release artifact).
if (shasums.length) {
  writeFileSync(join(__dirname, "SHASUMS256.txt"), shasums.sort().join("\n") + "\n");
  console.log(`wrote SHASUMS256.txt (${shasums.length} binaries)`);
}
console.log(`\nwrote ${wrote} platform package.json, copied ${copied} binaries.`);
console.log(copied < TARGETS.length
  ? `Remaining binaries need cross-compilation (CI matrix, one job per triple), then re-run + npm/publish.mjs.`
  : `All binaries present — ready for npm/publish.mjs.`);
