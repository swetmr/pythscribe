import type { Locator, Page } from '@playwright/test'
import type { InteractionScript, Step } from '../interactions/types'
import { defaultSettle } from './lockstep'

// Generated interaction walks (interaction-differential v3).
//
// A walk is a bounded seeded-random sequence of actions discovered from the
// LIVE React ORACLE page: at each step the generator enumerates the page's
// currently-interactable elements (in-page evaluate, document order — see
// discoverActions), picks one with a seeded PRNG, executes it on the oracle
// (so the next discovery sees the post-action state), and records a purely
// DATA-shaped ActionDesc. The recorded Action[] is then replayed in lockstep
// on the .ps track and the oracle track via the UNCHANGED v2 runner
// (lib/lockstep.ts) — both tracks get the byte-identical script.
//
// Determinism: mulberry32 PRNG (no Math.random anywhere), discovery in
// document order, candidate targeting that never depends on wall-clock
// (testid+nth, or raw document-order index into the fixed candidate CSS
// query — legal cross-track because per-step DOM parity is what the runner
// asserts). Same seed + same app build => byte-identical walk.
//
// Timer-driven clones (twitter) generate AND replay under Playwright's fake
// clock; every generated step advances the clock by a fixed per-clone amount
// (genAdvanceClockMs) large enough to flush SEND_MS sends + TOAST_MS toasts,
// so each step lands on a quiescent, diffable state.

// ---------------------------------------------------------------------------
// Seeded PRNG — mulberry32. Tiny, well-distributed, fully reproducible.
// ---------------------------------------------------------------------------
export function mulberry32(seed: number): () => number {
  let a = seed >>> 0
  return () => {
    a = (a + 0x6d2b79f5) | 0
    let t = Math.imul(a ^ (a >>> 15), 1 | a)
    t = (t + Math.imul(t ^ (t >>> 7), 61 | t)) ^ t
    return ((t ^ (t >>> 14)) >>> 0) / 4294967296
  }
}

export function pick<T>(rng: () => number, arr: T[]): T {
  return arr[Math.floor(rng() * arr.length)]
}

// ---------------------------------------------------------------------------
// Action model — pure data, JSON-serializable, so a failing walk's minimal
// reproducer can be attached to the report and replayed exactly.
// ---------------------------------------------------------------------------

/** How to find the element again, identically on every track. */
export type TargetDesc =
  | { by: 'testid'; testid: string; nth: number }
  | { by: 'index'; nth: number } // nth match of CANDIDATE_QUERY in document order

export type ActionDesc =
  | { kind: 'click'; target: TargetDesc; label: string }
  | { kind: 'fill'; target: TargetDesc; value: string; label: string }
  | { kind: 'press'; target: TargetDesc; key: string; label: string }
  | { kind: 'select'; target: TargetDesc; value: string; label: string }

export interface GeneratedWalk {
  clone: string
  seed: number
  actions: ActionDesc[]
}

// Fixed candidate query — MUST stay in sync between discoverActions (in-page)
// and locatorFor ('index' targets replay as page.locator(QUERY).nth(n);
// querySelectorAll on a selector list returns unique elements in document
// order, exactly like Playwright's .nth over the same selector list).
export const CANDIDATE_QUERY =
  'button, a[href], input, textarea, select, summary, [role="button"], [role="checkbox"], [role="tab"], [role="menuitem"], [role="link"], [data-testid]'

interface Candidate {
  target: TargetDesc
  /** 'click' | 'fill' | 'select' — expanded into concrete actions below. */
  op: 'click' | 'fill' | 'select'
  /** For selects: the option values to choose from. */
  options?: string[]
  /** Short human label: tag, testid and/or trimmed text. */
  label: string
}

export interface DiscoverOpts {
  /** Clone slug — <a href> candidates must stay under /<clone> (oracle: /react-reference/<clone>). */
  clone: string
  /** data-testids (exact) to never touch. */
  excludeTestIds?: string[]
}

/**
 * Enumerate currently-interactable elements on the page, in document order.
 * Runs entirely in-page; returns pure data.
 *
 * Included: buttons, in-clone links, text inputs/textareas, checkboxes/radios,
 * selects, ARIA click-roles, plus any [data-testid] element whose computed
 * cursor is 'pointer' (the clones' idiom for clickable divs — kanban cards,
 * carousel tiles, ...). Excluded: invisible (no client rects), disabled,
 * aria-hidden subtrees, file inputs, external/target=_blank/download links,
 * and anything in excludeTestIds.
 */
export async function discoverActions(page: Page, opts: DiscoverOpts): Promise<Candidate[]> {
  return page.evaluate(
    ({ query, clone, exclude }) => {
      const out: Candidate[] = []
      const all = Array.from(document.querySelectorAll<HTMLElement>(query))
      const testidCount: Record<string, number> = {}

      const clip = (s: string) => s.replace(/\s+/g, ' ').trim().slice(0, 48)

      for (let i = 0; i < all.length; i++) {
        const el = all[i]
        const tag = el.tagName
        const testid = el.getAttribute('data-testid') || ''
        // nth among elements sharing this testid (getByTestId(...).nth(n));
        // count BEFORE any filtering so the index is purely structural.
        let tidNth = 0
        if (testid) {
          tidNth = testidCount[testid] || 0
          testidCount[testid] = tidNth + 1
        }

        if (exclude.indexOf(testid) !== -1) continue
        if (el.getClientRects().length === 0) continue
        if ((el as HTMLButtonElement).disabled) continue
        if (el.closest('[aria-hidden="true"]')) continue

        const target: TargetDesc = testid
          ? { by: 'testid', testid, nth: tidNth }
          : { by: 'index', nth: i }
        const role = el.getAttribute('role') || ''
        const labelText =
          el.getAttribute('aria-label') ||
          clip(el.textContent || '') ||
          el.getAttribute('placeholder') ||
          ''
        const label =
          `<${tag.toLowerCase()}>` + (testid ? ` [${testid}]` + (tidNth ? `#${tidNth}` : '') : '') +
          (labelText ? ` "${clip(labelText)}"` : '')

        if (tag === 'SELECT') {
          const options = Array.from((el as HTMLSelectElement).options)
            .filter((o) => !o.disabled)
            .map((o) => o.value)
          if (options.length > 1) out.push({ target, op: 'select', options, label })
          continue
        }
        if (tag === 'INPUT') {
          const type = ((el as HTMLInputElement).type || 'text').toLowerCase()
          if (type === 'file' || type === 'hidden' || type === 'range' || type === 'color') continue
          if (type === 'checkbox' || type === 'radio' || type === 'button' || type === 'submit') {
            out.push({ target, op: 'click', label })
          } else {
            out.push({ target, op: 'fill', label })
          }
          continue
        }
        if (tag === 'TEXTAREA') {
          out.push({ target, op: 'fill', label })
          continue
        }
        if (tag === 'A') {
          const a = el as HTMLAnchorElement
          if (a.target === '_blank' || a.hasAttribute('download')) continue
          let path: string
          try {
            const u = new URL(a.href, location.href)
            if (u.origin !== location.origin) continue
            path = u.pathname
          } catch {
            continue
          }
          // stay inside the clone on every track: strip the oracle prefix,
          // then require /<clone> (or /<clone>/...)
          const p = path.replace(/^\/react-reference/, '')
          if (p !== `/${clone}` && p.indexOf(`/${clone}/`) !== 0) continue
          out.push({ target, op: 'click', label })
          continue
        }
        if (tag === 'BUTTON' || tag === 'SUMMARY') {
          out.push({ target, op: 'click', label })
          continue
        }
        if (['button', 'checkbox', 'tab', 'menuitem', 'link'].indexOf(role) !== -1) {
          out.push({ target, op: 'click', label })
          continue
        }
        // Bare [data-testid] element: clickable-div idiom only.
        if (testid && getComputedStyle(el).cursor === 'pointer') {
          out.push({ target, op: 'click', label })
        }
      }
      return out
    },
    { query: CANDIDATE_QUERY, clone: opts.clone, exclude: opts.excludeTestIds ?? [] },
  )
}

// ---------------------------------------------------------------------------
// Locators + execution — shared by generation (oracle) and replay (all tracks)
// ---------------------------------------------------------------------------
export function locatorFor(page: Page, target: TargetDesc): Locator {
  if (target.by === 'testid') return page.getByTestId(target.testid).nth(target.nth)
  return page.locator(CANDIDATE_QUERY).nth(target.nth)
}

/** Per-action timeout — fail FAST (an unactionable target on a shrink
 *  candidate must not eat the whole test budget). */
export const ACTION_TIMEOUT_MS = 5_000

export async function performAction(page: Page, action: ActionDesc): Promise<void> {
  const loc = locatorFor(page, action.target)
  switch (action.kind) {
    case 'click':
      return loc.click({ timeout: ACTION_TIMEOUT_MS })
    case 'fill':
      return loc.fill(action.value, { timeout: ACTION_TIMEOUT_MS })
    case 'press':
      return loc.press(action.key, { timeout: ACTION_TIMEOUT_MS })
    case 'select':
      return void (await loc.selectOption(action.value, { timeout: ACTION_TIMEOUT_MS }))
  }
}

// Deterministic fill corpus. '#fail' is IN the corpus on purpose — the
// twitter clone's failure/rollback path triggers on it, identically on both
// tracks, so generated walks exercise the optimistic-rollback surface.
const WORDS = [
  'parity', 'walk', 'lockstep', 'oracle', 'compile', 'python', 'rust',
  'seeded', 'shrink', 'diff', 'tier7', 'wasm', '#fail', 'quiescent',
]

function fillValue(rng: () => number): string {
  const n = 1 + Math.floor(rng() * 3)
  const words: string[] = []
  for (let i = 0; i < n; i++) words.push(pick(rng, WORDS))
  return words.join(' ')
}

// ---------------------------------------------------------------------------
// Walk generation
// ---------------------------------------------------------------------------
export interface GenConfig {
  clone: string
  seed: number
  /** Target number of steps (walk may be shorter if the page runs out of actions). */
  steps: number
  /** Fake-clock advance after every step (clones with useClock). */
  advanceClockMs?: number
  useClock?: boolean
  excludeTestIds?: string[]
}

/**
 * Drive the ORACLE page with seeded-random actions, recording each executed
 * action as data. The page must already be at the oracle route with the
 * clone's ready() satisfied. Fully deterministic for a given seed + build.
 */
export async function generateWalk(page: Page, cfg: GenConfig): Promise<GeneratedWalk> {
  const rng = mulberry32(cfg.seed)
  const actions: ActionDesc[] = []

  while (actions.length < cfg.steps) {
    const candidates = await discoverActions(page, {
      clone: cfg.clone,
      excludeTestIds: cfg.excludeTestIds,
    })
    if (candidates.length === 0) break

    // Try a few PRNG picks in case the chosen element is unactionable
    // (covered by an overlay, detached mid-hover, ...). Failed attempts
    // consume PRNG draws deterministically and are NOT recorded.
    let executed: ActionDesc | null = null
    for (let attempt = 0; attempt < 3 && !executed; attempt++) {
      const cand = pick(rng, candidates)
      const action = concretize(cand, rng)
      try {
        await performAction(page, action)
        executed = action
      } catch {
        // unactionable right now — draw again
      }
    }
    if (!executed) break
    actions.push(executed)

    // Settle exactly like the replay runner will: fixed clock advance under
    // useClock, double-rAF otherwise.
    if (cfg.useClock) {
      await page.clock.runFor(cfg.advanceClockMs ?? 0)
    }
    await defaultSettle(page, !!cfg.useClock)
  }

  return { clone: cfg.clone, seed: cfg.seed, actions }
}

function concretize(cand: Candidate, rng: () => number): ActionDesc {
  switch (cand.op) {
    case 'select': {
      const value = pick(rng, cand.options ?? [''])
      return { kind: 'select', target: cand.target, value, label: cand.label }
    }
    case 'fill': {
      // ~25% of text-input picks press Enter instead (submit idiom:
      // kanban composer/rename, search boxes...). Draw order is fixed:
      // branch draw first, then the value draw when filling.
      if (rng() < 0.25) {
        return { kind: 'press', target: cand.target, key: 'Enter', label: cand.label }
      }
      return { kind: 'fill', target: cand.target, value: fillValue(rng), label: cand.label }
    }
    case 'click':
      return { kind: 'click', target: cand.target, label: cand.label }
  }
}

// ---------------------------------------------------------------------------
// Action[] -> InteractionScript (the v2 runner's native input)
// ---------------------------------------------------------------------------
export interface WalkScriptBase {
  clone: string
  useClock?: boolean
  volatileValueTestIds?: string[]
  ready: InteractionScript['ready']
}

export function stepName(i: number, a: ActionDesc): string {
  const what =
    a.kind === 'fill' ? `fill ${a.label} = ${JSON.stringify(a.value)}`
    : a.kind === 'press' ? `press ${a.key} on ${a.label}`
    : a.kind === 'select' ? `select ${JSON.stringify(a.value)} in ${a.label}`
    : `click ${a.label}`
  return `walk[${i}] ${what}`
}

export function walkToScript(
  base: WalkScriptBase,
  actions: ActionDesc[],
  advanceClockMs?: number,
): InteractionScript {
  const steps: Step[] = actions.map((a, i) => ({
    name: stepName(i, a),
    run: (p) => performAction(p, a),
    ...(base.useClock ? { advanceClock: advanceClockMs ?? 0 } : {}),
  }))
  return {
    clone: base.clone,
    useClock: base.useClock,
    volatileValueTestIds: base.volatileValueTestIds,
    ready: base.ready,
    steps,
  }
}
