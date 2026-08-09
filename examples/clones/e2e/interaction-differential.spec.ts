import { test } from '@playwright/test'
import { runLockstep } from './lib/lockstep'
import type { InteractionScript } from './interactions/types'
import { kanbanScript } from './interactions/kanban'
import { twitterScript } from './interactions/twitter'
import { spotifyScript } from './interactions/spotify'
import { youtubeScript } from './interactions/youtube'
import { netflixScript } from './interactions/netflix'
import { courseraScript } from './interactions/coursera'
import { game2048Script } from './interactions/2048'
import { tetrisScript } from './interactions/tetris'

// Interaction differential (v2) — TREX-style runtime-state parity.
//
// For each interactive clone, ONE shared testid-targeted interaction script
// (e2e/interactions/<clone>.ts) is driven in LOCKSTEP against the compiled
// .ps production route and the React oracle route, opened in the same
// browser but SEPARATE contexts (no localStorage bleed). After every step:
//
//   1. per-step DOM diff — dependency-free serializer injected via
//      page.evaluate (lib/dom-snapshot.ts) over <body> (covers portals);
//      byte-compared, with one settle-retry to absorb transient races;
//      failures name the step + first differing JSON path and attach both
//      trees to the report.
//   2. per-step error differential — errors captured on the ps page during
//      the step, minus the per-clone allowlist, minus everything the oracle
//      page has emitted → must be empty; failures name the step + error.
//   3. freeze probe — evaluate + rAF round-trip (rAF skipped under the fake
//      clock) with a 2s budget per page; a hang reports "froze at step N"
//      instead of an opaque later timeout.
//
// The lockstep machinery itself lives in lib/lockstep.ts, shared with the
// v3 GENERATED-walk spec (interaction-differential.gen.spec.ts).
//
// Timer-driven fixtures (twitter: SEND_MS optimistic sends, TOAST_MS
// auto-dismiss) run under Playwright's page.clock on BOTH pages, advanced in
// lockstep via Step.advanceClock — mid-flight optimistic states become
// stable, diffable checkpoints.
//
// Set PYTHS_E2E_PSC=1 to additionally drive the compiled .psc route
// (/<clone>-psc) as a third lockstep track (off by default; verify:psc
// covers textual equivalence, this adds runtime equivalence).
//
// 'hello' is deliberately not scripted (trivial fixture — scoping doc §2b).

const SCRIPTS: InteractionScript[] = [
  kanbanScript,
  twitterScript,
  spotifyScript,
  youtubeScript,
  netflixScript,
  courseraScript,
  game2048Script,
  tetrisScript,
]

const INCLUDE_PSC = !!process.env.PYTHS_E2E_PSC

for (const script of SCRIPTS) {
  test(`interaction differential — ${script.clone} (lockstep ps vs react oracle${INCLUDE_PSC ? ' vs psc' : ''})`, async ({ browser }, testInfo) => {
    test.setTimeout(240_000)
    await runLockstep(browser, script, testInfo, { includePsc: INCLUDE_PSC })
  })
}
