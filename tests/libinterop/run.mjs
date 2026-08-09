// Track-B library-interop behavioral suite — runner.
//
// Usage (from repo root):  node tests/libinterop/run.mjs
//
// Dual-track methodology at library scale: each third-party React library
// has a TSX reference component (the oracle) and a .ps twin (the system
// under test); one vitest spec drives BOTH through identical behavioral
// assertions (mount, interact via user-event, assert). Divergence between
// the two tracks = compiler finding.
//
// Covered (10 packages / 8 spec pairs): @radix-ui/react-dialog,
// @radix-ui/react-dropdown-menu, @radix-ui/react-checkbox,
// class-variance-authority + clsx + tailwind-merge (one shadcn-Button unit),
// lucide-react, react-hook-form, @tanstack/react-query, framer-motion.
//
// Prerequisites:
//   - cargo build --release -p pyths_cli   (or set PYTHS_BIN)
//   - npm install in tests/libinterop      (this runner does it on first use)

import { spawnSync } from 'node:child_process'
import { existsSync } from 'node:fs'
import { dirname, join, resolve } from 'node:path'
import { fileURLToPath } from 'node:url'

const HERE = dirname(fileURLToPath(import.meta.url))

if (!existsSync(join(HERE, 'node_modules'))) {
  console.log('[libinterop] node_modules missing — running npm install ...')
  const npm = process.platform === 'win32' ? 'npm.cmd' : 'npm'
  const i = spawnSync(npm, ['install', '--no-audit', '--no-fund'], {
    cwd: HERE,
    encoding: 'utf8',
    stdio: 'inherit',
    shell: process.platform === 'win32',
    timeout: 600_000,
  })
  if (i.status !== 0) {
    console.error('[libinterop] npm install failed')
    process.exit(i.status ?? 1)
  }
}

// Run vitest's JS entry via node directly — the .bin shim is a bash script
// that spawnSync cannot exec on Windows.
const VITEST_JS = resolve(HERE, 'node_modules/vitest/vitest.mjs')
const r = spawnSync(process.execPath, [VITEST_JS, 'run', '--config', 'vitest.config.ts'], {
  cwd: HERE,
  encoding: 'utf8',
  timeout: 600_000,
  stdio: 'inherit',
  env: { ...process.env },
})
process.exit(r.status ?? 1)
