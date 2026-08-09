#!/usr/bin/env node
// Token-efficiency measurement: PythScribe vs equivalent React+TypeScript.
//
// Two modes:
//
//   1. **Default (no install required)**: a heuristic that approximates GPT-4
//      tokenization as `chars / 3.7` for code, which is well within ±5% of
//      cl100k_base for typical source files. Use this for quick numbers.
//
//   2. **--real**: uses `js-tiktoken` (npm) if installed, for cl100k_base
//      counts that match exactly what an LLM would see. Install with:
//
//        npm install js-tiktoken
//
// Usage:
//   node bench/measure_tokens.mjs               # heuristic
//   node bench/measure_tokens.mjs --real        # cl100k_base via js-tiktoken

import { readFile, stat } from "node:fs/promises";
import { dirname, resolve, basename } from "node:path";
import { fileURLToPath } from "node:url";

const here = dirname(fileURLToPath(import.meta.url));
const root = resolve(here, "..");

const args = process.argv.slice(2);
const useReal = args.includes("--real");

const PAIRS = [
  {
    label: "dashboard_500",
    pythscribe: resolve(root, "large-samples/pythscribe/dashboard_500.ps"),
    tsx: resolve(root, "large-samples/react-equivalent/Dashboard500.tsx"),
  },
  {
    label: "app_1000",
    pythscribe: resolve(root, "large-samples/pythscribe/app_1000.ps"),
    tsx: resolve(root, "large-samples/react-equivalent/App1000.tsx"),
  },
];

async function exists(path) {
  try {
    await stat(path);
    return true;
  } catch {
    return false;
  }
}

async function loadEncoder() {
  if (!useReal) return null;
  try {
    const { encodingForModel } = await import("js-tiktoken");
    return encodingForModel("gpt-4");
  } catch (e) {
    console.warn(
      "[warn] --real requested but js-tiktoken not installed; falling back to heuristic.",
    );
    console.warn("       npm install js-tiktoken");
    return null;
  }
}

function heuristicTokens(text) {
  // Empirically chars/3.7 tracks cl100k_base within ~5% on TS/Python source.
  // (Whitespace-heavy → cheaper; symbol-heavy → costlier; the average lands here.)
  return Math.round(text.length / 3.7);
}

function loc(text) {
  // Significant lines: not blank, not pure comment.
  let total = 0;
  let sig = 0;
  for (const raw of text.split(/\r?\n/)) {
    total += 1;
    const line = raw.trim();
    if (line === "") continue;
    if (line.startsWith("//") || line.startsWith("#")) continue;
    sig += 1;
  }
  return { total, sig };
}

async function measure(path, encoder) {
  const text = await readFile(path, "utf8");
  const tokens = encoder
    ? encoder.encode(text).length
    : heuristicTokens(text);
  const { total, sig } = loc(text);
  return { path, bytes: text.length, tokens, total, sig };
}

const enc = await loadEncoder();
const mode = enc ? "cl100k_base (real)" : "heuristic (chars/3.7)";

console.log(`# Token-efficiency comparison (mode: ${mode})\n`);

let psTotalTokens = 0;
let tsTotalTokens = 0;

for (const p of PAIRS) {
  const psExists = await exists(p.pythscribe);
  const tsExists = await exists(p.tsx);

  if (!psExists || !tsExists) {
    console.log(`## ${p.label}: SKIPPED`);
    if (!psExists) console.log(`  missing: ${p.pythscribe}`);
    if (!tsExists) console.log(`  missing: ${p.tsx}`);
    console.log("");
    continue;
  }

  const ps = await measure(p.pythscribe, enc);
  const ts = await measure(p.tsx, enc);
  const tokDelta = (1 - ps.tokens / ts.tokens) * 100;
  const locDelta = (1 - ps.sig / ts.sig) * 100;

  console.log(`## ${p.label}`);
  console.log("");
  console.log(`| Metric             | PythScribe | React+TS | Δ (PS vs TS) |`);
  console.log(`|--------------------|-----------:|---------:|-------------:|`);
  console.log(
    `| Bytes              | ${ps.bytes.toLocaleString()} | ${ts.bytes.toLocaleString()} | ${(
      (1 - ps.bytes / ts.bytes) *
      100
    ).toFixed(1)}% |`,
  );
  console.log(
    `| Tokens             | ${ps.tokens.toLocaleString()} | ${ts.tokens.toLocaleString()} | ${tokDelta.toFixed(1)}% |`,
  );
  console.log(
    `| Lines (total)      | ${ps.total} | ${ts.total} | ${(
      (1 - ps.total / ts.total) *
      100
    ).toFixed(1)}% |`,
  );
  console.log(
    `| Lines (significant)| ${ps.sig} | ${ts.sig} | ${locDelta.toFixed(1)}% |`,
  );
  console.log("");

  psTotalTokens += ps.tokens;
  tsTotalTokens += ts.tokens;
}

if (psTotalTokens > 0 && tsTotalTokens > 0) {
  const overall = (1 - psTotalTokens / tsTotalTokens) * 100;
  console.log(
    `## Overall: PythScribe uses ${overall.toFixed(1)}% fewer tokens across ${PAIRS.length} sample(s).`,
  );
}
