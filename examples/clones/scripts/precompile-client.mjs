// Precompiles every "use client" shared/<clone>/*.ps component to a sibling
// `.client.js` (gitignored). Run before `next dev`/`next build` (wired as
// predev/prebuild in next/package.json).
//
// Why: Turbopack's client-reference proxy generation can't resolve a
// custom-extension (.ps/.psc) module id in the CLIENT graph, so "use
// client" components are compiled ahead of time to plain .js instead of
// being transformed by the Turbopack loader. Server components, layouts,
// and pages compile through the loader normally — this script only
// touches client islands. next-plugin-pyths's `rewritePsImports` prefers
// `.client.js` over `.psc`/`.ps` for any extensionless relative import, so
// a server `.ps`/`.tsx` page doing `from ...shared.hello.HelloCard import
// HelloCard` resolves to this precompiled file automatically — no per-app
// import-path change needed.
//
// Scales across clones automatically: any shared/<clone>/*.ps file whose
// first non-docstring statement is the literal `"use client"` gets
// precompiled. New clones need zero changes to this script.
import { execFileSync } from 'node:child_process'
import { readFileSync, writeFileSync, globSync } from 'node:fs'
import { rewritePsImports } from 'next-plugin-pyths/loader.js'
import path from 'node:path'
import { fileURLToPath } from 'node:url'

const PYTHS = process.env.PYTHS_BIN ?? 'pyths'
const __dirname = typeof import.meta.dirname !== 'undefined'
  ? import.meta.dirname
  : path.dirname(fileURLToPath(import.meta.url))
const root = path.resolve(__dirname, '..') // examples/clones/

function isUseClient(file) {
  const src = readFileSync(file, 'utf8')
  // Strip a single leading triple-quoted module docstring (if present),
  // then find the first line that is neither blank nor a `#` comment —
  // that's the first REAL statement. It must be exactly "use client"
  // (matches the A3/Wave-C precedent in reference-app/frontend-next's
  // Counter.ps). `#` comments are the norm here, not docstrings — see
  // HelloCard.ps's note on the Turbopack UTF-8 char-boundary panic.
  const withoutDocstring = src.replace(/^\s*("""[\s\S]*?"""|'''[\s\S]*?''')\s*/m, '')
  const firstStatementLine = withoutDocstring
    .split('\n')
    .find((l) => l.trim().length > 0 && !l.trim().startsWith('#')) ?? ''
  return firstStatementLine.trim() === '"use client"'
}

const psFiles = globSync('shared/**/*.ps', { cwd: root }).map((f) => path.join(root, f))
const clientFiles = psFiles.filter(isUseClient)

if (clientFiles.length === 0) {
  console.log('precompile-client: no "use client" shared/*.ps files found')
  process.exit(0)
}

for (const file of clientFiles) {
  const out = file.replace(/\.ps$/, '.client.js')
  console.log(`precompile-client: ${path.relative(root, file)} -> ${path.relative(root, out)}`)
  execFileSync(PYTHS, ['compile', file, '-o', out], { stdio: 'inherit' })
  // Rewrite extensionless relative imports to explicit .client.js/.psc/.ps
  // siblings (same importer-aware fix as next-plugin #66) — without this a
  // MULTI-FILE island's sub-imports resolve the .tsx oracle via global
  // extension order in Next's client graph (found by the YouTube clone;
  // single-module islands were the workaround, now unnecessary).
  writeFileSync(out, rewritePsImports(readFileSync(out, 'utf8'), file))
}
console.log(`precompile-client: ${clientFiles.length} island(s) compiled`)
