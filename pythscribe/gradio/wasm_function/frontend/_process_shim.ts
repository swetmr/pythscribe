// react-dom (production build) and @xyflow/react read `process.env.NODE_ENV` at module load; in a
// Vite LIB build there is no automatic `process` shim, so the bare reference throws
// "process is not defined" and the React island never mounts. This module (imported FIRST by
// flow_island.tsx, before react) defines a minimal `globalThis.process` so the checks resolve to a
// production build. It touches nothing else and is inside the lazy island chunk (never in index.js).
const g = globalThis as unknown as { process?: { env?: Record<string, string | undefined> } };
if (!g.process) g.process = { env: {} };
if (!g.process.env) g.process.env = {};
if (!g.process.env.NODE_ENV) g.process.env.NODE_ENV = "production";
export {};
