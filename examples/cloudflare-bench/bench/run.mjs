#!/usr/bin/env node
// Benchmark two CF Worker URLs against each other.
//
// Usage:
//   node bench/run.mjs --pythscribe=<url> --pyodide=<url> [--iterations=200]
//
// Each Worker is hit `iterations` times for each endpoint; latency is
// measured per request. We report:
//   - cold-start (first request after a fresh isolate; we can't directly
//     control this on production CF, so we report "first-N" latencies)
//   - p50/p95/p99 of warm requests
//   - mean and stddev

const args = Object.fromEntries(
  process.argv.slice(2).map((a) => {
    const [k, v] = a.replace(/^--/, "").split("=");
    return [k, v ?? "true"];
  }),
);

const psUrl = args.pythscribe || "http://localhost:8787";
const pyUrl = args.pyodide || "http://localhost:8788";
const iterations = Number(args.iterations || 200);
const coldSamples = Number(args["cold-samples"] || 5);

const ENDPOINTS = [
  { name: "fibonacci", query: "n=30" },
  { name: "sum_squares", query: "n=10000" },
  { name: "sin_sum", query: "n=10000" },
];

async function timed(url) {
  const t0 = performance.now();
  const res = await fetch(url);
  const text = await res.text();
  const t1 = performance.now();
  return { ms: t1 - t0, ok: res.ok, body: text };
}

function quantile(sorted, q) {
  if (sorted.length === 0) return NaN;
  const i = Math.min(sorted.length - 1, Math.floor(q * sorted.length));
  return sorted[i];
}

function summarize(samples) {
  const sorted = [...samples].sort((a, b) => a - b);
  const sum = sorted.reduce((a, b) => a + b, 0);
  const mean = sum / sorted.length;
  const variance = sorted.reduce((a, b) => a + (b - mean) ** 2, 0) / sorted.length;
  return {
    n: sorted.length,
    mean: mean.toFixed(2),
    stdev: Math.sqrt(variance).toFixed(2),
    p50: quantile(sorted, 0.5).toFixed(2),
    p95: quantile(sorted, 0.95).toFixed(2),
    p99: quantile(sorted, 0.99).toFixed(2),
  };
}

async function benchOne(label, base) {
  console.log(`\n=== ${label} (${base}) ===`);
  const result = { label, base, endpoints: {} };
  for (const ep of ENDPOINTS) {
    const url = `${base}/${ep.name}?${ep.query}`;
    const cold = [];
    const warm = [];

    // Cold samples — issue requests to URLs that defeat any cache.
    // (CF doesn't actually let us reset the isolate from outside; we report
    // the first-N requests as a proxy for the cold-start band.)
    for (let i = 0; i < coldSamples; i++) {
      const u = `${url}&_cold=${i}_${Date.now()}`;
      const r = await timed(u);
      if (r.ok) cold.push(r.ms);
    }

    for (let i = 0; i < iterations; i++) {
      const r = await timed(url);
      if (r.ok) warm.push(r.ms);
    }

    result.endpoints[ep.name] = {
      cold: summarize(cold),
      warm: summarize(warm),
    };
    console.log(
      `  ${ep.name}: cold p50=${result.endpoints[ep.name].cold.p50}ms warm p50=${result.endpoints[ep.name].warm.p50}ms p99=${result.endpoints[ep.name].warm.p99}ms`,
    );
  }
  return result;
}

async function main() {
  console.log(`PythScribe: ${psUrl}`);
  console.log(`Pyodide:    ${pyUrl}`);
  console.log(`Iterations: ${iterations} (warm) + ${coldSamples} (cold)`);

  const ps = await benchOne("PythScribe", psUrl);
  const py = await benchOne("Pyodide", pyUrl);

  // Write a JSON snapshot next to RESULTS.md
  const out = { timestamp: new Date().toISOString(), pythscribe: ps, pyodide: py };
  console.log("\n=== JSON ===");
  console.log(JSON.stringify(out, null, 2));
}

main().catch((e) => {
  console.error("Bench failed:", e);
  process.exit(1);
});
