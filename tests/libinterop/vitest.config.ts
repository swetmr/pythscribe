/// <reference types="vitest" />
import { defineConfig } from 'vite'
import react from '@vitejs/plugin-react'
import pyths from 'vite-plugin-pyths'
import { resolve } from 'node:path'

// Track-B library-interop behavioral suite (tests/libinterop).
//
// Unlike tests/jsinterop/behavioral (which junctions examples/clones deps),
// this suite has its OWN package.json + node_modules: it needs real
// third-party packages (@radix-ui/*, cva, lucide-react, react-hook-form,
// @tanstack/react-query, framer-motion, ...) that the clones workspace
// doesn't carry. Run `npm install` here once (run.mjs does it for you).

const HERE = resolve(__dirname)
const REPO_ROOT = resolve(HERE, '../..') // pythscribe

const pythsBin =
  process.env.PYTHS_BIN ??
  resolve(REPO_ROOT, `target/release/pyths${process.platform === 'win32' ? '.exe' : ''}`)

export default defineConfig({
  root: HERE,
  plugins: [react(), pyths({ pythsBin, reactRefresh: false })],
  resolve: {
    dedupe: ['react', 'react-dom'],
  },
  server: {
    fs: { allow: [REPO_ROOT] },
  },
  test: {
    globals: true,
    environment: 'jsdom',
    setupFiles: [resolve(HERE, 'test/setup.ts')],
    css: false,
    include: ['specs/**/*.behavior.test.ts'],
    fileParallelism: false,
    testTimeout: 30_000,
    hookTimeout: 30_000,
  },
})
