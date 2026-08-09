import { LockstepFailure, isDifferentialFinding } from './lockstep'
import type { ActionDesc } from './walk-gen'
import { stepName } from './walk-gen'

// Shrink-to-minimal-reproducer (interaction-differential v3).
//
// A raw walk that diverges at step N is not an acceptable finding on its
// own — the deliverable is the MINIMAL replayable Step[]:
//
//   1. PREFIX shrink: the lockstep runner stops at the first failing step,
//      so actions[0..failIndex] is the shortest failing *prefix* — truncate
//      there (everything after the failure never ran and is noise).
//   2. DELTA-DEBUG (ddmin-lite): walking backwards from the last non-final
//      action, try removing each earlier action; keep the removal whenever
//      the reduced walk STILL produces a differential finding. One linear
//      pass, newest-first, which handles the common case (unrelated warm-up
//      steps before the trigger) in O(n) replays.
//   3. CONFIRM: the final minimal sequence is re-run once more; only a
//      confirmed-failing sequence is reported.
//
// Classification matters: a candidate whose removal makes a LATER step
// unactionable fails on the ORACLE track ('step-failure' + track 'oracle').
// That is an INVALID WALK, not a reproduction — isDifferentialFinding()
// filters it, so the shrinker never "reduces" into a walk that merely
// cannot run.

/** Replays actions[] in a fresh lockstep run; resolves to the differential
 *  failure, or null when the run passes OR is an invalid walk. */
export type ReplayFn = (actions: ActionDesc[]) => Promise<LockstepFailure | null>

export interface ShrinkResult {
  minimal: ActionDesc[]
  failure: LockstepFailure
  /** total lockstep replays spent (budget accounting). */
  replays: number
  log: string[]
}

export async function shrinkWalk(
  replay: ReplayFn,
  actions: ActionDesc[],
  initialFailure: LockstepFailure,
  opts: { maxReplays?: number } = {},
): Promise<ShrinkResult> {
  const maxReplays = opts.maxReplays ?? 40
  let replays = 0
  const log: string[] = []

  const tryRun = async (cand: ActionDesc[]): Promise<LockstepFailure | null> => {
    replays++
    return replay(cand)
  }

  // -- 1. prefix shrink ------------------------------------------------------
  // stepIndex -1 = the initial render already diverged: minimal repro is [].
  let current: ActionDesc[]
  let failure = initialFailure
  if (initialFailure.stepIndex < 0) {
    return { minimal: [], failure, replays, log: ['initial render diverges — empty walk reproduces'] }
  }
  current = actions.slice(0, initialFailure.stepIndex + 1)
  log.push(`prefix shrink: ${actions.length} -> ${current.length} steps (failed at step ${initialFailure.stepIndex})`)

  {
    const f = await tryRun(current)
    if (f && isDifferentialFinding(f)) {
      failure = f
      // The failure may move earlier on re-run only if the original run was
      // nondeterministic; trust the fresh index.
      if (f.stepIndex >= 0 && f.stepIndex + 1 < current.length) {
        current = current.slice(0, f.stepIndex + 1)
        log.push(`prefix re-tightened to ${current.length} steps`)
      }
    } else {
      log.push('WARNING: prefix did not reproduce — reporting the full failing walk unshrunk')
      return { minimal: actions, failure, replays, log }
    }
  }

  // -- 2. ddmin-lite: drop non-essential earlier steps, newest-first ---------
  // (the last action is the trigger — never dropped)
  for (let i = current.length - 2; i >= 0 && replays < maxReplays; i--) {
    const cand = current.slice(0, i).concat(current.slice(i + 1))
    const f = await tryRun(cand)
    if (f && isDifferentialFinding(f)) {
      log.push(`dropped step ${i} (${current[i].kind}) — still fails: ${f.kind} @ ${f.stepLabel}`)
      current = cand
      failure = f
    } else {
      log.push(`kept step ${i} (${current[i].kind}) — ${f ? 'non-differential' : 'passes'} without it`)
    }
  }

  // -- 3. confirm ------------------------------------------------------------
  const confirm = await tryRun(current)
  if (!confirm) {
    log.push('WARNING: minimal candidate no longer reproduces on confirm run — flaky reduction')
  } else {
    failure = confirm
  }

  return { minimal: current, failure, replays, log }
}

export function formatReproducer(clone: string, seed: number, minimal: ActionDesc[], failure: LockstepFailure): string {
  const steps = minimal.map((a, i) => '  ' + stepName(i, a)).join('\n')
  return (
    `MINIMAL REPRODUCER — clone=${clone} seed=${seed} (${minimal.length} step(s))\n` +
    `${steps || '  (empty — initial render diverges)'}\n` +
    `failure: [${failure.kind}] on ${failure.track} track at "${failure.stepLabel}"\n` +
    `${failure.message}\n` +
    `replay: PYTHS_E2E_WALK_REPRO=<path-to-attached-json> npx playwright test --config e2e/playwright.vite.config.ts e2e/interaction-differential.gen.spec.ts`
  )
}
