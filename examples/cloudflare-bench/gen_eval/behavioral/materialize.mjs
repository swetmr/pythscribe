// Materialize macro completions into compilable .ps/.psc scratch files.
//
// For a given experiment dir (e.g. baseline-001), read each macro_*_{ps,psc}_{n}.md
// completion, extract its first fenced code block, and write it to a gitignored
// scratch file under behavioral/.work/<exp>/ named <task>__<cond>__<sample>.<ext>
// so vite-plugin-pyths can compile it on import.
//
// Also emits behavioral/.work/<exp>/manifest.json listing every materialized unit
// so the vitest specs can enumerate samples without re-scanning the raw dir.
//
// Usage: node materialize.mjs <exp_id>   (default: baseline-001)

import { readdirSync, readFileSync, writeFileSync, mkdirSync, rmSync, existsSync } from "node:fs";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const HERE = dirname(fileURLToPath(import.meta.url));
const GEN_EVAL = resolve(HERE, "..");
// Overridable so external experiment folders (e.g. reference-app's
// experiments/geneval-rerun for the arXiv-v2 model roster) can reuse the oracle.
export const RAW_ROOT = process.env.GEN_EVAL_RAW_ROOT || join(GEN_EVAL, "raw");

// Nine macro tasks -> exported React component name pinned by each prompt.
export const MACRO_TASKS = [
  { task: "macro_hello_card", component: "HelloCard" },
  { task: "macro_counter_panel", component: "CounterPanel" },
  { task: "macro_todo_list", component: "TodoApp" },
  { task: "macro_kanban_lite", component: "KanbanLite" },
  { task: "macro_video_grid", component: "VideoGrid" },
  { task: "macro_tweet_composer", component: "TweetFeed" },
  { task: "macro_playlist_player", component: "PlaylistPlayer" },
  { task: "macro_course_cards", component: "CourseCatalog" },
  { task: "macro_movie_rows", component: "MovieBrowser" },
];

const CONDS = ["ps", "psc"];
const SAMPLES = [1, 2, 3, 4, 5];

function extractCodeBlock(text) {
  // First fenced code block, any info string (mirrors run_eval.mjs).
  const m = /```[^\n]*\n([\s\S]*?)```/.exec(text);
  return m ? m[1].replace(/\s+$/, "") + "\n" : null;
}

export function workDir(exp) {
  return join(HERE, ".work", exp);
}

export function materialize(exp) {
  const rawDir = join(RAW_ROOT, exp);
  if (!existsSync(rawDir)) throw new Error(`raw dir not found: ${rawDir}`);
  const out = workDir(exp);
  rmSync(out, { recursive: true, force: true });
  mkdirSync(out, { recursive: true });
  mkdirSync(join(out, "results"), { recursive: true });

  const units = [];
  const missing = [];
  for (const { task, component } of MACRO_TASKS) {
    for (const cond of CONDS) {
      for (const sample of SAMPLES) {
        const ext = cond === "ps" ? "ps" : "psc";
        const md = join(rawDir, `${task}_${cond}_${sample}.md`);
        if (!existsSync(md)) { missing.push(`${task}_${cond}_${sample}`); continue; }
        const code = extractCodeBlock(readFileSync(md, "utf8"));
        const base = `${task}__${cond}__${sample}`;
        const unit = { task, component, cond, sample, base };
        if (!code) {
          // No fenced block -> record as un-materializable (behavioral fail: no_code).
          unit.file = null;
          unit.no_code = true;
        } else {
          const file = join(out, `${base}.${ext}`);
          writeFileSync(file, code, "utf8");
          unit.file = file;
        }
        units.push(unit);
      }
    }
  }

  const manifest = { exp, workDir: out, generatedAt: new Date().toISOString(), units };
  writeFileSync(join(out, "manifest.json"), JSON.stringify(manifest, null, 2), "utf8");
  return { manifest, missing };
}

if (import.meta.url === `file://${process.argv[1]}` || process.argv[1] === fileURLToPath(import.meta.url)) {
  const exp = process.argv[2] || "baseline-001";
  const { manifest, missing } = materialize(exp);
  const withCode = manifest.units.filter((u) => u.file).length;
  console.log(`[materialize] exp=${exp}: ${withCode}/${manifest.units.length} units written to ${manifest.workDir}`);
  if (missing.length) console.log(`[materialize] missing raw files: ${missing.join(", ")}`);
}
