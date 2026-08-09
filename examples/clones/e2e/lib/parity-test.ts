import { test as base, expect } from '@playwright/test'
import { RuntimeCapture, attachRuntimeCapture, assertRuntimeClean, freezeProbe } from './runtime-capture'

// Drop-in base for the clone e2e specs (interaction-differential v1).
// Existing specs change ONE line — `import { test, expect } from './lib/parity-test'`
// — and every test transparently gains:
//   * console-error / pageerror / unhandled-rejection / crash capture
//   * a teardown zero-error policy with per-clone allowlist + react-oracle
//     subtraction (see runtime-capture.ts)
//   * a cheap main-thread freeze probe (evaluate + rAF round-trip)
// No test-body edits, no new interactions, no CI wiring (the config already
// globs *.spec.ts).

export const test = base.extend<{ runtimeCapture: RuntimeCapture }>({
  runtimeCapture: [
    async ({ page }, use, testInfo) => {
      const cap = await attachRuntimeCapture(page)
      await use(cap)

      // Teardown 1 — freeze probe: interactions this test performed must
      // leave the main thread responsive. Reported as its own named failure
      // instead of an opaque later timeout.
      const aliveness = await freezeProbe(page)

      // Teardown 2 — runtime-error policy (drains the rejection channel).
      await assertRuntimeClean(cap, page, testInfo)

      if (aliveness === 'frozen') {
        throw new Error(
          `Freeze probe: main thread unresponsive after "${testInfo.title}" ` +
            `(page ${cap.lastPathname}) — evaluate/rAF round-trip did not complete within 2s`,
        )
      }
    },
    { auto: true },
  ],
})

export { expect }
