// S2 — grammar-based differential fuzzer (deterministic, seeded).
//
// Generates small Python programs from a grammar restricted to constructs
// documented as supported in docs/language-reference.md / README.md (and
// cross-checked against crates/pyths_codegen_js/src/method_table.rs for
// str/list/dict method calls, so only methods that actually exist are
// generated), then diffs CPython vs compiled-and-run PythScribe stdout
// byte-for-byte via the shared harness.mjs.
//
// Grammar covers (see README/language-reference.md sections cited inline):
//   - int/float/str/bool/None literals; list/dict/tuple/set literals
//   - arithmetic/comparison/boolean ops, random nesting, NO inserted parens
//     (stresses operator-precedence parsing — see "Operators" section)
//   - unary -/not; ternary; chained comparison (`a < b < c`)
//   - lambda, including IIFE `(lambda x: x + 1)(3)`
//   - list comprehensions with an optional `if` condition
//   - slicing/indexing including negative indices
//   - f-strings
//   - str/list/dict method calls (method_table.rs-verified only)
//   - for/while loops with break/continue
//   - try/except of builtin error types (IndexError/KeyError/ZeroDivisionError)
//   - multiple assignment / unpacking (`a, b = b, a`)
//
// Deliberately excluded (documented known-limitations / Sweep-A residuals,
// see docs/known-limitations.md):
//   - whole-valued floats through untracked channels (pre-existing A4) — the
//     post-hoc `looksLikeWholeFloatResidual()` filter below skips any
//     candidate whose CPython output would trip this, so the suite never
//     asserts on a case the compiler is known not to handle.
//   - zero divisors in general arithmetic (would hit the float
//     division-by-zero-message residual, F4, when a float flows through a
//     variable) — the arithmetic generator always regenerates a nonzero
//     divisor for `/`, `//`, `%`.
//   - `zip()` / `enumerate()` results repr'd directly as nested structures —
//     NOT a known-limitations residual, but a documented representational
//     fact (README: "tuple → [1, 2, 3] (arrays in JS)"): tuples and lists
//     share one untagged JS-array representation, so values *returned* by
//     zip/enumerate (not literal tuple syntax, which IS tracked and reprs
//     correctly with parens) print as `[...]` not `(...)`. The grammar
//     always consumes zip/enumerate via for-loop unpacking, never a direct
//     repr of the raw return value.
//
// Previously excluded, RE-ENABLED by the Sweep-A fix batch: `.title()`
// composition (#87), `.2f` on division results (#86), the swap idiom and
// nested unpack targets (#84/#85), and int("...")/ValueError triggers
// (#82).
//
// Run:  node tests/differential/fuzz_gen.mjs
// Wider local sweep:  node tests/differential/fuzz_gen.mjs --n 5000 --seed 12345

import { checkPythonAvailable, runPython, runCase } from "./harness.mjs";

const DEFAULT_SEED = 20260703;
const DEFAULT_N = 300;

function parseArgs(argv) {
    let seed = DEFAULT_SEED, n = DEFAULT_N;
    for (let i = 0; i < argv.length; i++) {
        if (argv[i] === "--seed") seed = Number(argv[++i]);
        else if (argv[i] === "--n") n = Number(argv[++i]);
    }
    return { seed, n };
}

// --- Deterministic seeded PRNG (mulberry32) -----------------------------
function mulberry32(seed) {
    let a = seed >>> 0;
    return function () {
        a |= 0; a = (a + 0x6D2B79F5) | 0;
        let t = Math.imul(a ^ (a >>> 15), 1 | a);
        t = (t + Math.imul(t ^ (t >>> 7), 61 | t)) ^ t;
        return ((t ^ (t >>> 14)) >>> 0) / 4294967296;
    };
}

function pick(rng, arr) { return arr[Math.floor(rng() * arr.length)]; }
function int(rng, lo, hi) { return lo + Math.floor(rng() * (hi - lo + 1)); }
function nonzeroInt(rng, lo, hi) {
    let v;
    do { v = int(rng, lo, hi); } while (v === 0);
    return v;
}
function bool(rng) { return rng() < 0.5; }

// Curated "safe" float pool: values whose typical +,-,*,/ combos are very
// unlikely to land on a whole number (mitigates the A4 residual at the
// source), backstopped by the post-hoc output filter below regardless.
const SAFE_FLOATS = [0.5, 1.5, 2.5, 0.25, 1.25, 3.75, 2.75, 0.1, 1.1, 2.3, -0.5, -1.5, -2.25];
function floatLit(rng) { return pick(rng, SAFE_FLOATS); }

const WORDS = ["foo", "bar", "baz", "qux", "hello", "world", "spam", "eggs"];
function strLit(rng) { return JSON.stringify(pick(rng, WORDS)); }

// --- method_table.rs-verified method pools ------------------------------
// Cross-referenced against crates/pyths_codegen_js/src/method_table.rs —
// only methods listed there (Rename/Inline/Hybrid/Runtime, never
// Strategy::Unsupported) appear here.
// "title" re-enabled: #87 fixed pyStrTitle to lowercase the rest of each
// word (CPython word-boundary semantics), so it now composes correctly
// with the other case-changing methods.
const STR_METHODS_NOARG = ["upper", "lower", "strip", "capitalize", "swapcase", "title"];
const LIST_METHODS_READONLY = ["copy"]; // non-mutating, safe to chain freely

// --- int/bool/str/None leaf ----------------------------------------------
function intLeaf(rng) { return String(int(rng, -12, 12)); }

// --- Arithmetic/comparison/boolean expression tree (int-typed) ---------
// Builds a random nested expression WITHOUT inserted parens, over int
// leaves, to stress the parser's operator-precedence handling the same way
// CPython's grammar handles it (see docs/language-reference.md "Operators").
function genArithExpr(rng, depth, varsInScope) {
    if (depth <= 0 || rng() < 0.35) {
        if (varsInScope.length && rng() < 0.4) return pick(rng, varsInScope);
        return intLeaf(rng);
    }
    const kind = pick(rng, ["arith", "arith", "cmp", "bool", "unary", "chained_cmp"]);
    if (kind === "arith") {
        const op = pick(rng, ["+", "-", "*"]); // no bare / or // here — kept
        // deterministic and out of the float-message residual's way;
        // division is exercised separately (see genDivExpr).
        const l = genArithExpr(rng, depth - 1, varsInScope);
        const r = genArithExpr(rng, depth - 1, varsInScope);
        return `${l} ${op} ${r}`;
    }
    if (kind === "cmp") {
        const op = pick(rng, ["<", "<=", ">", ">=", "==", "!="]);
        const l = genArithExpr(rng, depth - 1, varsInScope);
        const r = genArithExpr(rng, depth - 1, varsInScope);
        return `${l} ${op} ${r}`;
    }
    if (kind === "chained_cmp") {
        const a = genArithExpr(rng, depth - 1, varsInScope);
        const b = genArithExpr(rng, depth - 1, varsInScope);
        const c = genArithExpr(rng, depth - 1, varsInScope);
        const op1 = pick(rng, ["<", "<="]);
        const op2 = pick(rng, ["<", "<="]);
        return `${a} ${op1} ${b} ${op2} ${c}`;
    }
    if (kind === "bool") {
        const op = pick(rng, ["and", "or"]);
        const l = genArithExpr(rng, depth - 1, varsInScope);
        const r = genArithExpr(rng, depth - 1, varsInScope);
        return `${l} ${op} ${r}`;
    }
    // unary
    const sub = genArithExpr(rng, depth - 1, varsInScope);
    return pick(rng, [`-${sub}`, `not ${sub}`]);
}

// --- Post-hoc known-limitations filter -----------------------------------
// If CPython's own output for a candidate looks like it would trip the A4
// whole-float-display residual, discard the candidate (regenerate) rather
// than assert on a case the compiler is known not to handle. Deterministic:
// driven only by CPython's own output for a given (seeded) candidate.
function looksLikeWholeFloatResidual(pyOut) {
    return /-?\d+\.0\b/.test(pyOut);
}

// --- Program templates ----------------------------------------------------
// Each returns { _setup, expr } (same schema as cpython_corpus.json).
const TEMPLATES = [
    // T1: nested arithmetic/comparison/boolean/unary/chained-comparison tree.
    function arithBoolTree(rng) {
        const a = int(rng, -10, 10), b = nonzeroInt(rng, -10, 10), c = int(rng, -10, 10);
        const setup = `a = ${a}\nb = ${b}\nc = ${c}`;
        const expr = genArithExpr(rng, int(rng, 2, 3), ["a", "b", "c"]);
        return { setup, expr };
    },
    // T2: ternary expression, possibly chained.
    function ternary(rng) {
        const a = int(rng, -10, 10), b = int(rng, -10, 10);
        const cond1 = genArithExpr(rng, 1, ["a", "b"]);
        const cond2 = genArithExpr(rng, 1, ["a", "b"]);
        const chained = bool(rng);
        const expr = chained
            ? `(a if ${cond1} else (b if ${cond2} else a + b))`
            : `(a if ${cond1} else b)`;
        return { setup: `a = ${a}\nb = ${b}`, expr };
    },
    // T3: lambda IIFE.
    function lambdaIife(rng) {
        const n = int(rng, -8, 8);
        const body = pick(rng, [`x + ${int(rng, 1, 5)}`, `x * ${nonzeroInt(rng, 1, 4)}`, `x - ${int(rng, 1, 5)}`, `-x`]);
        return { setup: "", expr: `(lambda x: ${body})(${n})` };
    },
    // T4: list comprehension, optional condition.
    function listComprehension(rng) {
        const lo = int(rng, 0, 3), hi = int(rng, lo + 3, lo + 8);
        const withCond = bool(rng);
        const body = pick(rng, ["x * 2", "x + 1", "-x", "x * x"]);
        const expr = withCond
            ? `[${body} for x in range(${lo}, ${hi}) if x % 2 == 0]`
            : `[${body} for x in range(${lo}, ${hi})]`;
        return { setup: "", expr };
    },
    // T5: dict/set comprehension. The string-key restriction (the old #83
    // sidestep: non-string keys silently stringified in the plain-object
    // dict backing) is LIFTED — dict comprehensions with non-string keys
    // now compile to Map-backed PyDicts with CPython key semantics, so
    // int-keyed comprehensions repr-compare directly. `d.items()` pairs
    // are tuple-marked since #83 too, so `sorted(d.items())` also
    // repr-compares directly.
    function dictSetComprehension(rng) {
        const lo = int(rng, 0, 2), hi = int(rng, lo + 3, lo + 6);
        const shape = int(rng, 0, 3);
        if (shape === 0) {
            // int-keyed dict comprehension, repr'd directly (#83)
            const key = pick(rng, ["x", "x + 1", "x * 2"]);
            return { setup: "", expr: `{${key}: x * x for x in range(${lo}, ${hi})}` };
        }
        if (shape === 1) {
            // string-keyed comprehension via f-string (plain-object shape)
            return {
                setup: "",
                expr: `sorted([[k, v] for k, v in {f"k{x}": x * x for x in range(${lo}, ${hi})}.items()])`,
            };
        }
        if (shape === 2) {
            // int-keyed items() round-trip: pairs repr as (k, v) since #83
            return { setup: "", expr: `sorted({x: x * 3 for x in range(${lo}, ${hi})}.items())` };
        }
        return { setup: "", expr: `sorted({x % 3 for x in range(${lo}, ${hi})})` };
    },
    // T6: slicing/indexing including negative indices.
    function slicingIndexing(rng) {
        const xs = Array.from({ length: int(rng, 5, 8) }, () => int(rng, -9, 9));
        const setup = `xs = [${xs.join(", ")}]`;
        const idx = pick(rng, [
            `xs[${int(rng, 0, xs.length - 1)}]`,
            `xs[${-int(rng, 1, xs.length)}]`,
            `xs[${int(rng, 0, 2)}:${int(rng, 3, xs.length)}]`,
            `xs[::-1]`,
            `xs[:${int(rng, 1, xs.length - 1)}]`,
            `xs[${-int(rng, 2, xs.length)}:]`,
        ]);
        return { setup, expr: idx };
    },
    // T7: f-strings (including negative-number format specs). `.2f` on
    // raw `a / b` division re-enabled: #86 replaced `toFixed` with an
    // exact round-half-to-even formatter, so tie boundaries (13/8 =
    // 1.625) now match CPython.
    function fstrings(rng) {
        const a = int(rng, -20, 20);
        const b = nonzeroInt(rng, 1, 9);
        const f = floatLit(rng);
        const choice = pick(rng, [
            `f"a={a} b={b} sum={a + b}"`,
            `f"nested={f'{a}-{b}'}"`,
            `f"{a:+d}"`,
            `f"{${f}:.2f}"`,
            `f"{a / b:.2f}"`,
            `f"{a / b:.3f}"`,
        ]);
        return { setup: `a = ${a}\nb = ${b}`, expr: choice };
    },
    // T8: str methods (method_table.rs-verified).
    function strMethods(rng) {
        const w = pick(rng, WORDS);
        const m1 = pick(rng, STR_METHODS_NOARG);
        const chain2 = bool(rng);
        let expr = `${JSON.stringify(w)}.${m1}()`;
        if (chain2) {
            const m2 = pick(rng, STR_METHODS_NOARG);
            expr = `${expr}.${m2}()`;
        }
        const withReplace = bool(rng);
        if (withReplace) {
            expr = `${JSON.stringify(w)}.replace(${JSON.stringify(w[0])}, "Z")`;
        }
        return { setup: "", expr };
    },
    // T9: list methods via setup mutation (mirrors corpus style).
    function listMethods(rng) {
        const xs = Array.from({ length: int(rng, 3, 5) }, () => int(rng, -9, 9));
        const op = pick(rng, ["append", "extend", "insert", "reverse", "sort", "pop", "remove", "copy"]);
        let setup = `xs = [${xs.join(", ")}]`;
        let expr = "xs";
        if (op === "append") { setup += `\nxs.append(${int(rng, -9, 9)})`; }
        else if (op === "extend") { setup += `\nxs.extend([${int(rng, -9, 9)}, ${int(rng, -9, 9)}])`; }
        else if (op === "insert") { setup += `\nxs.insert(${int(rng, 0, xs.length)}, ${int(rng, -9, 9)})`; }
        else if (op === "reverse") { setup += `\nxs.reverse()`; }
        else if (op === "sort") { setup += `\nxs.sort()`; }
        else if (op === "pop") { setup += `\nv = xs.pop()`; expr = "[v, xs]"; }
        else if (op === "remove" && xs.length) { setup += `\nxs.remove(${xs[0]})`; }
        else if (op === "copy") { setup += `\nys = xs.copy()\nys.append(999)`; expr = "[xs, ys]"; }
        return { setup, expr };
    },
    // T10: dict methods. `.items()` pairs are consumed via a `[k, v]`
    // nested comprehension rather than repr'd raw — see the note on
    // dictSetComprehension above (array-vs-tuple representation, not a bug).
    function dictMethods(rng) {
        const keys = ["a", "b", "c"].slice(0, int(rng, 2, 3));
        const entries = keys.map((k) => `"${k}": ${int(rng, -9, 9)}`).join(", ");
        const setup = `d = {${entries}}`;
        const op = pick(rng, ["get_present", "get_missing", "keys", "values", "items", "pop", "setdefault"]);
        const itemsExpr = `sorted([[k, v] for k, v in d.items()])`;
        let expr = "d";
        let extra = "";
        if (op === "get_present") expr = `d.get("${keys[0]}", -1)`;
        else if (op === "get_missing") expr = `d.get("zzz", -1)`;
        else if (op === "keys") expr = `sorted(d.keys())`;
        else if (op === "values") expr = `sorted(d.values())`;
        else if (op === "items") expr = itemsExpr;
        else if (op === "pop") { extra = `\nv = d.pop("${keys[0]}")`; expr = `[v, ${itemsExpr}]`; }
        else if (op === "setdefault") { extra = `\nd.setdefault("zzz", 42)`; expr = itemsExpr; }
        return { setup: setup + extra, expr };
    },
    // T11: for/while loop with break/continue, accumulating into a list.
    function loopBreakContinue(rng) {
        const n = int(rng, 6, 10);
        const threshold = int(rng, 2, n - 2);
        const useWhile = bool(rng);
        let setup;
        if (useWhile) {
            setup = [
                "result = []",
                "i = 0",
                `while i < ${n}:`,
                "    if i % 2 == 0:",
                "        i += 1",
                "        continue",
                `    if i > ${threshold + 3}:`,
                "        break",
                "    result.append(i)",
                "    i += 1",
            ].join("\n");
        } else {
            setup = [
                "result = []",
                `for i in range(${n}):`,
                "    if i % 2 == 0:",
                "        continue",
                `    if i > ${threshold + 3}:`,
                "        break",
                "    result.append(i)",
            ].join("\n");
        }
        return { setup, expr: "result" };
    },
    // T12: try/except of a builtin error type. The ValueError trigger
    // (`int("abc")`) is re-enabled — #82 gave int() real string
    // validation with a CPython-shaped ValueError.
    function tryExcept(rng) {
        const kind = pick(rng, ["index", "key", "zerodiv", "value"]);
        let setup;
        if (kind === "index") {
            setup = [
                "outcome = None",
                "try:",
                "    xs = [1, 2, 3]",
                `    xs[${int(rng, 10, 20)}]`,
                "except IndexError as e:",
                '    outcome = "caught-index"',
            ].join("\n");
        } else if (kind === "key") {
            setup = [
                "outcome = None",
                "try:",
                '    d = {"a": 1}',
                '    d["missing-key"]',
                "except KeyError as e:",
                '    outcome = "caught-key"',
            ].join("\n");
        } else if (kind === "value") {
            setup = [
                "outcome = None",
                "try:",
                '    v = int("not-a-number")',
                "except ValueError as e:",
                "    outcome = str(e)",
            ].join("\n");
        } else {
            setup = [
                "outcome = None",
                "try:",
                `    v = ${int(rng, 1, 9)} / 0`,
                "except ZeroDivisionError as e:",
                '    outcome = "caught-zerodiv"',
            ].join("\n");
        }
        return { setup, expr: "outcome" };
    },
    // T13: multiple assignment / unpacking — flat first-time, the swap
    // idiom (re-enabled: #84 emits let-predeclared destructuring
    // assignment), and nested unpack targets (re-enabled: #85 emits real
    // nested destructuring patterns).
    function multiAssignUnpack(rng) {
        const a = int(rng, -9, 9), b = int(rng, -9, 9), c = int(rng, -9, 9);
        const kind = pick(rng, ["flat", "swap", "nested"]);
        if (kind === "swap") {
            const setup = `x, y = ${a}, ${b}\nx, y = y, x`;
            return { setup, expr: "[x, y]" };
        }
        if (kind === "nested") {
            const setup = `x, (y, z) = ${a}, (${b}, ${c})`;
            return { setup, expr: "[x, y, z]" };
        }
        const setup = `x, y, z = ${a}, ${b}, ${c}`;
        return { setup, expr: "[x, y, z]" };
    },
];

// --- Main -------------------------------------------------------------
async function main() {
    const { seed, n } = parseArgs(process.argv.slice(2));

    if (!checkPythonAvailable()) {
        console.log("[S2] python not on PATH — skipping fuzzer");
        process.exit(0);
    }

    console.log(`[S2] grammar-based differential fuzzer: seed=${seed} n=${n}`);
    console.log(
        `[S2] wider local sweep: node tests/differential/fuzz_gen.mjs --n 5000 --seed 12345`
    );

    const rng = mulberry32(seed);
    let pass = 0, fail = 0, skippedResidual = 0;
    const failures = [];

    let caseIndex = 0;
    let guard = 0; // safety valve against pathological infinite regeneration
    while (caseIndex < n && guard < n * 20) {
        guard++;
        const templateIdx = int(rng, 0, TEMPLATES.length - 1);
        const template = TEMPLATES[templateIdx];
        const built = template(rng);
        const setup = built.setup || "";
        const expr = built.expr;

        // Get CPython's output first (cheap) so we can apply the post-hoc
        // known-limitations filter before ever invoking the compiler.
        let pyOut;
        try {
            pyOut = runPython(setup, expr);
        } catch (e) {
            // A generated program that CPython itself rejects (e.g. an
            // unreachable combination) — regenerate rather than count it.
            continue;
        }
        if (looksLikeWholeFloatResidual(pyOut)) {
            skippedResidual++;
            continue;
        }

        const id = `s2_seed${seed}_case${caseIndex}`;
        const result = runCase({ id, _setup: setup, expr });
        if (result.pass) {
            pass++;
        } else {
            fail++;
            failures.push({
                id, caseIndex, seed, template: template.name, setup, expr, ...result,
            });
        }
        caseIndex++;
    }

    console.log(
        `[S2] fuzzer: ${pass} pass / ${pass + fail} total ` +
        `(${skippedResidual} regenerated past known-limitations filter, seed=${seed}, n=${n})`
    );
    for (const f of failures) {
        console.log(`\n[S2] FAILURE — reproduce with: node tests/differential/fuzz_gen.mjs --seed ${f.seed}`);
        console.log(`  case index: ${f.caseIndex}  template: ${f.template}`);
        console.log("  --- generated program ---");
        if (f.setup) console.log(f.setup);
        console.log(`print(repr(${f.expr}))`);
        console.log("  --- output ---");
        if (f.why) console.log(`  error: ${f.why}`);
        else {
            console.log(`  cpython:    ${JSON.stringify(f.py)}`);
            console.log(`  pythscribe: ${JSON.stringify(f.ps)}`);
        }
    }
    process.exit(fail === 0 ? 0 : 1);
}

main();
