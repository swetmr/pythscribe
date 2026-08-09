// Pyodide-based equivalent of the PythScribe worker.
//
// Note: Pyodide is large (~6 MB compressed) and ships an entire CPython
// runtime. We load it lazily on the first request. The first request after
// each cold start pays the full Pyodide initialization cost; subsequent
// requests reuse the cached interpreter.
//
// This file is the worker entry. `loadPyodide` is fetched from the public
// CDN at runtime — Cloudflare Workers does not bundle it.

let pyodideReady = null;

const SOURCE = `
import math

def fibonacci(n):
    if n < 2:
        return n
    a, b = 0, 1
    for _ in range(2, n + 1):
        a, b = b, a + b
    return b

def sum_squares(n):
    total = 0
    for i in range(1, n + 1):
        total += i * i
    return total

def sin_sum(n):
    total = 0.0
    for i in range(1, n + 1):
        total += math.sin(i)
    return total
`;

async function getPyodide() {
  if (pyodideReady) return pyodideReady;
  pyodideReady = (async () => {
    // Dynamic import (Workers must enable workers_dev / nodejs_compat depending on bundler).
    const { loadPyodide } = await import("https://cdn.jsdelivr.net/pyodide/v0.26.2/full/pyodide.mjs");
    const pyodide = await loadPyodide({
      indexURL: "https://cdn.jsdelivr.net/pyodide/v0.26.2/full/",
    });
    pyodide.runPython(SOURCE);
    return pyodide;
  })();
  return pyodideReady;
}

export default {
  async fetch(request) {
    try {
      const url = new URL(request.url);
      const path = url.pathname.replace(/^\//, "");
      const params = url.searchParams;
      const py = await getPyodide();

      let result;
      switch (path) {
        case "fibonacci":
          result = py.runPython(`fibonacci(${Number(params.get("n") || 30)})`);
          break;
        case "sum_squares":
          result = py.runPython(`sum_squares(${Number(params.get("n") || 10000)})`);
          break;
        case "sin_sum":
          result = py.runPython(`sin_sum(${Number(params.get("n") || 10000)})`);
          break;
        default:
          return new Response(
            JSON.stringify({ available: ["fibonacci", "sum_squares", "sin_sum"] }),
            { status: 404, headers: { "content-type": "application/json" } },
          );
      }
      return new Response(JSON.stringify({ result }), {
        headers: { "content-type": "application/json" },
      });
    } catch (e) {
      return new Response(JSON.stringify({ error: e.message, name: e.name }), {
        status: 500,
        headers: { "content-type": "application/json" },
      });
    }
  },
};
