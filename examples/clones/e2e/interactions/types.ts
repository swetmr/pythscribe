import type { Page } from '@playwright/test'

// Shared multi-step interaction script format (interaction-differential v2).
// ONE script per clone, authored against data-testids only — testids are
// shared across tracks by construction, so the same script drives the
// compiled .ps route, the React oracle route (and optionally the .psc route).

export interface Step {
  /** Human-readable step name — failure messages cite it. */
  name: string
  /** The interaction, run in lockstep on every track's page. */
  run: (page: Page) => Promise<void>
  /**
   * Quiescence marker: an expect()-based wait that defines "this step is
   * settled" (toast visible/gone, count updated, ...). Preferred over
   * wall-clock waits. Runs on every track after `run` (+ clock advance).
   */
  settle?: (page: Page) => Promise<void>
  /**
   * With useClock: milliseconds to advance BOTH pages' fake clocks after
   * `run` — drives setTimeout-based flows (optimistic sends, toast
   * auto-dismiss) deterministically.
   */
  advanceClock?: number
  /** Skip the per-step DOM byte-diff (error differential + freeze probe still run). */
  skipDomDiff?: boolean
}

export interface InteractionScript {
  /** Clone slug: routes are `/<clone>`, `/react-reference/<clone>`, `/<clone>-psc`. */
  clone: string
  /** Install Playwright's fake clock on every page before navigation. */
  useClock?: boolean
  /** Elements (by data-testid) whose live @value is excluded from snapshots. */
  volatileValueTestIds?: string[]
  /** Wait until the app is interactive; also the place for per-page setup. */
  ready: (page: Page) => Promise<void>
  steps: Step[]
}
