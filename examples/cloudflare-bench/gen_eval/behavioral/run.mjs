// Behavioral macro oracle — end-to-end runner.
//
// For each experiment (Opus / Sonnet / Haiku):
//   1. materialize the generated macro completions into compilable scratch files
//   2. run the vitest behavioral suite (each generated component is rendered and
//      driven; the task's pinned behavior is asserted)
//   3. aggregate the per-unit result JSONs into behavioral pass@1 per (task,cond)
//      and overall, cross-referenced against token counts from results/<exp>.jsonl
//   4. write behavioral/results/<exp>.summary.json + append rows to the ablation
//      ledger (one row per condition, carrying behavioral_pass_rate)
//
// Usage: node run.mjs [exp ...]      (default: all three baselines)
//   exp ids map:  baseline-001 -> Opus, baseline-sonnet, baseline-haiku

import { spawnSync, execFileSync } from 'node:child_process'
import { readFileSync, writeFileSync, readdirSync, existsSync, appendFileSync, symlinkSync } from 'node:fs'
import { dirname, join, resolve } from 'node:path'
import { fileURLToPath } from 'node:url'
import { materialize, workDir, MACRO_TASKS } from './materialize.mjs'

const HERE = dirname(fileURLToPath(import.meta.url))
const GEN_EVAL = resolve(HERE, '..')
const RESULTS_DIR = process.env.GEN_EVAL_RESULTS_DIR || join(GEN_EVAL, 'results')
const LEDGER = resolve(GEN_EVAL, '..', 'bench', 'ablations', 'ledger.jsonl')
const REPO_ROOT = resolve(HERE, '..', '..', '..', '..')
const PYTHS_BIN = process.env.PYTHS_BIN || resolve(REPO_ROOT, 'target/release/pyths.exe')
// Run vitest's JS entry via node directly — the .bin shim is a bash script that
// spawnSync cannot exec on Windows.
const VITEST_JS = resolve(REPO_ROOT, 'examples/clones/node_modules/vitest/vitest.mjs')

const CONDS = ['ps', 'psc']
// exp id -> results/<file>.jsonl + human label
const EXP_META = {
  'baseline-001': { results: 'baseline-001.jsonl', label: 'opus', ledgerExp: 'behavioral-opus' },
  'baseline-sonnet': { results: 'baseline-sonnet.jsonl', label: 'sonnet', ledgerExp: 'behavioral-sonnet' },
  'baseline-haiku': { results: 'baseline-haiku.jsonl', label: 'haiku', ledgerExp: 'behavioral-haiku' },
  // v2: same suite, regenerated with the convention-aware authoring manual (PSX gotchas).
  'macrov2-opus': { results: 'macrov2-opus.jsonl', label: 'opus-v2', ledgerExp: 'behavioral-opus-v2' },
  'macrov2-sonnet': { results: 'macrov2-sonnet.jsonl', label: 'sonnet-v2', ledgerExp: 'behavioral-sonnet-v2' },
  'macrov2-haiku': { results: 'macrov2-haiku.jsonl', label: 'haiku-v2', ledgerExp: 'behavioral-haiku-v2' },
}

/** Built-in exps come from EXP_META; anything else (e.g. rerun-<model> from an
 *  external folder via GEN_EVAL_RAW_ROOT/GEN_EVAL_RESULTS_DIR) maps generically. */
function expMeta(exp) {
  if (EXP_META[exp]) return EXP_META[exp]
  const label = exp.replace(/^rerun-/, '')
  return { results: `${exp}.jsonl`, label, ledgerExp: `behavioral-${label}` }
}

/** vite-plugin-pyths, vitest, react etc. live in examples/clones/node_modules.
 *  The config (in this dir) resolves its imports from a sibling node_modules,
 *  so expose the clones deps here via a directory junction (Windows-friendly). */
function ensureNodeModules() {
  const link = join(HERE, 'node_modules')
  if (existsSync(link)) return
  const target = resolve(REPO_ROOT, 'examples/clones/node_modules')
  symlinkSync(target, link, 'junction')
  console.log('[run] created node_modules junction -> examples/clones/node_modules')
}

function commit() {
  try { return execFileSync('git', ['rev-parse', 'HEAD'], { cwd: REPO_ROOT, encoding: 'utf8' }).trim() }
  catch { return null }
}

function median(xs) {
  if (!xs.length) return null
  const s = [...xs].sort((a, b) => a - b)
  const m = Math.floor(s.length / 2)
  return s.length % 2 ? s[m] : (s[m - 1] + s[m]) / 2
}

/** o200k_out token count per (task,cond,sample) from the compile-eval results. */
function loadTokens(exp) {
  const file = join(RESULTS_DIR, expMeta(exp).results)
  const map = {}
  if (!existsSync(file)) return map
  for (const line of readFileSync(file, 'utf8').split('\n').filter(Boolean)) {
    let r
    try { r = JSON.parse(line) } catch { continue }
    if (r.kind !== 'macro') continue
    map[`${r.task}__${r.condition}__${r.sample}`] = { o200k: r.o200k_out, compilePass: r.pass }
  }
  return map
}

function runVitest(exp) {
  const r = spawnSync(process.execPath, [VITEST_JS, 'run', '--config', 'vitest.config.ts'], {
    cwd: HERE,
    encoding: 'utf8',
    timeout: 600_000,
    env: {
      ...process.env,
      PYTHS_BIN,
      BEHAVIORAL_EXP: exp,
      BEHAVIORAL_DIR: HERE.replace(/\\/g, '/'),
    },
  })
  return r
}

function loadResults(exp) {
  const dir = join(workDir(exp), 'results')
  const out = {}
  for (const f of readdirSync(dir).filter((f) => f.endsWith('.json'))) {
    const r = JSON.parse(readFileSync(join(dir, f), 'utf8'))
    out[`${r.task}__${r.cond}__${r.sample}`] = r
  }
  return out
}

function aggregate(exp, results, tokens) {
  const perTask = {}
  const overall = { ps: { pass: 0, n: 0 }, psc: { pass: 0, n: 0 } }
  // token accounting restricted to behaviorally-correct samples
  const correctTokens = { ps: [], psc: [] }

  for (const { task } of MACRO_TASKS) {
    perTask[task] = { ps: { pass: 0, n: 0 }, psc: { pass: 0, n: 0 } }
    for (const cond of CONDS) {
      for (let s = 1; s <= 5; s++) {
        const key = `${task}__${cond}__${s}`
        const r = results[key]
        if (!r) continue // e.g. a raw file that never existed
        perTask[task][cond].n++
        overall[cond].n++
        if (r.behavioral_pass) {
          perTask[task][cond].pass++
          overall[cond].pass++
          const tok = tokens[key]?.o200k
          if (typeof tok === 'number') correctTokens[cond].push(tok)
        }
      }
    }
  }

  const rate = (o) => (o.n ? o.pass / o.n : null)
  const medPs = median(correctTokens.ps)
  const medPsc = median(correctTokens.psc)
  const saving = medPs && medPsc ? 1 - medPsc / medPs : null

  return {
    exp,
    label: expMeta(exp).label,
    overall: {
      ps: { ...overall.ps, rate: rate(overall.ps) },
      psc: { ...overall.psc, rate: rate(overall.psc) },
    },
    perTask: Object.fromEntries(Object.entries(perTask).map(([t, o]) => [t, {
      ps: { ...o.ps, rate: rate(o.ps) },
      psc: { ...o.psc, rate: rate(o.psc) },
    }])),
    tokens_among_behaviorally_correct: {
      ps: { median_o200k: medPs, n: correctTokens.ps.length },
      psc: { median_o200k: medPsc, n: correctTokens.psc.length },
      median_saving_psc_vs_ps: saving,
    },
  }
}

function pct(x) { return x == null ? ' n/a ' : (100 * x).toFixed(0).padStart(3) + '%' }

function printSummary(agg) {
  console.log(`\n===== behavioral macro oracle — ${agg.label} (${agg.exp}) =====`)
  console.log('task                    .ps        .psc')
  for (const [task, o] of Object.entries(agg.perTask)) {
    const ps = `${pct(o.ps.rate)} (${o.ps.pass}/${o.ps.n})`
    const psc = `${pct(o.psc.rate)} (${o.psc.pass}/${o.psc.n})`
    console.log(`${task.padEnd(22)} ${ps.padEnd(11)} ${psc}`)
  }
  const O = agg.overall
  console.log(`${'OVERALL'.padEnd(22)} ${(pct(O.ps.rate) + ` (${O.ps.pass}/${O.ps.n})`).padEnd(11)} ${pct(O.psc.rate)} (${O.psc.pass}/${O.psc.n})`)
  const T = agg.tokens_among_behaviorally_correct
  console.log(`tokens among behaviorally-correct (median o200k): ps=${T.ps.median_o200k} (n=${T.ps.n})  psc=${T.psc.median_o200k} (n=${T.psc.n})`)
  console.log(`median token saving (.psc vs .ps), behaviorally-correct only: ${T.median_saving_psc_vs_ps == null ? 'n/a' : (100 * T.median_saving_psc_vs_ps).toFixed(1) + '%'}`)
}

function appendLedger(agg, sha) {
  const date = new Date().toISOString().slice(0, 10)
  const ledgerExp = expMeta(agg.exp).ledgerExp
  // Idempotent: drop any prior rows for this behavioral exp before re-appending.
  if (existsSync(LEDGER)) {
    const kept = readFileSync(LEDGER, 'utf8').split('\n').filter(Boolean).filter((l) => {
      try { return JSON.parse(l).exp_id !== ledgerExp } catch { return true }
    })
    writeFileSync(LEDGER, kept.join('\n') + (kept.length ? '\n' : ''), 'utf8')
  }
  for (const cond of CONDS) {
    const o = agg.overall[cond]
    const row = {
      exp_id: ledgerExp,
      date, commit: sha,
      corpus: 'gen_eval/tasks/tasks.jsonl',
      condition: cond,
      axis: { model: agg.label, phase: 'macro', oracle: 'behavioral' },
      metric: {
        behavioral_pass_rate: o.rate,
        behavioral_pass: o.pass,
        median_o200k_behaviorally_correct: agg.tokens_among_behaviorally_correct[cond].median_o200k,
      },
      n: o.n,
      raw_ref: `${process.env.GEN_EVAL_RAW_REF_PREFIX || 'gen_eval/raw/'}${agg.exp}/`,
    }
    appendFileSync(LEDGER, JSON.stringify(row) + '\n', 'utf8')
  }
}

function main() {
  const exps = process.argv.slice(2).length ? process.argv.slice(2) : Object.keys(EXP_META)
  ensureNodeModules()
  const sha = commit()
  const summaries = []
  for (const exp of exps) {
    console.log(`\n[run] materializing ${exp} ...`)
    let manifest, missing
    try { ({ manifest, missing } = materialize(exp)) }
    catch (e) { console.error(`[run] skipping ${exp}: ${e.message}`); continue }
    const withCode = manifest.units.filter((u) => u.file).length
    console.log(`[run] ${withCode}/${manifest.units.length} units; missing=${missing.length}`)
    console.log(`[run] running vitest for ${exp} ...`)
    const v = runVitest(exp)
    // vitest exit code is nonzero when any behavioral sample fails — expected.
    const tail = (v.stdout || '').split('\n').filter((l) => /Test Files|Tests /.test(l)).slice(-2)
    tail.forEach((l) => console.log('   ' + l.trim()))
    if (v.error) console.error('[run] vitest spawn error:', v.error)

    const tokens = loadTokens(exp)
    const results = loadResults(exp)
    const agg = aggregate(exp, results, tokens)
    printSummary(agg)
    writeFileSync(join(RESULTS_DIR, `${exp}.behavioral.summary.json`),
      JSON.stringify(agg, null, 2), 'utf8')
    appendLedger(agg, sha)
    summaries.push(agg)
  }
  writeFileSync(join(HERE, 'results.summary.json'), JSON.stringify(summaries, null, 2), 'utf8')
  console.log(`\n[run] wrote per-exp summaries to ${RESULTS_DIR} and ledger rows to ${LEDGER}`)
}

main()
