/// <reference types="vitest" />
import { defineConfig } from 'vite'
import react from '@vitejs/plugin-react'
import pyths from 'vite-plugin-pyths'
import { resolve } from 'node:path'

// Behavioral macro oracle harness.
//
// Renders each *generated* macro completion (materialized under .work/<exp>/)
// through vite-plugin-pyths and asserts the task's pinned BEHAVIOR — not just
// compile success. Reuses the examples/clones workspace's node_modules (react,
// testing-library, vitest, the pyths plugin) by rooting Vite there, so this dir
// needs no install of its own.

const HERE = resolve(__dirname)
const CLONES = resolve(HERE, '../../../clones')       // examples/clones (deps live here)
const REPO_ROOT = resolve(HERE, '../../../../..')     // pythscribe

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
    // Isolate per file so one sample's compile/mount crash cannot poison another
    // task's suite; keep the run single-threaded for deterministic result files.
    fileParallelism: false,
    testTimeout: 30_000,
    hookTimeout: 30_000,
    // A generated component that throws on interaction is a recorded behavioral
    // FAIL (caught in _harness), but React 19 also re-reports the handler error
    // to the process as "uncaught". Ignore those here so one misbehaving sample
    // cannot abort the run — the per-unit result JSON is the source of truth.
    dangerouslyIgnoreUnhandledErrors: true,
  },
})
