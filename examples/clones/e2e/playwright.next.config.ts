import { defineConfig, devices } from '@playwright/test'
import { fileURLToPath } from 'node:url'
import path from 'node:path'

const here = path.dirname(fileURLToPath(import.meta.url))
const workspaceRoot = path.resolve(here, '..') // examples/clones/

// Next track — production build served via `next start` on a FIXED port
// (3999), distinct from the Vite track's port (4173). Run
// `npm run build -w next` first (wired into the root `e2e` script, which
// also runs precompile-client via the `prebuild` hook) — this config only
// serves.
export default defineConfig({
  testDir: '.',
  testMatch: '*.spec.ts',
  fullyParallel: true,
  forbidOnly: !!process.env.CI,
  retries: process.env.CI ? 2 : 0,
  workers: process.env.CI ? 1 : undefined,
  reporter: process.env.CI ? 'github' : 'list',
  use: {
    baseURL: 'http://localhost:3999',
    trace: 'on-first-retry',
  },
  webServer: {
    command: 'npm run start -w next',
    cwd: workspaceRoot,
    url: 'http://localhost:3999',
    reuseExistingServer: !process.env.CI,
    timeout: 30_000,
    stdout: 'pipe',
    stderr: 'pipe',
  },
  projects: [
    {
      name: 'chromium',
      use: { ...devices['Desktop Chrome'] },
    },
  ],
})
