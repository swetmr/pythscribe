import type { Browser, BrowserContext, Page, TestInfo } from '@playwright/test'
import {
  RuntimeCapture,
  attachRuntimeCapture,
  freezeProbe,
  isAllowlisted,
} from './runtime-capture'
import { domSnapshot, firstDiff } from './dom-snapshot'
import type { InteractionScript, Step } from '../interactions/types'

// Lockstep multi-track runner — the interaction-differential v2 core,
// extracted from interaction-differential.spec.ts so the v3 GENERATED-walk
// spec (interaction-differential.gen.spec.ts) can reuse the exact same
// DOM-diff / error-differential / freeze machinery instead of reinventing it.
//
// For one InteractionScript it opens the React oracle route and the compiled
// .ps route (optionally the .psc route) in SEPARATE contexts of the same
// browser and, after the initial render and after every step, asserts:
//
//   1. freeze probe        — evaluate + rAF round-trip, 2s budget per page
//   2. error differential  — ps-track errors minus allowlist minus oracle
//   3. per-step DOM diff   — injected serializer over <body>, byte-compared,
//                            one settle-retry to absorb transient races
//
// Every failure is thrown as a typed LockstepFailure so callers (the shrink
// loop in particular) can distinguish a genuine differential finding from an
// invalid walk (a step that cannot even be performed on the ORACLE page).

export type FailureKind =
  | 'freeze'
  | 'error-differential'
  | 'dom-divergence'
  | 'step-failure' // step.run / step.settle itself failed on some track

export type TrackKind = 'oracle' | 'ps' | 'psc'

export class LockstepFailure extends Error {
  readonly kind: FailureKind
  /** Track the failure was observed on ('oracle' for dom-divergence = the comparison itself). */
  readonly track: TrackKind
  /** Index into script.steps; -1 = initial render. */
  readonly stepIndex: number
  readonly stepLabel: string

  constructor(
    message: string,
    props: { kind: FailureKind; track: TrackKind; stepIndex: number; stepLabel: string },
  ) {
    super(message)
    this.name = 'LockstepFailure'
    this.kind = props.kind
    this.track = props.track
    this.stepIndex = props.stepIndex
    this.stepLabel = props.stepLabel
  }
}

/** A differential finding = reproducible evidence of cross-track divergence.
 *  A step-failure on the ORACLE track is NOT one (the walk itself is invalid
 *  there — e.g. a shrink candidate removed a step a later step depended on). */
export function isDifferentialFinding(err: unknown): err is LockstepFailure {
  return err instanceof LockstepFailure && !(err.kind === 'step-failure' && err.track === 'oracle')
}

export interface Track {
  kind: TrackKind
  path: string
  context: BrowserContext
  page: Page
  capture: RuntimeCapture
}

export interface LockstepOpts {
  /** Also drive the compiled .psc route as a third track. */
  includePsc?: boolean
  /** Skip testInfo.attach on failure (shrink candidate runs). */
  quiet?: boolean
}

export async function defaultSettle(page: Page, useClock: boolean): Promise<void> {
  if (page.isClosed()) return
  try {
    if (useClock) await page.clock.runFor(0)
    else
      await page.evaluate(
        () => new Promise<void>((r) => requestAnimationFrame(() => requestAnimationFrame(() => r()))),
      )
  } catch {
    /* navigation race — the step's own settle assertions are authoritative */
  }
}

async function snapshotAll(tracks: Track[], script: InteractionScript): Promise<string[]> {
  const opts = { volatileValueTestIds: script.volatileValueTestIds }
  return Promise.all(tracks.map((t) => domSnapshot(t.page, opts)))
}

async function assertStepParity(
  tracks: Track[],
  script: InteractionScript,
  stepIndex: number,
  stepName: string,
  errBaseline: number[],
  testInfo: TestInfo,
  quiet: boolean,
): Promise<void> {
  const oracle = tracks[0]

  // (3) freeze probe first — a frozen page would hang the snapshot evaluate.
  for (const t of tracks) {
    const state = await freezeProbe(t.page, { useRaf: !script.useClock })
    if (state === 'frozen') {
      throw new LockstepFailure(
        `[${script.clone}] FROZE at step "${stepName}" on the ${t.kind} track (${t.path}): ` +
          `main thread unresponsive within 2s`,
        { kind: 'freeze', track: t.kind, stepIndex, stepLabel: stepName },
      )
    }
  }

  // (2) per-step error differential: new errors on compiled tracks, minus
  // allowlist, minus everything the oracle has emitted.
  for (const t of tracks) await t.capture.drain()
  const oracleAll = oracle.capture.normalizedSet()
  for (let i = 0; i < tracks.length; i++) {
    const t = tracks[i]
    if (t.kind === 'oracle') continue
    const fresh = t.capture.errors.slice(errBaseline[i])
    const residual = [...new Set(fresh.map((e) => e.normalized))].filter(
      (n) => !isAllowlisted(n, script.clone) && !oracleAll.includes(n),
    )
    if (residual.length > 0) {
      const detail = residual
        .map((n) => {
          const raw = fresh.find((e) => e.normalized === n)
          return `  [${raw?.source}] ${n}`
        })
        .join('\n')
      if (!quiet) {
        await testInfo.attach(`errors-${script.clone}-${t.kind}`, {
          contentType: 'application/json',
          body: JSON.stringify(fresh, null, 2),
        })
      }
      throw new LockstepFailure(
        `[${script.clone}] runtime-error differential at step "${stepName}" ` +
          `(${t.kind} track has ${residual.length} error(s) the React oracle does not):\n${detail}`,
        { kind: 'error-differential', track: t.kind, stepIndex, stepLabel: stepName },
      )
    }
  }

  // (1) per-step DOM diff, with one settle-retry to absorb transient races
  // (e.g. media metadata attributes landing a frame apart).
  let snaps = await snapshotAll(tracks, script)
  for (let i = 1; i < tracks.length; i++) {
    if (snaps[i] === snaps[0]) continue
    await tracks[0].page.waitForTimeout(300)
    for (const t of tracks) await defaultSettle(t.page, !!script.useClock)
    snaps = await snapshotAll(tracks, script)
    if (snaps[i] === snaps[0]) continue

    const diff = firstDiff(snaps[i], snaps[0])
    if (!quiet) {
      await testInfo.attach(`dom-${script.clone}-${stepName}-oracle.json`, {
        contentType: 'application/json',
        body: snaps[0],
      })
      await testInfo.attach(`dom-${script.clone}-${stepName}-${tracks[i].kind}.json`, {
        contentType: 'application/json',
        body: snaps[i],
      })
    }
    throw new LockstepFailure(
      `[${script.clone}] DOM divergence at step "${stepName}" (${tracks[i].kind} vs oracle): ` +
        `first diff at ${diff?.path}\n  ${tracks[i].kind}: ${JSON.stringify(diff?.a)}\n  oracle: ${JSON.stringify(diff?.b)}\n` +
        `(full serialized trees attached)`,
      { kind: 'dom-divergence', track: tracks[i].kind, stepIndex, stepLabel: stepName },
    )
  }
}

export async function runLockstep(
  browser: Browser,
  script: InteractionScript,
  testInfo: TestInfo,
  opts: LockstepOpts = {},
): Promise<void> {
  const quiet = !!opts.quiet
  const trackDefs: { kind: TrackKind; path: string }[] = [
    { kind: 'oracle', path: `/react-reference/${script.clone}` },
    { kind: 'ps', path: `/${script.clone}` },
    ...(opts.includePsc ? [{ kind: 'psc' as const, path: `/${script.clone}-psc` }] : []),
  ]

  const tracks: Track[] = []
  try {
    for (const def of trackDefs) {
      const context = await browser.newContext()
      const page = await context.newPage()
      const capture = await attachRuntimeCapture(page)
      if (script.useClock) await page.clock.install()
      tracks.push({ ...def, context, page, capture })
    }

    for (const t of tracks) {
      await t.page.goto(t.path)
      await script.ready(t.page)
    }

    // Step 0 — initial render parity.
    await assertStepParity(tracks, script, -1, 'initial render', tracks.map(() => 0), testInfo, quiet)

    for (let s = 0; s < script.steps.length; s++) {
      const step: Step = script.steps[s]
      const label = `${s + 1}. ${step.name}`

      // Pre-drain so the per-step error window is exact (and so rejections
      // queued in window.__runtimeErrors survive reload steps).
      for (const t of tracks) await t.capture.drain()
      const errBaseline = tracks.map((t) => t.capture.errors.length)

      // Any run/settle failure is re-thrown naming the step, the track it
      // failed on, and the runtime errors captured during the step — so an
      // injected throw that breaks the flow is reported as a differential
      // finding, not an anonymous expect timeout.
      const enrich = async (t: Track, phase: string, err: unknown): Promise<never> => {
        await t.capture.drain()
        const idx = tracks.indexOf(t)
        const fresh = t.capture.errors.slice(errBaseline[idx]).map((e) => `  [${e.source}] ${e.normalized}`)
        throw new LockstepFailure(
          `[${script.clone}] step "${label}" ${phase} failed on the ${t.kind} track (${t.path})` +
            (fresh.length ? `\nruntime errors captured during this step:\n${fresh.join('\n')}` : '') +
            `\ncause: ${err instanceof Error ? err.message : String(err)}`,
          { kind: 'step-failure', track: t.kind, stepIndex: s, stepLabel: label },
        )
      }

      for (const t of tracks) {
        try {
          await step.run(t.page)
        } catch (err) {
          await enrich(t, 'interaction', err)
        }
      }
      if (script.useClock && step.advanceClock) {
        for (const t of tracks) await t.page.clock.runFor(step.advanceClock)
      }
      for (const t of tracks) {
        try {
          if (step.settle) await step.settle(t.page)
          else await defaultSettle(t.page, !!script.useClock)
        } catch (err) {
          await enrich(t, 'settle', err)
        }
      }

      if (step.skipDomDiff) continue
      await assertStepParity(tracks, script, s, label, errBaseline, testInfo, quiet)
    }
  } finally {
    for (const t of tracks) await t.context.close().catch(() => {})
  }
}
