// Shared behavioral-oracle harness.
//
// defineBehaviorSuite(taskId, componentName, drive) enumerates every
// materialized (cond, sample) unit for `taskId` from the manifest, and for each
// one runs an isolated `it`: dynamic-import the generated module, pick the
// pinned component export, render it, then run `drive` (the task's 2-4 core
// behavioral assertions). Each outcome is written to a per-unit result JSON
// (compile-fail | mount-fail | assert-fail | ok) so the runner can aggregate
// behavioral pass@1 independently of vitest's own exit code.

import { describe, it, expect } from 'vitest'
import { render, cleanup, screen, within } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { createElement } from 'react'
import { readFileSync, writeFileSync } from 'node:fs'
import { join } from 'node:path'

export { expect, screen, within }
export const user = () => userEvent.setup()

/** Innermost element whose textContent matches `re` (smallest match wins). */
export function innermostByText(root: HTMLElement, re: RegExp): HTMLElement | null {
  const all = Array.from(root.querySelectorAll<HTMLElement>('*'))
    .filter((el) => re.test(el.textContent || ''))
  if (!all.length) return null
  all.sort((a, b) => (a.textContent || '').length - (b.textContent || '').length)
  return all[0]
}

/** All text-entry controls, tolerant of the input flavor: role `textbox`
 *  (input[type=text]/textarea) OR `searchbox` (input[type=search]) — models
 *  legitimately pick either for search/entry UIs. Query order is stable
 *  (textboxes first) so `[0]` keeps its old meaning where both exist. */
export function textEntries(): HTMLElement[] {
  return [...screen.queryAllByRole('textbox'), ...screen.queryAllByRole('searchbox')]
}

/** Buttons or tabs matching `name` — filter tabs are legitimately either. */
export function buttonsOrTabs(name: RegExp): HTMLElement[] {
  return [...screen.queryAllByRole('button', { name }), ...screen.queryAllByRole('tab', { name })]
}

/** The primary action button for `name`, tolerant of sibling look-alikes:
 *  prefer an exact accessible-name match, then the broad pattern minus
 *  `exclude` (e.g. per-item Repost/Reply buttons in a feed). */
export function primaryButton(name: RegExp, exact: RegExp, exclude?: RegExp): HTMLElement {
  const all = screen.queryAllByRole('button', { name })
  const label = (b: HTMLElement) =>
    (b.getAttribute('aria-label') || b.textContent || '').trim()
  const exactHit = all.find((b) => exact.test(label(b)))
  if (exactHit) return exactHit
  const kept = exclude ? all.filter((b) => !exclude.test(label(b))) : all
  if (!kept.length) throw new Error(`no button matching ${name} (after excluding ${exclude})`)
  return kept[0]
}

/** Count elements carrying an inline width style expressed as a percent. */
export function elementsWithPercentWidth(root: HTMLElement): HTMLElement[] {
  return Array.from(root.querySelectorAll<HTMLElement>('[style]'))
    .filter((el) => /width:\s*\d/.test(el.getAttribute('style') || '') &&
      /%/.test((el.getAttribute('style') || '').split('width')[1] || ''))
}

// `import.meta.url` is not a file:// URL under the vitest transform, so anchor
// on an env var (set by run.mjs) with an absolute default matching this repo.
const BEHAVIORAL = process.env.BEHAVIORAL_DIR ||
  './examples/cloudflare-bench/gen_eval/behavioral'
const EXP = process.env.BEHAVIORAL_EXP || 'baseline-001'
const WORK = join(BEHAVIORAL, '.work', EXP)

type Unit = {
  task: string; component: string; cond: 'ps' | 'psc'; sample: number
  base: string; file: string | null; no_code?: boolean
}
type Manifest = { exp: string; units: Unit[] }

function loadManifest(): Manifest {
  return JSON.parse(readFileSync(join(WORK, 'manifest.json'), 'utf8'))
}

function recordResult(u: Unit, pass: boolean, reason: string) {
  const rec = {
    exp: EXP, task: u.task, cond: u.cond, sample: u.sample,
    behavioral_pass: pass, reason,
  }
  writeFileSync(join(WORK, 'results', `${u.base}.json`), JSON.stringify(rec), 'utf8')
}

/** Pick the pinned component export; fall back to App, then any function export. */
function pickComponent(mod: Record<string, unknown>, name: string): any {
  if (typeof mod[name] === 'function') return mod[name]
  if (typeof mod.App === 'function') return mod.App
  const fn = Object.values(mod).find((v) => typeof v === 'function')
  if (fn) return fn
  throw new Error(`no component export (looked for ${name} / App)`)
}

export type DriveCtx = {
  render: (props?: Record<string, unknown>) => ReturnType<typeof render>
  Component: any
  screen: ReturnType<typeof render>
  u: Unit
}

/**
 * @param drive async fn receiving a render helper + the picked Component.
 *   Should perform the task's load-bearing behavioral assertions. Any throw
 *   (assertion or interaction error) marks the sample a behavioral FAIL.
 */
export function defineBehaviorSuite(
  taskId: string,
  componentName: string,
  drive: (ctx: { mount: (props?: Record<string, unknown>) => any; Component: any }) => Promise<void> | void,
) {
  const units = loadManifest().units.filter((u) => u.task === taskId)

  describe(`${taskId} [${EXP}] behavioral oracle`, () => {
    for (const u of units) {
      it(`${u.cond} #${u.sample}`, async () => {
        // 1) no code at all (model error / empty completion)
        if (!u.file || u.no_code) {
          recordResult(u, false, 'no_code')
          throw new Error('no_code: completion produced no fenced block')
        }

        // 2) compile + import (vite-plugin-pyths transforms on import)
        let mod: Record<string, unknown>
        try {
          const spec = '/@fs/' + u.file.replace(/\\/g, '/')
          mod = (await import(/* @vite-ignore */ spec)) as Record<string, unknown>
        } catch (e: any) {
          recordResult(u, false, 'compile-fail: ' + String(e?.message || e).slice(0, 240))
          throw e
        }

        // 3) pick + mount
        let Component: any
        let mountResult: ReturnType<typeof render> | null = null
        const mount = (props: Record<string, unknown> = {}) => {
          mountResult = render(createElement(Component, props))
          return mountResult
        }
        try {
          Component = pickComponent(mod, componentName)
        } catch (e: any) {
          recordResult(u, false, 'mount-fail: ' + String(e?.message || e).slice(0, 240))
          throw e
        }

        // 4) drive behavioral assertions
        try {
          await drive({ mount, Component })
          recordResult(u, true, 'ok')
        } catch (e: any) {
          // Distinguish a render/mount crash from a plain assertion failure.
          const msg = String(e?.message || e)
          const kind = mountResult ? 'assert-fail' : 'mount-fail'
          recordResult(u, false, `${kind}: ` + msg.slice(0, 240))
          throw e
        } finally {
          cleanup()
        }
      })
    }
  })
}
