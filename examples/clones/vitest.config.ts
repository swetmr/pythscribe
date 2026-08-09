/// <reference types="vitest" />
import { defineConfig } from 'vite'
import react from '@vitejs/plugin-react'
import pyths from 'vite-plugin-pyths'

// Root-level harness for shared/**/*.test.tsx tri-track render-parity —
// deliberately NOT nested inside vite/ or next/ (shared/ is framework-
// agnostic; the parity tests should run without either app's dev server).
// PYTHS_BIN env pattern matches vite/vite.config.ts and next/next.config.mjs
// (reference-app-style — see CONTRIBUTING.md "Binary resolution").
const pythsBin =
  process.env.PYTHS_BIN ??
  'pyths'

export default defineConfig({
  plugins: [react(), pyths({ pythsBin, reactRefresh: false })],
  resolve: {
    // Force a single React instance across the workspace's hoisted
    // node_modules (see vite/vite.config.ts for the full B-017 rationale).
    dedupe: ['react', 'react-dom'],
  },
  test: {
    globals: true,
    environment: 'jsdom',
    setupFiles: ['./test/setup.ts'],
    css: false,
    include: ['shared/**/*.{test,spec}.{ts,tsx}'],
  },
})
