import type { Page, TestInfo } from '@playwright/test'
import { createHash } from 'node:crypto'
import { mkdirSync, readFileSync, writeFileSync, existsSync } from 'node:fs'
import { fileURLToPath } from 'node:url'
import path from 'node:path'

// Runtime-error differential capture (interaction-differential v1).
//
// Instruments a Playwright page with three error channels —
//   1. page.on('console')      → console.error / console.warning messages
//   2. page.on('pageerror')    → uncaught exceptions AND (in Chromium)
//                                unhandled promise rejections
//   3. an addInitScript 'unhandledrejection' listener → window.__runtimeErrors
//      (belt-and-braces: deterministic ordering + rejections handled "too late")
// plus page.on('crash').
//
// Policy (applied by parity-test.ts in fixture teardown):
//   ZERO-ERROR with a per-clone allowlist on BOTH tracks, plus
//   ps-track-minus-react-oracle SUBTRACTION: an error on the compiled
//   .ps/.psc route is excused when the React oracle recorded the identical
//   normalized error for the same spec+test (run-scoped ledger under
//   test-results/runtime-ledger). The existing specs list the oracle route
//   FIRST so, with serial workers (CI), the oracle ledger exists by the
//   time the ps variant runs.

export type ErrorSource = 'console' | 'pageerror' | 'unhandledrejection' | 'crash'

export interface CapturedError {
  source: ErrorSource
  raw: string
  normalized: string
}

const HERE = path.dirname(fileURLToPath(import.meta.url))
const LEDGER_DIR = path.resolve(HERE, '..', 'test-results', 'runtime-ledger')

// ---------------------------------------------------------------------------
// Normalization — compare ps-vs-react on MESSAGE identity, not build artifacts.
// Strips origins/ports, content-hash chunks, :line:col positions; first line
// only (compiled stacks legitimately differ between tracks — frame identity
// would defeat cross-track subtraction).
// ---------------------------------------------------------------------------
export function normalizeError(raw: string): string {
  let s = (raw || '').trim().split('\n')[0]
  // strip scheme://host[:port] but keep the path
  s = s.replace(/https?:\/\/[a-zA-Z0-9.-]+(:\d+)?\//g, '/')
  // strip vite content hashes: assets/index-Dk2jf9aB.js -> assets/index.js
  s = s.replace(/([\w[\]]+)-[A-Za-z0-9_-]{6,}\.(m?js|css)/g, '$1.$2')
  // strip :line:col after a source-file extension
  s = s.replace(/\.(m?js|cjs|ts|tsx|css)(:\d+)+/g, '.$1')
  return s.replace(/\s+/g, ' ').trim()
}

// ---------------------------------------------------------------------------
// Per-clone allowlist — regexes matched against the NORMALIZED error.
// Key = clone slug ('' segment rules in cloneKeyFromPath). '*' applies always.
// ---------------------------------------------------------------------------
export const ALLOWLIST: Record<string, RegExp[]> = {
  // The coursera clone has a DELIBERATE error-boundary crash trigger
  // (quiz-crash-dev → throw QuizCrashError('quiz dev crash'), the same
  // Tier-7 custom exception class on ALL tracks). React 19's default
  // onCaughtError logs the caught error via console.error on BOTH tracks.
  // The entry is NAME-ANCHORED on purpose: it permits the deliberate crash
  // (the oracle track has no ledger to subtract against) while any
  // cross-track naming divergence — e.g. the old `Exception: quiz dev
  // crash` (.ps) vs `Error: quiz dev crash` (.tsx) mismatch — is NOT
  // allowlisted and fails the differential. Do not widen this back to a
  // bare message substring.
  coursera: [
    /^QuizCrashError: quiz dev crash/,
    /error occurred in the <\w+> component/i,
    /React will try to recreate this component tree/i,
  ],
  '*': [],
}

export function cloneKeyFromPath(pathname: string): string {
  let p = pathname.replace(/^\/react-reference/, '')
  p = p.replace(/^\//, '')
  const seg = p.split('/')[0] || 'home'
  return seg.replace(/-psc$/, '')
}

export function allowlistFor(cloneKey: string): RegExp[] {
  return [...(ALLOWLIST[cloneKey] || []), ...(ALLOWLIST['*'] || [])]
}

export function isAllowlisted(normalized: string, cloneKey: string): boolean {
  return allowlistFor(cloneKey).some((re) => re.test(normalized))
}

// ---------------------------------------------------------------------------
// Capture
// ---------------------------------------------------------------------------
export class RuntimeCapture {
  readonly errors: CapturedError[] = []
  readonly warnings: string[] = []
  lastPathname = '/'
  private readonly page: Page

  constructor(page: Page) {
    this.page = page
  }

  push(source: ErrorSource, raw: string) {
    this.errors.push({ source, raw, normalized: normalizeError(raw) })
  }

  /** Pull any init-script-channel rejections out of the page. Safe on closed pages. */
  async drain(): Promise<void> {
    if (this.page.isClosed()) return
    try {
      const items = await this.page.evaluate(() => {
        const w = window as unknown as { __runtimeErrors?: { text: string }[] }
        const out = w.__runtimeErrors || []
        w.__runtimeErrors = []
        return out.map((it) => it.text)
      })
      for (const text of items) this.push('unhandledrejection', text)
    } catch {
      // page navigated/closed mid-drain — the pageerror channel still saw
      // anything Chromium surfaced; nothing more to do.
    }
  }

  /** Deduped normalized error strings. */
  normalizedSet(): string[] {
    return [...new Set(this.errors.map((e) => e.normalized))]
  }
}

export async function attachRuntimeCapture(page: Page): Promise<RuntimeCapture> {
  const cap = new RuntimeCapture(page)

  await page.addInitScript(() => {
    const w = window as unknown as { __runtimeErrors: { source: string; text: string }[] }
    w.__runtimeErrors = []
    window.addEventListener('unhandledrejection', (e) => {
      const r = (e as PromiseRejectionEvent).reason as { stack?: string } | undefined
      const text = r && r.stack ? String(r.stack) : String(r)
      w.__runtimeErrors.push({ source: 'unhandledrejection', text })
    })
  })

  page.on('console', (msg) => {
    const type = msg.type()
    if (type === 'error') cap.push('console', msg.text())
    else if (type === 'warning') cap.warnings.push(msg.text())
  })
  page.on('pageerror', (err) => {
    cap.push('pageerror', err.stack || err.message || String(err))
  })
  page.on('crash', () => {
    cap.push('crash', 'page crashed (browser-level)')
  })
  page.on('framenavigated', (frame) => {
    if (frame === page.mainFrame()) {
      try {
        cap.lastPathname = new URL(frame.url()).pathname
      } catch {
        /* about:blank etc. */
      }
    }
  })

  return cap
}

// ---------------------------------------------------------------------------
// Freeze probe — "is the main thread still alive after this test's
// interactions?" evaluate(() => 0) + a requestAnimationFrame round-trip
// (rAF skipped when Playwright's fake clock is installed — rAF is faked).
// ---------------------------------------------------------------------------
export async function freezeProbe(
  page: Page,
  opts: { timeoutMs?: number; useRaf?: boolean } = {},
): Promise<'alive' | 'frozen' | 'skipped'> {
  const timeoutMs = opts.timeoutMs ?? 2000
  const useRaf = opts.useRaf ?? true
  if (page.isClosed()) return 'skipped'
  const probe = page
    .evaluate((withRaf) => {
      if (!withRaf) return 0
      return new Promise<number>((resolve) => requestAnimationFrame(() => resolve(0)))
    }, useRaf)
    .then(
      () => 'alive' as const,
      () => 'skipped' as const, // context destroyed mid-probe — not a freeze
    )
  const timer = new Promise<'frozen'>((resolve) => setTimeout(() => resolve('frozen'), timeoutMs))
  return Promise.race([probe, timer])
}

// ---------------------------------------------------------------------------
// Run-scoped oracle ledger (test-results/ is wiped per Playwright run)
// ---------------------------------------------------------------------------
function ledgerFile(specFile: string, title: string, routeKind: 'oracle' | 'ps'): string {
  const key = createHash('sha1').update(path.basename(specFile) + '::' + title).digest('hex').slice(0, 16)
  return path.join(LEDGER_DIR, `${key}.${routeKind}.json`)
}

export function writeLedger(specFile: string, title: string, routeKind: 'oracle' | 'ps', normalized: string[]) {
  try {
    mkdirSync(LEDGER_DIR, { recursive: true })
    writeFileSync(
      ledgerFile(specFile, title, routeKind),
      JSON.stringify({ specFile: path.basename(specFile), title, routeKind, normalized }, null, 2),
    )
  } catch {
    /* ledger is best-effort */
  }
}

export function readOracleLedger(specFile: string, title: string): string[] | null {
  const f = ledgerFile(specFile, title, 'oracle')
  if (!existsSync(f)) return null
  try {
    return JSON.parse(readFileSync(f, 'utf8')).normalized as string[]
  } catch {
    return null
  }
}

// ---------------------------------------------------------------------------
// Teardown policy assertion
// ---------------------------------------------------------------------------
export async function assertRuntimeClean(cap: RuntimeCapture, page: Page, testInfo: TestInfo): Promise<void> {
  await cap.drain()

  const pathname = cap.lastPathname
  const cloneKey = cloneKeyFromPath(pathname)
  const routeKind: 'oracle' | 'ps' = pathname.startsWith('/react-reference') ? 'oracle' : 'ps'

  const all = cap.normalizedSet()
  // Ledger first, so a failing oracle test still records its errors for
  // the ps variant's subtraction.
  writeLedger(testInfo.file, testInfo.title, routeKind, all)

  let residual = all.filter((n) => !isAllowlisted(n, cloneKey))
  let excusedByOracle: string[] = []
  if (routeKind === 'ps' && residual.length > 0) {
    const oracle = readOracleLedger(testInfo.file, testInfo.title)
    if (oracle) {
      excusedByOracle = residual.filter((n) => oracle.includes(n))
      residual = residual.filter((n) => !oracle.includes(n))
    }
  }

  if (cap.errors.length > 0 || cap.warnings.length > 0) {
    await testInfo.attach('runtime-capture', {
      contentType: 'application/json',
      body: JSON.stringify(
        { route: pathname, routeKind, cloneKey, errors: cap.errors, warnings: cap.warnings, excusedByOracle },
        null,
        2,
      ),
    })
  }

  if (residual.length > 0) {
    const details = residual
      .map((n) => {
        const raw = cap.errors.find((e) => e.normalized === n)
        return `  [${raw?.source}] ${n}\n    raw: ${(raw?.raw || '').split('\n').slice(0, 3).join('\n         ')}`
      })
      .join('\n')
    throw new Error(
      `Runtime-error differential (${routeKind} track, ${pathname}): ` +
        `${residual.length} error(s) not allowlisted` +
        (routeKind === 'ps' ? ' and not present on the React oracle' : '') +
        `:\n${details}`,
    )
  }
}
