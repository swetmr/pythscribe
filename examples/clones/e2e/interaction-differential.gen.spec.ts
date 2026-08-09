import { test, expect } from '@playwright/test'
import type { Browser, TestInfo } from '@playwright/test'
import { mkdirSync, readFileSync, writeFileSync } from 'node:fs'
import path from 'node:path'
import { LockstepFailure, isDifferentialFinding, runLockstep } from './lib/lockstep'
import {
  type ActionDesc,
  type GeneratedWalk,
  type WalkScriptBase,
  generateWalk,
  walkToScript,
} from './lib/walk-gen'
import { formatReproducer, shrinkWalk } from './lib/shrink'
import { kanbanScript } from './interactions/kanban'
import { twitterScript } from './interactions/twitter'
import { spotifyScript } from './interactions/spotify'
import { youtubeScript } from './interactions/youtube'
import { netflixScript } from './interactions/netflix'
import { courseraScript } from './interactions/coursera'
import { game2048Script } from './interactions/2048'
import { tetrisScript } from './interactions/tetris'

// Interaction differential v3 — GENERATED interaction walks.
//
// Instead of the six hand-authored scripts, this spec GENERATES a bounded
// seeded-random interaction walk per {clone, seed}: interactive elements are
// discovered from the LIVE React ORACLE page (lib/walk-gen.ts), a mulberry32
// PRNG picks actions, and the recorded data-only Action[] is replayed in
// lockstep on /<clone> vs /react-reference/<clone> through the UNCHANGED v2
// runner (lib/lockstep.ts: per-step DOM byte-diff + per-step error
// differential + freeze probe).
//
// On any differential finding the walk is SHRUNK to a minimal reproducer
// (lib/shrink.ts: prefix shrink + ddmin-lite + confirm re-run); the test
// fails with the minimal Step[] + seed and attaches a replayable JSON.
//
// CI FOOTPRINT — deliberately tiny by default:
//   * default run = ONE fixed-seed deterministic smoke walk on kanban
//     (burn-in verified with --repeat-each=3). No randomness in CI: the walk
//     is a pure function of (seed, app build).
//   * the multi-clone / multi-seed sweep is OPT-IN:
//       PYTHS_E2E_WALKS=<seeds-per-clone> \
//         npx playwright test --config e2e/playwright.vite.config.ts \
//         e2e/interaction-differential.gen.spec.ts
//     with optional PYTHS_E2E_WALK_CLONES=kanban,twitter (csv filter),
//     PYTHS_E2E_WALK_STEPS=<n> (walk length, default per-clone),
//     PYTHS_E2E_WALK_SEED=<base> (seeds are base..base+n-1, default 1).
//   * PYTHS_E2E_WALK_DETERMINISM=1 adds a same-seed-twice generation test.
//   * PYTHS_E2E_WALK_REPRO=<file.json> replays a previously attached
//     minimal-reproducer JSON ({clone, seed, actions}).

interface GenCloneConfig {
  base: WalkScriptBase
  steps: number
  advanceClockMs?: number
  excludeTestIds?: string[]
}

// Bases reuse the hand-authored scripts' clone/useClock/volatile/ready —
// the generated walk replaces only the Step[].
const GEN_CLONES: Record<string, GenCloneConfig> = {
  kanban: { base: kanbanScript, steps: 12 },
  // 4000ms flushes SEND_MS (600) + TOAST_MS (3000) every step → each step
  // lands on a quiescent, diffable state.
  twitter: { base: twitterScript, steps: 12, advanceClockMs: 4000 },
  spotify: { base: spotifyScript, steps: 12 },
  youtube: { base: youtubeScript, steps: 12 },
  netflix: { base: netflixScript, steps: 12 },
  coursera: { base: courseraScript, steps: 12 },
  '2048': { base: game2048Script, steps: 12 },
  // 1000ms fires exactly one gravity step per action (React defers the
  // re-render + reschedule off the fake-timer loop), keeping each generated
  // step on a quiescent, diffable state.
  tetris: { base: tetrisScript, steps: 12, advanceClockMs: 1000 },
}

const INCLUDE_PSC = !!process.env.PYTHS_E2E_PSC

// PYTHS_E2E_WALK_DUMP=<dir>: also write walk/repro JSONs to files (test
// attachments are only kept in the report; a dump dir makes them diffable).
function dump(name: string, body: string): void {
  const dir = process.env.PYTHS_E2E_WALK_DUMP
  if (!dir) return
  try {
    mkdirSync(dir, { recursive: true })
    writeFileSync(path.join(dir, name), body)
  } catch {
    /* dump is best-effort */
  }
}

async function generateForClone(
  browser: Browser,
  clone: string,
  cfg: GenCloneConfig,
  seed: number,
  steps: number,
): Promise<GeneratedWalk> {
  const context = await browser.newContext()
  try {
    const page = await context.newPage()
    if (cfg.base.useClock) await page.clock.install()
    await page.goto(`/react-reference/${clone}`)
    await cfg.base.ready(page)
    return await generateWalk(page, {
      clone,
      seed,
      steps,
      useClock: cfg.base.useClock,
      advanceClockMs: cfg.advanceClockMs,
      excludeTestIds: cfg.excludeTestIds,
    })
  } finally {
    await context.close().catch(() => {})
  }
}

async function replayWalk(
  browser: Browser,
  cfg: GenCloneConfig,
  actions: ActionDesc[],
  testInfo: TestInfo,
  quiet: boolean,
): Promise<LockstepFailure | null> {
  const script = walkToScript(cfg.base, actions, cfg.advanceClockMs)
  try {
    await runLockstep(browser, script, testInfo, { includePsc: INCLUDE_PSC, quiet })
    return null
  } catch (err) {
    if (err instanceof LockstepFailure) return err
    throw err // infra error — not a differential result
  }
}

async function runWalkTest(
  browser: Browser,
  testInfo: TestInfo,
  clone: string,
  seed: number,
  stepsOverride?: number,
): Promise<void> {
  const cfg = GEN_CLONES[clone]
  const steps = stepsOverride ?? cfg.steps

  // 1. generate against the oracle
  const walk = await generateForClone(browser, clone, cfg, seed, steps)
  const walkJson = JSON.stringify(walk, null, 2)
  await testInfo.attach(`walk-${clone}-seed${seed}.json`, {
    contentType: 'application/json',
    body: walkJson,
  })
  dump(`walk-${clone}-seed${seed}.json`, walkJson)
  expect(walk.actions.length, 'generator found interactive elements').toBeGreaterThan(0)

  // 2. lockstep replay through the v2 runner
  const failure = await replayWalk(browser, cfg, walk.actions, testInfo, false)
  if (!failure) return // parity holds along this walk

  const isFinding: boolean = isDifferentialFinding(failure)
  if (!isFinding) {
    throw new Error(
      `[${clone}] generated walk (seed ${seed}) could not be replayed on the ORACLE track — ` +
        `generation/replay nondeterminism, not a differential finding:\n${failure.message}`,
    )
  }

  // 3. shrink to the minimal reproducer, confirm, report
  const result = await shrinkWalk(
    (actions) => replayWalk(browser, cfg, actions, testInfo, true),
    walk.actions,
    failure,
  )
  const reproJson = JSON.stringify({ clone, seed, actions: result.minimal }, null, 2)
  await testInfo.attach(`repro-${clone}-seed${seed}.json`, {
    contentType: 'application/json',
    body: reproJson,
  })
  dump(`repro-${clone}-seed${seed}.json`, reproJson)
  throw new Error(
    formatReproducer(clone, seed, result.minimal, result.failure) +
      `\n\nshrink log (${result.replays} replays):\n  ` +
      result.log.join('\n  '),
  )
}

// ---------------------------------------------------------------------------
// Default CI set — ONE fixed-seed deterministic smoke walk (kanban).
// ---------------------------------------------------------------------------
const SMOKE = { clone: 'kanban', seed: 20260801, steps: 10 }

test(`generated walk smoke — ${SMOKE.clone} (fixed seed ${SMOKE.seed}, lockstep ps vs react oracle)`, async ({ browser }, testInfo) => {
  test.setTimeout(300_000)
  await runWalkTest(browser, testInfo, SMOKE.clone, SMOKE.seed, SMOKE.steps)
})

// ---------------------------------------------------------------------------
// Opt-in sweep — PYTHS_E2E_WALKS=<seeds-per-clone>
// ---------------------------------------------------------------------------
const SWEEP_SEEDS = Number(process.env.PYTHS_E2E_WALKS || 0)
if (SWEEP_SEEDS > 0) {
  const filter = (process.env.PYTHS_E2E_WALK_CLONES || Object.keys(GEN_CLONES).join(','))
    .split(',')
    .map((s) => s.trim())
    .filter((s) => s in GEN_CLONES)
  const baseSeed = Number(process.env.PYTHS_E2E_WALK_SEED || 1)
  const stepsOverride = process.env.PYTHS_E2E_WALK_STEPS
    ? Number(process.env.PYTHS_E2E_WALK_STEPS)
    : undefined

  for (const clone of filter) {
    for (let s = 0; s < SWEEP_SEEDS; s++) {
      const seed = baseSeed + s
      test(`generated walk sweep — ${clone} seed ${seed}`, async ({ browser }, testInfo) => {
        test.setTimeout(600_000)
        await runWalkTest(browser, testInfo, clone, seed, stepsOverride)
      })
    }
  }
}

// ---------------------------------------------------------------------------
// Opt-in determinism check — PYTHS_E2E_WALK_DETERMINISM=1
// same seed, two independent generation passes → byte-identical Action[].
// ---------------------------------------------------------------------------
if (process.env.PYTHS_E2E_WALK_DETERMINISM) {
  const targets = (process.env.PYTHS_E2E_WALK_CLONES || 'kanban')
    .split(',')
    .map((s) => s.trim())
    .filter((s) => s in GEN_CLONES)
  for (const clone of targets) {
    test(`generated walk determinism — ${clone} (same seed twice => identical Step[])`, async ({ browser }, testInfo) => {
      test.setTimeout(300_000)
      const cfg = GEN_CLONES[clone]
      const a = await generateForClone(browser, clone, cfg, SMOKE.seed, cfg.steps)
      const b = await generateForClone(browser, clone, cfg, SMOKE.seed, cfg.steps)
      const aJson = JSON.stringify(a, null, 2)
      const bJson = JSON.stringify(b, null, 2)
      await testInfo.attach(`determinism-${clone}-run1.json`, {
        contentType: 'application/json',
        body: aJson,
      })
      await testInfo.attach(`determinism-${clone}-run2.json`, {
        contentType: 'application/json',
        body: bJson,
      })
      dump(`determinism-${clone}-run1.json`, aJson)
      dump(`determinism-${clone}-run2.json`, bJson)
      expect(bJson).toBe(aJson)
    })
  }
}

// ---------------------------------------------------------------------------
// Opt-in reproducer replay — PYTHS_E2E_WALK_REPRO=<file.json>
// Replays an attached minimal-reproducer JSON ({clone, seed, actions}).
// ---------------------------------------------------------------------------
const REPRO_FILE = process.env.PYTHS_E2E_WALK_REPRO
if (REPRO_FILE) {
  const repro = JSON.parse(readFileSync(REPRO_FILE, 'utf8')) as {
    clone: string
    seed: number
    actions: ActionDesc[]
  }
  test(`generated walk reproducer — ${repro.clone} seed ${repro.seed} (${repro.actions.length} steps)`, async ({ browser }, testInfo) => {
    test.setTimeout(300_000)
    const failure = await replayWalk(browser, GEN_CLONES[repro.clone], repro.actions, testInfo, false)
    if (failure) throw new Error(formatReproducer(repro.clone, repro.seed, repro.actions, failure))
  })
}
