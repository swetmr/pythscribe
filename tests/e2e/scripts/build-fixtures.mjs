// Recompile both .ps fixtures into .js so the harness loads fresh output.
// Invoked by Playwright's webServer? No — the compile step is intentionally
// out-of-band so the user can re-run after each codegen change.
//
// Usage:  node ./scripts/build-fixtures.mjs

import { execFileSync } from "node:child_process";
import path from "node:path";
import { fileURLToPath } from "node:url";

const __filename = fileURLToPath(import.meta.url);
const __dirname = path.dirname(__filename);
const REPO_ROOT = path.resolve(__dirname, "..", "..", "..");

const PYTHS_BIN = process.env.PYTHS_BIN
    || path.join(REPO_ROOT, "target", "release", process.platform === "win32" ? "pyths.exe" : "pyths");

const FIXTURES = [
    "examples/cloudflare-bench/large-samples/pythscribe/dashboard_500.ps",
    "examples/cloudflare-bench/large-samples/pythscribe/app_1000.ps",
];

for (const f of FIXTURES) {
    const abs = path.join(REPO_ROOT, f);
    console.log(`[e2e] compiling ${f} ...`);
    execFileSync(PYTHS_BIN, ["compile", abs], {
        stdio: "inherit",
        env: { ...process.env, PYTHS_NO_CACHE: "1" },
    });
}
console.log(`[e2e] done.`);
