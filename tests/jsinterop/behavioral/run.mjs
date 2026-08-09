// Track-A Promise-interop behavioral suite — runner.
//
// Usage (from repo root):  node tests/jsinterop/behavioral/run.mjs
//
// Renders the fixed .ps components under components/ through
// vite-plugin-pyths (jsdom + @testing-library/react) and asserts the
// pinned async behaviors in specs/. Deps come from examples/clones —
// a node_modules junction is created here on first run.
//
// Prerequisite: cargo build --release -p pyths_cli (or set PYTHS_BIN).

import { spawnSync } from 'node:child_process'
import { existsSync, symlinkSync } from 'node:fs'
import { dirname, join, resolve } from 'node:path'
import { fileURLToPath } from 'node:url'

const HERE = dirname(fileURLToPath(import.meta.url))
const REPO_ROOT = resolve(HERE, '../../..')
// Run vitest's JS entry via node directly — the .bin shim is a bash script
// that spawnSync cannot exec on Windows.
const VITEST_JS = resolve(REPO_ROOT, 'examples/clones/node_modules/vitest/vitest.mjs')

/** Vitest resolves config plugins relative to this dir; expose the clones
 *  deps here via a directory junction (Windows-friendly). */
function ensureNodeModules() {
  const link = join(HERE, 'node_modules')
  if (existsSync(link)) return
  const target = resolve(REPO_ROOT, 'examples/clones/node_modules')
  symlinkSync(target, link, 'junction')
  console.log('[jsinterop-behavioral] created node_modules junction -> examples/clones/node_modules')
}

ensureNodeModules()
const r = spawnSync(process.execPath, [VITEST_JS, 'run', '--config', 'vitest.config.ts'], {
  cwd: HERE,
  encoding: 'utf8',
  timeout: 600_000,
  stdio: 'inherit',
  env: { ...process.env },
})
process.exit(r.status ?? 1)
