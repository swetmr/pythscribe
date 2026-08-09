#!/usr/bin/env node
// scripts/test-readme.mjs — Sweep B (S4): README/docs code blocks execute verbatim.
//
// Extracts every fenced code block from README.md plus the snippet blocks of
// docs/language-reference.md and docs/multi-file-apps.md and verifies them
// against the built `pyths` binary — npm-free (node + binary only).
//
// Policy per fence language:
//   python / ps  → written to a temp .ps file, `pyths compile` must succeed.
//                  README blocks that declare a filename header (`# name.ps`),
//                  contain print() and don't import React are additionally
//                  `pyths run` (exit 0 required). language-reference blocks
//                  are compile-only: their `# → ...` comments are JS-mapping
//                  annotations, NOT expected stdout.
//   bash         → executed line-by-line against the temp workspace, but only
//                  SAFE allowlisted commands: `pyths compile|run|check|fmt|lint`
//                  and `node <workspace .js>`. Everything else (git clone,
//                  cargo, cd, npm link/install, npx, pyths test/bundle/init)
//                  is skipped by command-word matching — packaging and
//                  install flows are covered separately by S6
//                  (scripts/test-packaging.mjs). Comment lines of the form
//                  `# → expected` directly after a command assert that the
//                  command's stdout matches exactly.
//   js / javascript / (no language) → skipped: these are illustrative
//                  compiled-output examples, config snippets (vite.config.js /
//                  next.config.mjs — exercised for real by S6) or directory
//                  trees, not executable PythScribe.
//
// language-reference.md scope: python blocks only. Its few bash blocks
// (`pyths check file.ps`, ...) reference placeholder filenames and duplicate
// the CLI surface already executed via README's CLI Reference blocks.
//
// Every extracted block must pass OR appear in an exclusion list below with
// a paper-trail comment (doc-fix / known-limitation / GitHub issue number).

import { mkdtempSync, writeFileSync, mkdirSync, readFileSync, rmSync, symlinkSync } from "node:fs";
import { spawnSync } from "node:child_process";
import { tmpdir } from "node:os";
import { join, dirname } from "node:path";
import { fileURLToPath } from "node:url";

const ROOT = join(dirname(fileURLToPath(import.meta.url)), "..");
const EXE = process.platform === "win32" ? ".exe" : "";
const PYTHS = process.env.PYTHS_BIN || join(ROOT, "target", "release", `pyths${EXE}`);

// ---------------------------------------------------------------------------
// Exclusions — the paper trail. Keyed by a distinctive substring of the block.
// ---------------------------------------------------------------------------

// Whole blocks excluded from testing entirely.
// (Empty since the Sweep-A fix batch: #99 chained assignment, #100 raw
// strings, and #101 del attr/subscript are fixed — those doc blocks
// execute verbatim again.)
const EXCLUDED_BLOCKS = [];

// Output assertions (`# → ...`) waived — the command must still exit 0.
// (Empty since the Sweep-A fix batch: #97 __str__ dispatch is fixed, so
// README's todo.ps output reproduces byte-for-byte.)
const EXCLUDED_OUTPUT_ASSERTS = [];

// ---------------------------------------------------------------------------
// Fenced-block extraction (handles fences indented inside list items).
// ---------------------------------------------------------------------------

function extractBlocks(relPath) {
    const text = readFileSync(join(ROOT, relPath), "utf-8");
    const lines = text.split(/\r?\n/);
    const blocks = [];
    let open = null; // { indent, lang, start, body: [] }
    for (let i = 0; i < lines.length; i++) {
        const m = lines[i].match(/^(\s*)```(\S*)\s*$/);
        if (m && !open) {
            open = { indent: m[1].length, lang: m[2].toLowerCase(), start: i + 1, body: [] };
        } else if (m && open) {
            blocks.push({
                file: relPath,
                line: open.start,
                lang: open.lang,
                code: open.body.join("\n"),
            });
            open = null;
        } else if (open) {
            open.body.push(lines[i].slice(open.indent));
        }
    }
    if (open) throw new Error(`${relPath}: unterminated fence at line ${open.start}`);
    return blocks;
}

// ---------------------------------------------------------------------------
// Runner helpers
// ---------------------------------------------------------------------------

const stripAnsi = (s) => s.replace(/\x1b\[[0-9;]*m/g, "");

function run(cmd, args, cwd) {
    const r = spawnSync(cmd, args, { cwd, encoding: "utf-8", timeout: 60000 });
    return {
        status: r.status,
        stdout: stripAnsi(r.stdout ?? ""),
        stderr: stripAnsi(r.stderr ?? ""),
    };
}

const results = { pass: 0, fail: 0, skip: 0, excluded: 0 };
const failures = [];

function report(kind, block, detail, ok, why) {
    const id = `${block.file}:${block.line} [${block.lang || "text"}] ${detail}`;
    if (ok === "skip") {
        results.skip++;
        if (process.env.VERBOSE) console.log(`  SKIP ${id} — ${why}`);
    } else if (ok === "excluded") {
        results.excluded++;
        console.log(`  EXCL ${id} — ${why}`);
    } else if (ok) {
        results.pass++;
        console.log(`  ok   ${id}`);
    } else {
        results.fail++;
        failures.push(`${id}\n       ${why}`);
        console.log(`  FAIL ${id} — ${why}`);
    }
}

// ---------------------------------------------------------------------------
// Workspace — one shared temp dir so bash blocks can reference files created
// by earlier python blocks (`# hello.ps` headers), exactly like a reader
// following the README top-to-bottom.
// ---------------------------------------------------------------------------

const ws = mkdtempSync(join(tmpdir(), "pyths-readme-"));
// Directories README examples write into / operate on:
//   dist/ — `pyths compile app.ps -o dist/app.js` (the CLI does not create
//           missing parent dirs; raw "os error 3" otherwise — issue #98)
//   src/  — `pyths fmt src/`, `pyths lint src/` need a project-like dir
mkdirSync(join(ws, "dist"));
mkdirSync(join(ws, "src"));
writeFileSync(join(ws, "src", "util.ps"), 'print("Hello, World!")\n');

// README `node xyz.js` blocks import the bare specifier "pyths-runtime" (the
// compiler emits `import { ... } from "pyths-runtime"`). Make it resolvable
// from the workspace exactly as `npm i pyths-runtime` would for a reader:
// symlink the repo's runtime package into node_modules. ("junction" is a
// Windows dir-junction — no admin needed — and a normal symlink elsewhere.)
mkdirSync(join(ws, "node_modules"));
symlinkSync(join(ROOT, "runtime"), join(ws, "node_modules", "pyths-runtime"), "junction");

let langrefCounter = 0;

function isExcludedBlock(block) {
    return EXCLUDED_BLOCKS.find((e) => block.code.includes(e.match));
}

// ---------------------------------------------------------------------------
// python/ps block handler
// ---------------------------------------------------------------------------

function testPythonBlock(block, { runnable }) {
    const header = block.code.match(/^#\s*([\w.-]+\.ps)\s*$/m);
    const name = header ? header[1] : `langref_${String(++langrefCounter).padStart(3, "0")}.ps`;
    const path = join(ws, name);
    writeFileSync(path, block.code + "\n");

    const c = run(PYTHS, ["compile", name, "--stdout"], ws);
    if (c.status !== 0) {
        report("compile", block, `compile ${name}`, false, c.stderr.trim().split("\n")[0]);
        return;
    }
    report("compile", block, `compile ${name}`, true);

    // Self-contained deterministic scripts (README-named, prints, non-React)
    // must also run cleanly. Output equality is asserted separately by the
    // README's own bash blocks (`# → ...`).
    const isReact = /from\s+(pyths\.react|react)\s+import|"use client"/.test(block.code);
    if (runnable && header && !isReact && block.code.includes("print(")) {
        const r = run(PYTHS, ["run", name], ws);
        report("run", block, `run ${name}`, r.status === 0,
            r.status === 0 ? "" : (r.stderr.trim().split("\n").slice(-3).join(" | ") || `exit ${r.status}`));
    }
}

// ---------------------------------------------------------------------------
// bash block handler
// ---------------------------------------------------------------------------

const PYTHS_SAFE_SUBCOMMANDS = new Set(["compile", "run", "check", "fmt", "lint"]);
const SKIP_COMMAND_WORDS = new Set([
    "git", "cd", "cargo", "npm", "npx", "pip", "curl", "for", "do", "done", "export",
]);

function testBashBlock(block) {
    const lines = block.code.split("\n");
    // Parse into [{ cmd, expected: [...] }]
    const steps = [];
    for (const raw of lines) {
        const line = raw.trim();
        if (!line) continue;
        const arrow = line.match(/^#\s*(?:→|->)\s?(.*)$/) || line.match(/^#\s*prints\s+(.*)$/i);
        if (arrow) {
            if (steps.length) steps[steps.length - 1].expected.push(arrow[1]);
            continue;
        }
        if (line.startsWith("#")) continue; // plain comment
        // Strip trailing inline comments (`pyths compile app.ps   # → app.js`):
        // an inline `# → file` annotates the artifact produced, not stdout.
        // Doc commands never contain a quoted or escaped `#`.
        const cmd = line.split(/\s+#/)[0].trim();
        if (!cmd) continue;
        steps.push({ cmd, expected: [] });
    }

    for (const step of steps) {
        const words = step.cmd.split(/\s+/);
        const word0 = words[0];

        let exec = null;
        if (word0 === "pyths" && PYTHS_SAFE_SUBCOMMANDS.has(words[1])) {
            exec = [PYTHS, words.slice(1)];
        } else if (word0 === "node" && words.length === 2 && words[1].endsWith(".js") && !words[1].includes("/")) {
            exec = ["node", [words[1]]]; // node on a workspace-local artifact only
        }

        if (!exec) {
            report("bash", block, `\`${step.cmd}\``, "skip", "not in safe allowlist (packaging/install covered by S6)");
            continue;
        }

        const r = run(exec[0], exec[1], ws);
        if (r.status !== 0) {
            report("bash", block, `\`${step.cmd}\``, false,
                (r.stderr.trim() || r.stdout.trim()).split("\n").slice(-2).join(" | ") || `exit ${r.status}`);
            continue;
        }

        if (step.expected.length > 0) {
            const waived = EXCLUDED_OUTPUT_ASSERTS.find((e) => step.cmd.includes(e.match));
            if (waived) {
                report("bash", block, `\`${step.cmd}\` output`, "excluded", waived.reason);
                continue;
            }
            const got = r.stdout.split(/\r?\n/).map((l) => l.trimEnd()).filter((l) => l !== "");
            const want = step.expected.map((l) => l.trimEnd());
            const match = got.length === want.length && got.every((l, i) => l === want[i]);
            report("bash", block, `\`${step.cmd}\` output`, match,
                match ? "" : `expected ${JSON.stringify(want)} got ${JSON.stringify(got)}`);
        } else {
            report("bash", block, `\`${step.cmd}\``, true);
        }
    }
}

// ---------------------------------------------------------------------------
// Main
// ---------------------------------------------------------------------------

console.log(`pyths binary: ${PYTHS}`);
console.log(`workspace:    ${ws}\n`);

const v = run(PYTHS, ["--version"], ws);
if (v.status !== 0) {
    console.error("FATAL: pyths binary not runnable. Build with `cargo build --release` or set PYTHS_BIN.");
    process.exit(2);
}

const plans = [
    // README: everything is fair game; named python blocks also run.
    { file: "README.md", langs: ["python", "ps", "bash"], runnable: true },
    // language-reference: python snippet blocks, compile-only (see header note).
    { file: "docs/language-reference.md", langs: ["python", "ps"], runnable: false },
    // multi-file-apps: python snippet blocks, compile-only. Its js/toml
    // blocks are bundler/config snippets and its one text block is a
    // directory tree — non-executable by policy, same as language-reference.
    { file: "docs/multi-file-apps.md", langs: ["python", "ps"], runnable: false },
];

for (const plan of plans) {
    console.log(`── ${plan.file} ──`);
    const blocks = extractBlocks(plan.file);
    for (const block of blocks) {
        if (!plan.langs.includes(block.lang)) {
            report("lang", block, `(${block.lang || "text"} block)`, "skip", "non-executable language by policy");
            continue;
        }
        const excl = isExcludedBlock(block);
        if (excl) {
            report("block", block, "(whole block)", "excluded", excl.reason);
            continue;
        }
        if (block.lang === "bash") testBashBlock(block);
        else testPythonBlock(block, { runnable: plan.runnable });
    }
    console.log();
}

rmSync(ws, { recursive: true, force: true });

console.log("──────────────────────────────────────────");
console.log(`pass: ${results.pass}  fail: ${results.fail}  excluded: ${results.excluded}  skipped: ${results.skip}`);
if (failures.length) {
    console.log("\nFailures:");
    for (const f of failures) console.log(`  - ${f}`);
    process.exit(1);
}
console.log("S4 README/docs verbatim suite: GREEN");
