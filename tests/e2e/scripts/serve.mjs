// Minimal static-file server for E2E harness.
// Serves files from tests/e2e/ and the project root (so harness HTMLs
// can fetch compiled fixtures via relative paths). No deps.

import http from "node:http";
import { promises as fs } from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

const __filename = fileURLToPath(import.meta.url);
const __dirname = path.dirname(__filename);
const E2E_ROOT = path.resolve(__dirname, "..");
const REPO_ROOT = path.resolve(E2E_ROOT, "..", "..");
const PORT = Number(process.env.PORT || 8765);

const MIME = {
  ".html": "text/html; charset=utf-8",
  ".js":   "application/javascript; charset=utf-8",
  ".mjs":  "application/javascript; charset=utf-8",
  ".css":  "text/css; charset=utf-8",
  ".json": "application/json; charset=utf-8",
  ".tsx":  "text/plain; charset=utf-8",
  ".ts":   "text/plain; charset=utf-8",
  ".txt":  "text/plain; charset=utf-8",
  ".svg":  "image/svg+xml",
  ".png":  "image/png",
};

// Healthcheck file the Playwright server-readiness probe hits.
await fs.mkdir(path.join(E2E_ROOT, "harness"), { recursive: true });
await fs.writeFile(path.join(E2E_ROOT, "harness", "healthcheck.txt"), "ok\n");

const server = http.createServer(async (req, res) => {
  try {
    const url = new URL(req.url, `http://${req.headers.host}`);
    let pathname = decodeURIComponent(url.pathname);
    if (pathname === "/") pathname = "/harness/index.html";

    // Resolve under E2E_ROOT first; fall back to REPO_ROOT for compiled
    // fixtures and React-equivalent .tsx sources.
    const candidates = [
      path.join(E2E_ROOT, pathname),
      path.join(REPO_ROOT, pathname.replace(/^\//, "")),
    ];
    let resolved = null;
    for (const c of candidates) {
      try {
        const st = await fs.stat(c);
        if (st.isFile()) { resolved = c; break; }
      } catch (_) { /* keep trying */ }
    }

    if (!resolved) {
      res.statusCode = 404;
      res.end(`Not found: ${pathname}\n`);
      return;
    }

    const ext = path.extname(resolved).toLowerCase();
    const ctype = MIME[ext] || "application/octet-stream";
    res.setHeader("Content-Type", ctype);
    res.setHeader("Cache-Control", "no-store");
    res.setHeader("Cross-Origin-Embedder-Policy", "credentialless");
    res.setHeader("Cross-Origin-Opener-Policy", "same-origin");
    const body = await fs.readFile(resolved);
    res.end(body);
  } catch (err) {
    res.statusCode = 500;
    res.end(`Server error: ${err.message}\n`);
  }
});

server.listen(PORT, "127.0.0.1", () => {
  console.log(`[e2e] static server on http://127.0.0.1:${PORT}`);
});
