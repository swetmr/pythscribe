// Generation-token evaluation harness for PythScribe (Node 22).
//
// Measures whether an LLM actually saves OUTPUT TOKENS writing .psc / .ps
// versus plain Python, at non-inferior correctness.
//
// For each task x condition x sample:
//   1. compose prompt  = condition manual (prompts/<cond>.md) + task prompt
//   2. invoke model    = `claude -p --output-format json --max-turns 1 --tools ""` (stdin)
//   3. extract         = first fenced code block from the completion
//   4. count           = o200k tokens of the code block (tiktoken, batched in
//                        ONE python invocation at the end of the run)
//   5. verify          = python: run under CPython, byte-compare stdout
//                        ps:     pyths compile -> node run -> byte-compare
//                        psc:    pyths expand -> compile expansion -> run
//                        macro tasks: compile-success only (+expand for psc);
//                        macro tasks skip the python condition (a plain-Python
//                        program has no React-component equivalent).
//   6. persist         = raw/<exp_id>/<task>_<cond>_<n>.md, results JSONL,
//                        one ledger row per (condition x phase) aggregate in
//                        ../bench/ablations/ledger.jsonl
//
// Stdout comparison normalizes \r\n -> \n and strips ONE trailing newline on
// both sides (Windows CPython emits \r\n on pipes; Node emits \n).
//
// Temperature is NOT controllable through the claude CLI; the eval relies on
// N samples per cell and reports medians/IQR instead (see report.md).
//
// Usage:
//   node run_eval.mjs --exp-id dryrun-001 --tasks 3 --n 1
//   node run_eval.mjs --exp-id base-001 --n 5 [--model <m>] \
//        [--conditions python,ps,psc] [--tasks <N>|id1,id2] [--kind micro|macro]

import { spawnSync } from "node:child_process";
import { mkdirSync, readFileSync, writeFileSync, appendFileSync, existsSync, rmSync } from "node:fs";
import path from "node:path";
import { fileURLToPath, pathToFileURL } from "node:url";

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const REPO_ROOT = path.resolve(__dirname, "..", "..", "..");
const PYTHS_BIN = path.join(REPO_ROOT, "target", "release",
    process.platform === "win32" ? "pyths.exe" : "pyths");
const RUNTIME_INDEX = path.resolve(REPO_ROOT, "runtime", "src", "index.js");
const RUNTIME_ASYNCIO = path.resolve(REPO_ROOT, "runtime", "asyncio.js");
const RUNTIME_STDLIB_DIR = path.resolve(REPO_ROOT, "runtime", "src", "stdlib");
const TASKS_PATH = path.join(__dirname, "tasks", "tasks.jsonl");
const PROMPTS_DIR = path.join(__dirname, "prompts");
const RAW_DIR = path.join(__dirname, "raw");
const RESULTS_DIR = path.join(__dirname, "results");
const LEDGER_PATH = path.resolve(__dirname, "..", "bench", "ablations", "ledger.jsonl");
const SCRATCH = path.join(REPO_ROOT, "target", "gen_eval_scratch");

const ALL_CONDITIONS = ["python", "ps", "psc"];

// ---------------------------------------------------------------- CLI args
function parseArgs(argv) {
    const args = { n: 5, conditions: ALL_CONDITIONS, tasks: null, kind: null, model: null, expId: null };
    for (let i = 2; i < argv.length; i++) {
        const a = argv[i];
        const next = () => argv[++i];
        if (a === "--exp-id") args.expId = next();
        else if (a === "--n") args.n = parseInt(next(), 10);
        else if (a === "--tasks") args.tasks = next();
        else if (a === "--kind") args.kind = next();
        else if (a === "--model") args.model = next();
        else if (a === "--conditions") args.conditions = next().split(",").map(s => s.trim());
        else { console.error(`unknown arg: ${a}`); process.exit(2); }
    }
    if (!args.expId) { console.error("--exp-id is required"); process.exit(2); }
    for (const c of args.conditions) {
        if (!ALL_CONDITIONS.includes(c)) { console.error(`unknown condition: ${c}`); process.exit(2); }
    }
    return args;
}

// ---------------------------------------------------------------- helpers
function normStdout(s) {
    return s.replace(/\r\n/g, "\n").replace(/\n$/, "");
}

function gitCommit() {
    // No git subprocess (shared working tree); read .git files directly.
    try {
        const head = readFileSync(path.join(REPO_ROOT, ".git", "HEAD"), "utf8").trim();
        if (head.startsWith("ref: ")) {
            const ref = head.slice(5).trim();
            const refPath = path.join(REPO_ROOT, ".git", ...ref.split("/"));
            if (existsSync(refPath)) return readFileSync(refPath, "utf8").trim();
            const packed = readFileSync(path.join(REPO_ROOT, ".git", "packed-refs"), "utf8");
            for (const line of packed.split("\n")) {
                if (line.endsWith(" " + ref)) return line.split(" ")[0];
            }
        }
        return head;
    } catch { return null; }
}

function extractCodeBlock(text) {
    // First fenced code block, any info string.
    const m = /```[^\n]*\n([\s\S]*?)```/.exec(text);
    return m ? m[1].replace(/\s+$/, "") + "\n" : null;
}

/** Rewrite bare-specifier runtime imports to absolute file:// URLs so Node can
 *  run compiled JS without a package.json (mirrors tests/differential/run.mjs). */
function rewireImports(jsSource) {
    const runtimeUrl = pathToFileURL(RUNTIME_INDEX).href;
    const asyncioUrl = pathToFileURL(RUNTIME_ASYNCIO).href;
    return jsSource
        .replace(/from\s+["']pyths-runtime\/asyncio["']/g, `from "${asyncioUrl}"`)
        .replace(/from\s+["']pyths-runtime\/react["']/g, `from "${runtimeUrl}"`)
        .replace(/from\s+["']pyths-runtime\/stdlib\/([a-zA-Z0-9_]+)["']/g, (_m, mod) => {
            const url = pathToFileURL(path.join(RUNTIME_STDLIB_DIR, `${mod}.js`)).href;
            return `from "${url}"`;
        })
        .replace(/from\s+["']pyths-runtime["']/g, `from "${runtimeUrl}"`);
}

// ---------------------------------------------------------------- model call
function callModel(prompt, model) {
    // `--tools ""` disables ALL built-in tools so the model can only emit a
    // text completion. Without this the model sometimes spends its single
    // turn on a tool call (e.g. trying to run `pyths expand` to self-verify a
    // .psc answer) and exits with error_max_turns instead of producing code.
    const cliArgs = ["-p", "--output-format", "json", "--max-turns", "1", "--tools", ""];
    if (model) cliArgs.push("--model", model);
    const r = spawnSync("claude", cliArgs, {
        input: prompt, encoding: "utf8", timeout: 300_000,
        maxBuffer: 32 * 1024 * 1024,
    });
    if (r.error) return { error: String(r.error) };
    if (r.status !== 0) {
        const detail = [(r.stderr || "").trim(), (r.stdout || "").trim()].filter(Boolean).join(" | ");
        return { error: `claude exited ${r.status}: ${detail.slice(0, 500)}` };
    }
    try {
        const j = JSON.parse(r.stdout);
        if (j.is_error) return { error: `claude is_error: ${JSON.stringify(j).slice(0, 500)}` };
        return {
            text: j.result ?? "",
            costUsd: j.total_cost_usd ?? null,
            apiOutputTokens: j.usage?.output_tokens ?? null,
            model: j.modelUsage ? Object.keys(j.modelUsage)[0] : null,
        };
    } catch (e) {
        return { error: `bad JSON from claude: ${String(e)}` };
    }
}

// ---------------------------------------------------------------- verifiers
// Verdicts: pass | fail_output | fail_compile | fail_expand | fail_runtime
//           | no_code_block | model_error | skipped_na
// syntax_err = true for no_code_block / fail_compile / fail_expand /
//              (python) SyntaxError-class failures.

function runNodeJs(jsPath) {
    const r = spawnSync("node", [jsPath], { encoding: "utf8", timeout: 20_000, maxBuffer: 8 * 1024 * 1024 });
    return r;
}

function verifyPython(code, task, work) {
    const pyPath = path.join(work, "sol.py");
    writeFileSync(pyPath, code, "utf8");
    const r = spawnSync("python", ["-X", "utf8", pyPath], { encoding: "utf8", timeout: 20_000 });
    if (r.error || r.status !== 0) {
        const stderr = r.stderr || String(r.error || "");
        const syntax = /SyntaxError|IndentationError|TabError/.test(stderr);
        return { verdict: "fail_runtime", syntaxErr: syntax, detail: stderr.slice(-800) };
    }
    const got = normStdout(r.stdout);
    return got === task.expected_stdout
        ? { verdict: "pass", syntaxErr: false, stdout: got }
        : { verdict: "fail_output", syntaxErr: false, stdout: got };
}

function compilePs(psPath) {
    return spawnSync(PYTHS_BIN, ["compile", psPath], {
        encoding: "utf8", timeout: 60_000,
        env: { ...process.env, PYTHS_NO_CACHE: "1" },
    });
}

function verifyPsSource(psPath, task, work) {
    const compile = compilePs(psPath);
    if (compile.error || compile.status !== 0) {
        return { verdict: "fail_compile", syntaxErr: true, detail: (compile.stderr || String(compile.error || "")).slice(-800) };
    }
    if (task.kind === "macro") return { verdict: "pass", syntaxErr: false };
    const jsPath = psPath.replace(/\.ps$/, ".js");
    const rewired = rewireImports(readFileSync(jsPath, "utf8"));
    const mjsPath = path.join(work, "sol.run.mjs");
    writeFileSync(mjsPath, rewired, "utf8");
    const node = runNodeJs(mjsPath);
    if (node.error || node.status !== 0) {
        return { verdict: "fail_runtime", syntaxErr: false, detail: (node.stderr || String(node.error || "")).slice(-800) };
    }
    const got = normStdout(node.stdout);
    return got === task.expected_stdout
        ? { verdict: "pass", syntaxErr: false, stdout: got }
        : { verdict: "fail_output", syntaxErr: false, stdout: got };
}

function verifyPs(code, task, work) {
    const psPath = path.join(work, "sol.ps");
    writeFileSync(psPath, code, "utf8");
    return verifyPsSource(psPath, task, work);
}

function verifyPsc(code, task, work) {
    const pscPath = path.join(work, "sol.psc");
    writeFileSync(pscPath, code, "utf8");
    const expandedPath = path.join(work, "sol.expanded.ps");
    const expand = spawnSync(PYTHS_BIN, ["expand", pscPath, "-o", expandedPath], {
        encoding: "utf8", timeout: 60_000,
        env: { ...process.env, PYTHS_NO_CACHE: "1" },
    });
    if (expand.error || expand.status !== 0 || !existsSync(expandedPath)) {
        return { verdict: "fail_expand", syntaxErr: true, detail: (expand.stderr || String(expand.error || "")).slice(-800) };
    }
    return verifyPsSource(expandedPath, task, work);
}

// ---------------------------------------------------------------- tokens
/** Batch-count o200k tokens for {key: text} in ONE python invocation. */
function countTokensBatch(byKey) {
    const inPath = path.join(SCRATCH, "tok_in.json");
    const outPath = path.join(SCRATCH, "tok_out.json");
    writeFileSync(inPath, JSON.stringify(byKey), "utf8");
    const script = [
        "import json, sys, tiktoken",
        "enc = tiktoken.get_encoding('o200k_base')",
        `data = json.load(open(r'${inPath}', encoding='utf-8'))`,
        "out = {k: len(enc.encode(v)) for k, v in data.items()}",
        `json.dump(out, open(r'${outPath}', 'w'))`,
    ].join("\n");
    const r = spawnSync("python", ["-X", "utf8", "-c", script], { encoding: "utf8", timeout: 120_000 });
    if (r.status !== 0) throw new Error(`tiktoken batch count failed: ${r.stderr}`);
    return JSON.parse(readFileSync(outPath, "utf8"));
}

// ---------------------------------------------------------------- stats
function median(xs) {
    if (!xs.length) return null;
    const s = [...xs].sort((a, b) => a - b);
    const mid = Math.floor(s.length / 2);
    return s.length % 2 ? s[mid] : (s[mid - 1] + s[mid]) / 2;
}
function quartile(xs, q) {
    if (!xs.length) return null;
    const s = [...xs].sort((a, b) => a - b);
    const pos = (s.length - 1) * q;
    const lo = Math.floor(pos), hi = Math.ceil(pos);
    return s[lo] + (s[hi] - s[lo]) * (pos - lo);
}
function iqr(xs) {
    if (!xs.length) return null;
    return +(quartile(xs, 0.75) - quartile(xs, 0.25)).toFixed(1);
}

// ---------------------------------------------------------------- main
const args = parseArgs(process.argv);

const allTasks = readFileSync(TASKS_PATH, "utf8").split("\n").filter(Boolean).map(l => JSON.parse(l));
let tasks = allTasks;
if (args.kind) tasks = tasks.filter(t => t.kind === args.kind);
if (args.tasks) {
    if (/^\d+$/.test(args.tasks)) tasks = tasks.slice(0, parseInt(args.tasks, 10));
    else {
        const ids = new Set(args.tasks.split(","));
        tasks = tasks.filter(t => ids.has(t.id));
    }
}
if (!tasks.length) { console.error("no tasks selected"); process.exit(2); }
if (!existsSync(PYTHS_BIN)) { console.error(`pyths binary missing: ${PYTHS_BIN} — run cargo build --release`); process.exit(2); }

const prompts = {};
for (const c of ALL_CONDITIONS) prompts[c] = readFileSync(path.join(PROMPTS_DIR, `${c}.md`), "utf8");

const rawDir = path.join(RAW_DIR, args.expId);
mkdirSync(rawDir, { recursive: true });
mkdirSync(RESULTS_DIR, { recursive: true });
mkdirSync(SCRATCH, { recursive: true });

const commit = gitCommit();
const date = new Date().toISOString().slice(0, 10);
const rows = [];
const codeByKey = {};   // for the batched token count

console.log(`[gen_eval] exp=${args.expId} tasks=${tasks.length} conditions=${args.conditions.join(",")} n=${args.n} model=${args.model ?? "(session default)"}`);

for (const task of tasks) {
    for (const cond of args.conditions) {
        if (task.kind === "macro" && cond === "python") {
            console.log(`  ${task.id} x python: skipped_na (no plain-Python equivalent for a component)`);
            continue;
        }
        for (let s = 1; s <= args.n; s++) {
            const key = `${task.id}_${cond}_${s}`;
            const fullPrompt = `${prompts[cond]}\n\n---\n\n# Task\n\n${task.prompt}\n`;
            const t0 = Date.now();
            const resp = callModel(fullPrompt, args.model);
            const callMs = Date.now() - t0;

            const row = {
                exp_id: args.expId, task: task.id, kind: task.kind, condition: cond, sample: s,
                verdict: null, pass: false, syntax_err: false,
                o200k_out: null, api_output_tokens: resp.apiOutputTokens ?? null,
                cost_usd: resp.costUsd ?? null, call_ms: callMs,
                model: resp.model ?? args.model ?? null,
                raw_ref: `raw/${args.expId}/${key}.md`,
            };

            if (resp.error) {
                row.verdict = "model_error"; row.detail = resp.error;
                writeFileSync(path.join(rawDir, `${key}.md`), `<!-- model_error -->\n${resp.error}\n`, "utf8");
            } else {
                writeFileSync(path.join(rawDir, `${key}.md`),
                    `<!-- exp=${args.expId} task=${task.id} cond=${cond} sample=${s} model=${row.model} cost_usd=${row.cost_usd} -->\n${resp.text}\n`, "utf8");
                const code = extractCodeBlock(resp.text);
                if (!code) {
                    row.verdict = "no_code_block"; row.syntax_err = true;
                } else {
                    codeByKey[key] = code;
                    const work = path.join(SCRATCH, key);
                    rmSync(work, { recursive: true, force: true });
                    mkdirSync(work, { recursive: true });
                    let v;
                    if (cond === "python") v = verifyPython(code, task, work);
                    else if (cond === "ps") v = verifyPs(code, task, work);
                    else v = verifyPsc(code, task, work);
                    row.verdict = v.verdict;
                    row.pass = v.verdict === "pass";
                    row.syntax_err = !!v.syntaxErr;
                    if (v.stdout !== undefined && !row.pass) row.got_stdout = v.stdout;
                    if (v.detail) row.detail = v.detail;
                }
            }
            rows.push(row);
            console.log(`  ${key}: ${row.verdict}${row.cost_usd != null ? ` ($${row.cost_usd.toFixed(3)})` : ""} [${(callMs / 1000).toFixed(1)}s]`);
        }
    }
}

// ---- batched o200k counts: completions + the three condition prompts ----
const toCount = { ...codeByKey };
for (const c of ALL_CONDITIONS) toCount[`__prompt_${c}`] = prompts[c];
const counts = Object.keys(toCount).length ? countTokensBatch(toCount) : {};
for (const row of rows) {
    const key = `${row.task}_${row.condition}_${row.sample}`;
    if (counts[key] !== undefined) row.o200k_out = counts[key];
}
const skillOverhead = Object.fromEntries(ALL_CONDITIONS.map(c => [c, counts[`__prompt_${c}`] ?? null]));

// ---- persist results ----
const resultsPath = path.join(RESULTS_DIR, `${args.expId}.jsonl`);
writeFileSync(resultsPath, rows.map(r => JSON.stringify(r)).join("\n") + "\n", "utf8");

// ---- ledger: one row per (condition x phase) aggregate ----
const ledgerRows = [];
for (const cond of args.conditions) {
    for (const phase of ["micro", "macro"]) {
        const sub = rows.filter(r => r.condition === cond && r.kind === phase && r.verdict !== "model_error");
        if (!sub.length) continue;
        const toks = sub.filter(r => r.o200k_out != null).map(r => r.o200k_out);
        ledgerRows.push({
            exp_id: args.expId,
            date,
            commit,
            corpus: "gen_eval/tasks/tasks.jsonl",
            condition: cond,
            axis: { tier_subset: cond === "psc" ? "A+B+C+dict(bundled)" : null, model: rows.find(r => r.model)?.model ?? args.model ?? "session-default", phase },
            metric: {
                o200k_out_median: median(toks),
                o200k_out_iqr: iqr(toks),
                pass_rate: +(sub.filter(r => r.pass).length / sub.length).toFixed(3),
                syntax_err_rate: +(sub.filter(r => r.syntax_err).length / sub.length).toFixed(3),
                skill_overhead_o200k: skillOverhead[cond],
            },
            n: sub.length,
            raw_ref: `gen_eval/raw/${args.expId}/`,
        });
    }
}
mkdirSync(path.dirname(LEDGER_PATH), { recursive: true });
for (const lr of ledgerRows) appendFileSync(LEDGER_PATH, JSON.stringify(lr) + "\n", "utf8");

// ---- summary ----
console.log(`\n[gen_eval] results -> ${resultsPath}`);
console.log(`[gen_eval] ledger  -> ${LEDGER_PATH} (+${ledgerRows.length} rows)`);
console.log(`[gen_eval] skill overhead (o200k prompt tokens): python=${skillOverhead.python} ps=${skillOverhead.ps} psc=${skillOverhead.psc}`);
const totalCost = rows.reduce((a, r) => a + (r.cost_usd || 0), 0);
console.log(`[gen_eval] total cost: $${totalCost.toFixed(2)} across ${rows.length} calls`);
for (const lr of ledgerRows) {
    const m = lr.metric;
    console.log(`  ${lr.condition}/${lr.axis.phase}: n=${lr.n} median_out=${m.o200k_out_median} iqr=${m.o200k_out_iqr} pass=${m.pass_rate} syntax_err=${m.syntax_err_rate}`);
}
