#!/usr/bin/env node
"use strict";

// Thin launcher for the `pyths` (PythScribe) compiler. The real native binary is
// shipped in a per-platform optional dependency (esbuild/swc-style); npm installs
// only the one matching the host `os`/`cpu`. We resolve it at runtime and exec it,
// forwarding argv and the exit code. No binary is bundled in this package itself.

const { spawnSync } = require("node:child_process");
const path = require("node:path");
const fs = require("node:fs");

// `pyths skill [name]` — emit a bundled authoring skill so an LLM/agent can pick it
// up from the installed package (node_modules/pythscribe/skills/*.md). Handled here,
// before we resolve the native binary, since it needs no compiler. With no name it
// lists what shipped; with a name it prints that skill's markdown to stdout.
if (process.argv[2] === "skill") {
  const skillsDir = path.join(__dirname, "..", "skills");
  let files = [];
  try {
    files = fs.readdirSync(skillsDir).filter((f) => f.endsWith(".md"));
  } catch {
    /* dir absent -> handled below */
  }
  if (files.length === 0) {
    process.stderr.write(
      `pyths: no bundled skills found (expected under ${skillsDir}).\n`,
    );
    process.exit(1);
  }
  const want = process.argv[3];
  if (!want) {
    process.stdout.write(
      "Bundled PythScribe skills (pass a name to print one):\n",
    );
    for (const f of files) {
      const name = f.replace(/\.md$/, "");
      process.stdout.write(`  ${name}\n    ${path.join(skillsDir, f)}\n`);
    }
    process.exit(0);
  }
  const match =
    files.find((f) => f.replace(/\.md$/, "") === want) ||
    files.find((f) => f.includes(want));
  if (!match) {
    const names = files.map((f) => f.replace(/\.md$/, "")).join(", ");
    process.stderr.write(
      `pyths: no skill matching "${want}". Available: ${names}.\n`,
    );
    process.exit(1);
  }
  process.stdout.write(fs.readFileSync(path.join(skillsDir, match), "utf8"));
  process.exit(0);
}

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
