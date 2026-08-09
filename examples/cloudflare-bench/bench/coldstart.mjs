#!/usr/bin/env node
// Cold-start + warm-latency harness (dual-track dashboard metric 6/7) —
// autocannon-backed, replacing the hand-rolled fetch/performance.now
// loop in run.mjs for the warm-load numbers.
//
//   node bench/coldstart.mjs --url=<worker-url> [--duration=10] [--connections=10] [--cold-samples=5] [--cold-gap=90]
//
// Two phases:
//   1. COLD-ish sampling: `cold-samples` single requests spaced
//      `cold-gap` seconds apart (long enough for CF to evict the
//      isolate on a quiet deployment — production isolate lifetime is
//      not client-controllable, so these are labeled first-request
//      latencies, not guaranteed cold starts; the <50ms claim is
//      assessed against these).
//   2. WARM load: autocannon for `duration`s at `connections`
//      concurrent connections — p50/p97_5/p99 from autocannon's
//      histogram (not hand-rolled timing).
//
// Results print as a markdown row to append to the dashboard.

import autocannon from "autocannon";

const args = Object.fromEntries(
  process.argv.slice(2).map((a) => {
    const [k, v] = a.replace(/^--/, "").split("=");
    return [k, v ?? "true"];
  }),
);
const url = args.url;
if (!url) {
  console.error("usage: node bench/coldstart.mjs --url=<worker-url> [--duration=10] [--connections=10] [--cold-samples=5] [--cold-gap=90]");
  process.exit(2);
}
const coldSamples = Number(args["cold-samples"] ?? 5);
const coldGap = Number(args["cold-gap"] ?? 90);
const duration = Number(args.duration ?? 10);
const connections = Number(args.connections ?? 10);

const sleep = (s) => new Promise((r) => setTimeout(r, s * 1000));

console.log(`[coldstart] phase 1: ${coldSamples} spaced first-requests (${coldGap}s apart)`);
const colds = [];
for (let i = 0; i < coldSamples; i++) {
  if (i > 0) await sleep(coldGap);
  const t0 = performance.now();
  const res = await fetch(url);
  await res.text();
  const ms = performance.now() - t0;
  colds.push(ms);
  console.log(`  sample ${i + 1}: ${ms.toFixed(1)} ms (${res.status})`);
}
colds.sort((a, b) => a - b);

console.log(`[coldstart] phase 2: autocannon ${duration}s @ ${connections} connections`);
const result = await autocannon({ url, duration, connections });

const median = colds[Math.floor(colds.length / 2)];
console.log("\n| URL | first-req median (ms) | first-req max | warm p50 | warm p97.5 | warm p99 | req/s |");
console.log("|---|--:|--:|--:|--:|--:|--:|");
console.log(
  `| ${url} | ${median.toFixed(1)} | ${colds[colds.length - 1].toFixed(1)} | ` +
  `${result.latency.p50} | ${result.latency.p97_5} | ${result.latency.p99} | ${result.requests.average} |`,
);
if (median >= 50) {
  console.log(`\nNOTE: first-request median ${median.toFixed(1)}ms >= 50ms — the <50ms cold-start claim is NOT supported by this run.`);
}
