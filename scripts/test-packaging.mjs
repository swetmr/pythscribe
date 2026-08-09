#!/usr/bin/env node
// scripts/test-packaging.mjs — Sweep B (S6): fresh-install packaging test.
//
// Proves the four publishable packages work when installed from their npm
// TARBALLS (not npm link, not repo-relative paths) into a fresh project in a
// temp directory OUTSIDE the repo:
//
//   1. `npm pack` each package → tarball.
//   2. Fresh Vite project: install pyths-runtime + vite-plugin-pyths tarballs
//      + vite from the registry; a `.ps` component using a runtime builtin
//      (len → pyLen) is the app's only component. `vite build` must succeed
//      and the compiled component (marker string) plus the bundled runtime
//      helper must appear in the dist output.
//   3. create-pyths-app from its tarball: run the scaffold bin into a temp
//      dir, verify every file it claims to generate exists, and compile the
//      generated entry (`app/page.ps`) + layout + component with the built
//      `pyths` binary. (The scaffold's own `npm install` of next/react is
//      deliberately skipped — compile verification needs only the binary.)
//
// Binary location: vite-plugin-pyths auto-detects `target/{debug,release}/pyths`
// relative to CWD or falls back to `pyths` on PATH — neither exists in a fresh
// temp project, so the test passes the binary explicitly via the plugin's
// documented `pythsBin` option, fed from the PYTHS_BIN env var (which this
// script defaults to <repo>/target/release/pyths).

import {
    mkdtempSync, mkdirSync, writeFileSync, readFileSync, readdirSync,
    existsSync, rmSync,
} from "node:fs";
import { spawnSync } from "node:child_process";
import { tmpdir } from "node:os";
import { join, dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const ROOT = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const EXE = process.platform === "win32" ? ".exe" : "";
const PYTHS = resolve(process.env.PYTHS_BIN || join(ROOT, "target", "release", `pyths${EXE}`));
const MARKER = "PYTHS_PACKAGING_MARKER_e2e_7431";

const PACKAGES = [
    { name: "pyths-runtime", dir: "runtime" },
    { name: "vite-plugin-pyths", dir: "packages/vite-plugin-pyths" },
    { name: "next-plugin-pyths", dir: "packages/next-plugin-pyths" },
    { name: "create-pyths-app", dir: "packages/create-pyths-app" },
];

let failed = false;
const ok = (msg) => console.log(`  ok   ${msg}`);
const fail = (msg) => { failed = true; console.log(`  FAIL ${msg}`); };

function sh(cmd, cwd, env = {}) {
    // shell:true so `npm` resolves to npm.cmd on Windows.
    // npm_config_os pins npm's platform filter to the real platform so
    // platform-specific optional deps (rollup/esbuild native bindings)
    // install even when a user-level .npmrc overrides `os` (env-level npm
    // config outranks user config).
    const r = spawnSync(cmd, {
        cwd, shell: true, encoding: "utf-8", timeout: 420000,
        env: { ...process.env, npm_config_os: process.platform, ...env },
    });
    return { status: r.status, stdout: r.stdout ?? "", stderr: r.stderr ?? "" };
}

function runBin(bin, args, cwd) {
    const r = spawnSync(bin, args, { cwd, encoding: "utf-8", timeout: 60000 });
    return { status: r.status, stdout: r.stdout ?? "", stderr: r.stderr ?? "" };
}

// ---------------------------------------------------------------------------
// Preflight
// ---------------------------------------------------------------------------

console.log(`pyths binary: ${PYTHS}`);
if (runBin(PYTHS, ["--version"]).status !== 0) {
    console.error("FATAL: pyths binary not runnable. Build with `cargo build --release` or set PYTHS_BIN.");
    process.exit(2);
}

const stage = mkdtempSync(join(tmpdir(), "pyths-packaging-"));
console.log(`staging dir:  ${stage}\n`);

// ---------------------------------------------------------------------------
// 1. npm pack each package
// ---------------------------------------------------------------------------

console.log("── npm pack ──");
const tarballs = {};
for (const pkg of PACKAGES) {
    const r = sh(`npm pack --pack-destination "${stage}"`, join(ROOT, pkg.dir));
    const tgz = r.stdout.trim().split(/\r?\n/).pop();
    const path = join(stage, tgz);
    if (r.status !== 0 || !tgz || !existsSync(path)) {
        fail(`npm pack ${pkg.name}: exit ${r.status} ${r.stderr.slice(0, 200)}`);
        continue;
    }
    tarballs[pkg.name] = path;
    ok(`npm pack ${pkg.name} → ${tgz}`);
}
if (failed) finish();

// ---------------------------------------------------------------------------
// 2. Fresh Vite project from tarballs
// ---------------------------------------------------------------------------

console.log("\n── fresh Vite project (runtime + vite-plugin tarballs) ──");
const app = join(stage, "vite-hello");
mkdirSync(app);

writeFileSync(join(app, "package.json"), JSON.stringify({
    name: "vite-hello",
    private: true,
    version: "0.0.0",
    type: "module",
    scripts: { build: "vite build" },
}, null, 2));

writeFileSync(join(app, "index.html"), `<!doctype html>
<html>
  <body>
    <div id="app"></div>
    <script type="module" src="/src/main.js"></script>
  </body>
</html>
`);

mkdirSync(join(app, "src"));
// The hello component: a .ps file whose output must reach the bundle.
// `len()` forces an import from the installed pyths-runtime tarball (pyLen),
// so the build also proves runtime subpath resolution from node_modules.
writeFileSync(join(app, "src", "hello.ps"), `def hello(name):
    return f"Hello, {name}! ${MARKER} len={len(name)}"
`);
writeFileSync(join(app, "src", "main.js"), `import { hello } from "./hello.ps";

document.getElementById("app").textContent = hello("world");
`);
// Binary handoff: the plugin's documented `pythsBin` option (auto-detect
// only probes CWD-relative cargo dirs + PATH, which a fresh project lacks).
writeFileSync(join(app, "vite.config.js"), `import pyths from "vite-plugin-pyths";

export default {
    plugins: [pyths({ pythsBin: process.env.PYTHS_BIN, emitDts: false })],
};
`);

const install = sh(
    // vite pinned to the stable rollup-based major: vite@8 (rolldown) has a
    // flaky native-binding optional-dep install on Windows runners.
    `npm install --no-audit --no-fund vite@^7 "${tarballs["pyths-runtime"]}" "${tarballs["vite-plugin-pyths"]}"`,
    app,
);
if (install.status !== 0) {
    fail(`npm install (vite project): exit ${install.status}\n${install.stderr.slice(-800)}`);
    finish();
}
ok("npm install vite + runtime/vite-plugin tarballs");

const build = sh("npm run build", app, { PYTHS_BIN: PYTHS });
if (build.status !== 0) {
    fail(`vite build: exit ${build.status}\n${(build.stderr + build.stdout).slice(-1200)}`);
    finish();
}
ok("vite build succeeded");

const assetsDir = join(app, "dist", "assets");
const bundle = readdirSync(assetsDir)
    .filter((f) => f.endsWith(".js"))
    .map((f) => readFileSync(join(assetsDir, f), "utf-8"))
    .join("\n");
if (bundle.includes(MARKER)) ok("compiled .ps component (marker) present in dist bundle");
else fail(`marker "${MARKER}" not found in dist/assets/*.js`);
// Minification renames pyLen/pyStr, but the runtime's property-name strings
// (`__pytuple__` in pyRepr, `__str__` probing in pyStr) survive minification
// and exist nowhere else in the app.
if (bundle.includes("__pytuple__") || bundle.includes("__str__") || bundle.includes("pyLen"))
    ok("pyths-runtime code bundled from tarball install");
else fail("pyths-runtime code not found in bundle — runtime tarball didn't resolve");

// ---------------------------------------------------------------------------
// 3. create-pyths-app from its tarball
// ---------------------------------------------------------------------------

console.log("\n── create-pyths-app scaffold (from tarball) ──");
const host = join(stage, "scaffold-host");
mkdirSync(host);
writeFileSync(join(host, "package.json"), JSON.stringify({
    name: "scaffold-host", private: true, version: "0.0.0",
}, null, 2));

const cInstall = sh(`npm install --no-audit --no-fund "${tarballs["create-pyths-app"]}"`, host);
if (cInstall.status !== 0) {
    fail(`npm install create-pyths-app tarball: exit ${cInstall.status}\n${cInstall.stderr.slice(-400)}`);
    finish();
}
// Run the scaffold bin exactly as npx would resolve it.
const scaffold = sh(`node node_modules/create-pyths-app/index.js demo-app`, host);
if (scaffold.status !== 0) {
    fail(`create-pyths-app scaffold: exit ${scaffold.status}\n${scaffold.stderr.slice(-400)}`);
    finish();
}
ok("scaffold ran (demo-app)");

const gen = join(host, "demo-app");
const expected = [
    "package.json",
    "next.config.mjs",
    "app/layout.ps",
    "app/page.ps",
    "components/header.ps",
];
for (const f of expected) {
    if (existsSync(join(gen, f))) ok(`generated ${f}`);
    else fail(`generated file missing: ${f}`);
}

// The generated entry (and the other .ps files) must compile with the real
// binary. Full `npm install` of the scaffold's next/react deps is skipped —
// compile verification needs only the compiler.
for (const f of ["app/page.ps", "app/layout.ps", "components/header.ps"]) {
    const c = runBin(PYTHS, ["compile", f, "--stdout"], gen);
    if (c.status === 0) ok(`pyths compile ${f}`);
    else fail(`pyths compile ${f}: ${c.stderr.trim().split("\n")[0]}`);
}

finish();

// ---------------------------------------------------------------------------

function finish() {
    try { rmSync(stage, { recursive: true, force: true }); } catch { /* Windows file locks */ }
    console.log("\n──────────────────────────────────────────");
    if (failed) {
        console.log("S6 packaging suite: FAILED");
        process.exit(1);
    }
    console.log("S6 packaging suite: GREEN");
    process.exit(0);
}
