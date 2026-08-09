#!/usr/bin/env node
"use strict";

// Thin launcher for the `pyths` (PythScribe) compiler. The real native binary is
// shipped in a per-platform optional dependency (esbuild/swc-style); npm installs
// only the one matching the host `os`/`cpu`. We resolve it at runtime and exec it,
// forwarding argv and the exit code. No binary is bundled in this package itself.

const { spawnSync } = require("node:child_process");
const path = require("node:path");
const fs = require("node:fs");

// host key -> platform package name
const PLATFORM_PACKAGES = {
  "win32 x64": "@pythscribe/cli-win32-x64",
  "linux x64": "@pythscribe/cli-linux-x64",
  "linux arm64": "@pythscribe/cli-linux-arm64",
  "darwin x64": "@pythscribe/cli-darwin-x64",
  "darwin arm64": "@pythscribe/cli-darwin-arm64",
};

function resolveBinary() {
  const key = `${process.platform} ${process.arch}`;
  const pkg = PLATFORM_PACKAGES[key];
  if (!pkg) return { pkg: null, bin: null };
  const exe = process.platform === "win32" ? "pyths.exe" : "pyths";
  try {
    // Resolve via the platform package's package.json (always resolvable, not
    // gated by an "exports" field), then join the known bin path.
    const pkgDir = path.dirname(require.resolve(`${pkg}/package.json`));
    const bin = path.join(pkgDir, "bin", exe);
    return { pkg, bin: fs.existsSync(bin) ? bin : null };
  } catch {
    return { pkg, bin: null };
  }
}

const { pkg, bin } = resolveBinary();

if (!bin) {
  const supported = Object.values(PLATFORM_PACKAGES).join(", ");
  process.stderr.write(
    `pyths: could not find the prebuilt compiler binary for ${process.platform}-${process.arch}.\n` +
      (pkg
        ? `The platform package "${pkg}" is expected but its binary was not found — optional dependencies may have been skipped (e.g. \`--no-optional\`, or a package manager that ignores os/cpu). Try reinstalling without \`--no-optional\`.\n`
        : `This platform is not among the prebuilt targets (${supported}).\n`) +
      `Alternatively build from source with a Rust toolchain: \`cargo install --git https://github.com/swetmr/pythscribe pyths\`.\n`,
  );
  process.exit(1);
}

const result = spawnSync(bin, process.argv.slice(2), { stdio: "inherit" });
if (result.error) {
  process.stderr.write(`pyths: failed to launch the compiler binary: ${result.error.message}\n`);
  process.exit(1);
}
process.exit(result.status === null ? 1 : result.status);
