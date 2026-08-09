import { defineConfig } from 'vite'
import react from '@vitejs/plugin-react'
import pyths from 'vite-plugin-pyths'

// `pyths` binary path: env var override > PATH lookup. Local dev (Windows,
// source clone): point at the local cargo-build output — matches
// reference-app/frontend/vite.config.ts (READ-ONLY reference for this pattern).
const pythsBin =
  process.env.PYTHS_BIN ??
  'pyths'

export default defineConfig({
  plugins: [react(), pyths({ pythsBin, reactRefresh: 'auto' })],
  resolve: {
    // Force a single instance of React + react-router. Without this, a
    // `.ps`-compiled shared component can resolve its own react-router
    // copy under Vite's dev-server module graph (npm workspaces hoist
    // most deps to the root node_modules, but a stray nested copy is
    // enough to break context — same B-017 rationale as reference-app).
    dedupe: ['react', 'react-dom', 'react-router'],
  },
  server: {
    port: 5173,
  },
  preview: {
    port: 4173,
  },
})
