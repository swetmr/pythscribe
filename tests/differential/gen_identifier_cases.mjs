// S1 — identifier/key sanitization suite (generated, exhaustive).
//
// For every word in WORDS, first determines *programmatically* (by shelling
// out to `python -c "compile(...)"` and checking the exit code) whether the
// word is usable as a Python identifier at all. This excludes true Python
// keywords (`in`, `return`, `else`, `import`, `while`, `with`, `yield`,
// `await`, `finally`, ...) automatically — no hardcoded exclusion list.
//
// For every word that survives, generates 5 differential cases (variable
// binding, function param, function name, dict key, class-instance
// attribute) and runs each through the same CPython-vs-PythScribe
// byte-for-byte harness as `run.mjs` (via the shared `harness.mjs` module).
//
// Run:  node tests/differential/gen_identifier_cases.mjs

import { checkPythonAvailable, runPython, runCase } from "./harness.mjs";
import { ORACLE_BIN, oracleArgs } from "./oracle_python.mjs";
import { spawnSync } from "node:child_process";

const WORDS = (
    "let const var new function this typeof delete void switch case default " +
    "catch finally do else enum export extends import in instanceof return " +
    "super throw while with yield await static debugger null true false " +
    "undefined NaN Infinity arguments eval constructor prototype __proto__ " +
    "hasOwnProperty toString valueOf"
).split(/\s+/);

if (!checkPythonAvailable()) {
    console.log("[S1] python not on PATH — skipping identifier suite");
    process.exit(0);
}

// --- Step 1: programmatic Python-identifier-validity filter -----------
function isValidPythonIdentifier(word) {
    const code = `compile(${JSON.stringify(word + " = 1")}, "<string>", "exec")`;
    const r = spawnSync(ORACLE_BIN, oracleArgs(["-c", code]), { encoding: "utf8" });
    return r.status === 0;
}

const surviving = WORDS.filter(isValidPythonIdentifier);
const excludedAsPythonKeyword = WORDS.filter((w) => !surviving.includes(w));

console.log(
    `[S1] ${WORDS.length} words total; ${surviving.length} are legal Python ` +
    `identifiers (excluded as Python keywords: ${excludedAsPythonKeyword.join(", ")})`
);

// --- Step 2: known-limitations skip-list --------------------------------
// None of the surviving words hit any of the 5 *pre-existing* documented
// residuals (those concern default-arg evaluation, float division messages,
// whole-float display, and `next(gen, default)` — none of which this
// suite's templates touch). This sweep originally surfaced 3 real bugs
// (#79 `case` hard-keyword, #80 capitalized-function `new`-call,
// #81 `__proto__` attribute no-op); all three were fixed in the Sweep-A
// fix batch.
//
// `eval:fn_name` is skipped BY DESIGN, not as a bug: that template emits
// `def eval(): ...` followed by the call `eval()`, and PythScribe — an
// ahead-of-time compiler — rejects any `eval(...)` call site at compile
// time (it emits a runtime throw), even when a user `def` shadows the
// builtin as CPython scoping would allow. That is an intentional
// AOT-unsupported-dynamic-builtin contract (same class as `exec`,
// `compile`, `__import__` — none of which appear in WORDS), so the case
// can never pass a byte-for-byte differential and is not an
// identifier-sanitization concern. `eval` as a *name* stays fully covered
// by the other 4 templates (var_binding, fn_param, dict_key, attr_name),
// which never call it.
const SKIP = new Set([
    "eval:fn_name", // by-design AOT divergence: `eval()` call site is rejected at compile time
]);

// --- Step 3: the 5 templates per surviving word -------------------------
function templatesFor(w) {
    return [
        {
            template: "var_binding",
            _setup: `${w} = 1`,
            expr: `${w} + 1`,
        },
        {
            template: "fn_param",
            _setup: `def f(${w}):\n    return ${w} + 1`,
            expr: `f(5)`,
        },
        {
            template: "fn_name",
            _setup: `def ${w}():\n    return 42`,
            expr: `${w}()`,
        },
        {
            template: "dict_key",
            _setup: `d = {"${w}": 10}`,
            expr: `[d["${w}"], "${w}" in d, len(d)]`,
        },
        {
            template: "attr_name",
            _setup: `class C:\n    def __init__(self):\n        self.${w} = 7\nc = C()`,
            expr: `c.${w}`,
        },
    ];
}

// --- Step 4: run every surviving-word × template case -------------------
let pass = 0, fail = 0;
const failures = [];
const excludedCases = [];

for (const w of surviving) {
    for (const t of templatesFor(w)) {
        const skipKey = `${w}:${t.template}`;
        if (SKIP.has(skipKey)) {
            excludedCases.push(skipKey);
            continue;
        }
        const id = `s1_${w.replace(/[^a-zA-Z0-9_]/g, "_")}_${t.template}`;
        const result = runCase({ id, _setup: t._setup, expr: t.expr });
        if (result.pass) {
            pass++;
        } else {
            fail++;
            failures.push({ id, word: w, template: t.template, setup: t._setup, expr: t.expr, ...result });
        }
    }
}

const total = pass + fail;
console.log(`[S1] identifier suite: ${pass} pass / ${total} total (${excludedCases.length} excluded via skip-list)`);
for (const f of failures) {
    console.log(`  - ${f.id} (word=${JSON.stringify(f.word)}, template=${f.template})`);
    console.log(`      setup: ${f.setup}`);
    console.log(`      expr:  ${f.expr}`);
    if (f.why) console.log(`      error: ${f.why}`);
    else {
        console.log(`      cpython:    ${JSON.stringify(f.py)}`);
        console.log(`      pythscribe: ${JSON.stringify(f.ps)}`);
    }
}
process.exit(fail === 0 ? 0 : 1);
