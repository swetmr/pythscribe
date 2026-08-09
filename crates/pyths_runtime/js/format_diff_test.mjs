// Differential test: compare pyFormatSpec output against CPython's
// format() over an enumerated set of (value, spec) combinations.
//
// Runs Python via `python -c` for each case; skips the suite if Python
// isn't on PATH. Test stdout is JSON-friendly so CI can grep failures.
//
// Run:  node crates/pyths_runtime/js/format_diff_test.mjs

import { execFileSync } from "node:child_process";
import { test } from "node:test";
import assert from "node:assert/strict";
import { pyFormatSpec, parseFormatSpec } from "./runtime.js";

// Lightweight Python availability probe — skip the suite cleanly if
// python isn't installed (CI matrices that don't include it).
let pythonOk = true;
try {
    execFileSync("python", ["-c", "print(format(3.14, '.2f'))"], { stdio: "pipe" });
} catch {
    pythonOk = false;
}

// PEP 3101 mini-parser mirror of `format_spec.rs`. We pass `(value, spec)`
// to JS, separately formatJSON them in Python, compare textual outputs.
//
// Cases enumerated by spec shape, paired with values that exercise the
// affected branches. Keep numbers exact in JS (avoid 0.1+0.2 surprises).
const CASES = [
    // Float fixed precision
    { v: 3.14159, s: ".2f" },
    { v: 3.14159, s: ".4f" },
    { v: 3.7, s: ".0f" },
    { v: 1234.5, s: ".1f" },
    { v: 0, s: ".3f" },
    { v: -2.5, s: ".2f" },

    // Integer
    { v: 1234567, s: "," },
    { v: 1234567, s: ",d" },
    { v: 7, s: "02d" },
    { v: 12, s: "04d" },
    { v: 1234, s: "08d" },

    // Hex / binary / octal
    { v: 255, s: "x" },
    { v: 255, s: "X" },
    { v: 255, s: "#x" },
    { v: 8, s: "b" },
    { v: 8, s: "#b" },
    { v: 8, s: "o" },

    // Scientific
    { v: 1234.5, s: ".2e" },
    { v: 0.00012345, s: ".3e" },
    { v: 1000000, s: ".2e" },

    // Percent
    { v: 0.0532, s: ".2%" },
    { v: 1, s: ".0%" },

    // String width + align
    { v: "hi", s: ">10s" },
    { v: "hi", s: "<10s" },
    { v: "hi", s: "^10s" },
    { v: "hi", s: "*^10s" },

    // Sign
    { v: 5, s: "+d" },
    { v: -5, s: "+d" },
    { v: 5, s: " d" },

    // Width-only number with grouping
    { v: 1234567.89, s: ",.2f" },

    // #86: round-half-to-even on the EXACT double value (JS toFixed
    // rounds exact ties away from zero; CPython rounds them to even).
    { v: 1.625, s: ".2f" },   // exact tie → 1.62 (even); toFixed says 1.63
    { v: 2.675, s: ".2f" },   // NOT a tie (2.67499...) → 2.67
    { v: 0.125, s: ".2f" },   // exact tie → 0.12
    { v: 0.375, s: ".2f" },   // exact tie → 0.38 (up to even)
    { v: 2.5, s: ".0f" },     // integer tie → 2
    { v: 1.5, s: ".0f" },     // integer tie → 2
    { v: -1.625, s: ".2f" },  // negative tie → -1.62
    { v: 0.125, s: ".1%" },   // % branch: 12.5% tie → 12.5→ exact-driven

    // Pythonic-checks sweep: `_` grouping (ints group by 3; b/o/x/X by 4)
    { v: 1234567, s: "_" },
    { v: 1234567, s: "_d" },
    { v: 1048575, s: "_x" },
    { v: 1234, s: "_b" },
    { v: 255, s: "#_b" },
    { v: 1234.5678, s: "_.2f" },
    { v: -1234567, s: "_" },

    // Pythonic-checks sweep: `,` on a float with no precision must keep
    // the full fractional part (toLocaleString truncates to 3 digits)
    { v: 1234.5678, s: "," },

    // Pythonic-checks sweep: zero-pad must be sign-aware (-0042, not 00-42)
    { v: -42, s: "05" },
    { v: -42, s: "05d" },
    { v: -3.14, s: "08.2f" },
    { v: 42, s: "05" },

    // Pythonic-checks sweep: `g` — CPython switchover/stripping/exponent
    { v: 12345.678, s: "g" },
    { v: 0.000012345, s: "g" },   // exp < -4 → scientific, 2-digit exp
    { v: 12345.678, s: ".3g" },   // 1.23e+04
    { v: 100.0, s: "g" },         // strip to '100'
    { v: 0.0001, s: "g" },        // fixed (exp == -4)
    { v: 1e21, s: "g" },          // 1e+21
    { v: 123456789, s: "g" },     // 1.23457e+08
    { v: 0, s: "g" },
    { v: 1500000, s: ".2g" },     // 1.5e+06
    { v: 0.5, s: ".2G" },
    { v: 12345.678, s: ".3G" },
];

function pyFormat(value, spec) {
    // Value embedded as JSON (Python-compatible for ints/floats/strings).
    const expr = `format(${JSON.stringify(value)}, ${JSON.stringify(spec)})`;
    const out = execFileSync("python", ["-c", `import sys; sys.stdout.write(${expr})`], { stdio: ["ignore", "pipe", "pipe"] });
    return out.toString();
}

function jsFormat(value, specStr) {
    // #108: parseFormatSpec is the shipped runtime parser (was a
    // test-only mirror, parseSpecForTest, before pyFormatDynamic existed).
    const opts = parseFormatSpec(specStr);
    return pyFormatSpec(value, opts);
}


if (!pythonOk) {
    test("format-spec differential vs CPython (skipped — python not in PATH)", () => {
        // No-op; CI that has python will run the real suite.
    });
} else {
    for (const c of CASES) {
        test(`format(${JSON.stringify(c.v)}, ${JSON.stringify(c.s)})`, () => {
            const py = pyFormat(c.v, c.s);
            const js = jsFormat(c.v, c.s);
            assert.equal(js, py, `spec ${c.s} for ${c.v}: js=${JSON.stringify(js)} py=${JSON.stringify(py)}`);
        });
    }
}
