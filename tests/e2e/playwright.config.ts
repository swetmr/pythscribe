import { defineConfig, devices } from "@playwright/test";

// E2E configuration for PythScribe ↔ React parity tests.
//
// Goals:
// - Deterministic rendering: pinned viewport, animations disabled,
//   shared base.css so font fingerprints match the React reference.
// - Both DOM-bytecode and pixel-perfect parity checks run independently.
// - Per-OS screenshot baselines (the runner stores under
//   `__screenshots__/<spec-name>-<projectName>-<platform>/...`).
//
// Run:
//   pnpm test:browser              # standard test pass
//   pnpm test:browser:update       # refresh screenshot baselines
//   pnpm test:browser:headed       # visual debug

export default defineConfig({
  testDir: "./tests",
  fullyParallel: false, // sequential keeps logs readable when iterating
  forbidOnly: !!process.env.CI,
  retries: 0,
  workers: 1,

  reporter: [
    ["list"],
    ["json", { outputFile: "../target/e2e-failures.json" }],
  ],

  // esbuild-wasm in the React-reference harness is fetched from esm.sh
  // on cold cache and can take 20-30s to initialize. Bump the per-test
  // timeout so first-pass runs don't spuriously fail.
  timeout: 60_000,

  expect: {
    // Pixel-perfect screenshot comparison threshold. `threshold` is
    // per-pixel color delta tolerance (0..1); `maxDiffPixels` is the
    // upper bound on differing pixels. These values are intentionally
    // tight — larger means flakier, smaller means brittle.
    toHaveScreenshot: {
      threshold: 0.2,
      maxDiffPixels: 100,
      animations: "disabled",
      caret: "hide",
    },
  },

  use: {
    baseURL: "http://127.0.0.1:8765",
    viewport: { width: 1280, height: 800 },
    deviceScaleFactor: 1,
    // No video / no trace by default — turn on when triaging.
    video: "off",
    trace: "off",
    screenshot: "only-on-failure",
  },

  projects: [
    {
      name: "chromium",
      use: { ...devices["Desktop Chrome"] },
    },
  ],

  // Static file server for the harness pages and the compiled fixtures.
  webServer: {
    command: "node ./scripts/serve.mjs",
    url: "http://127.0.0.1:8765/harness/healthcheck.txt",
    reuseExistingServer: !process.env.CI,
    timeout: 30_000,
  },
});
