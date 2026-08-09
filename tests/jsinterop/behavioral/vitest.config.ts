/// <reference types="vitest" />
import { defineConfig } from 'vite'
import react from '@vitejs/plugin-react'
import pyths from 'vite-plugin-pyths'
import { resolve } from 'node:path'

// Track-A Promise-interop behavioral suite (tests/jsinterop/behavioral).
//
// Same harness shape as examples/cloudflare-bench/gen_eval/behavioral:
// reuses the examples/clones workspace's node_modules (react,
// testing-library, vitest, vite-plugin-pyths) by rooting Vite there, so
// this dir needs no install of its own (run.mjs creates the junction).

const HERE = resolve(__dirname)
const REPO_ROOT = resolve(HERE, '../../..')           // pythscribe
const CLONES = resolve(REPO_ROOT, 'examples/clones')  // deps live here

const pythsBin =
  process.env.PYTHS_BIN ??
  resolve(REPO_ROOT, 'target/release/pyths.exe')

export default defineConfig({
  root: CLONES,
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
    setupFiles: [resolve(CLONES, 'test/setup.ts')],
    css: false,
    include: [resolve(HERE, 'specs/**/*.behavior.test.ts').replace(/\\/g, '/')],
    fileParallelism: false,
    testTimeout: 30_000,
    hookTimeout: 30_000,
  },
})
