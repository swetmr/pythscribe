import { defineConfig } from 'vite'
import react from '@vitejs/plugin-react'
import pyths from 'vite-plugin-pyths'

// `pyths` binary: env override > local source-clone build.
const pythsBin =
  process.env.PYTHS_BIN ?? 'pyths'

export default defineConfig({
  plugins: [react(), pyths({ pythsBin, reactRefresh: 'auto' })],
  resolve: { dedupe: ['react', 'react-dom'] },
  server: { port: 5173 },
  preview: { port: 4173 },
})
