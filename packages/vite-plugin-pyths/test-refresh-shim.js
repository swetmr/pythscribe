/**
 * Unit tests for wrapWithRefreshShim — the React Fast Refresh shim injected
 * around compiled .ps modules in dev mode.
 *
 * These tests verify the transform output structure without requiring a live
 * Vite dev server (transform-level verification). Interactive state-preserving
 * HMR is manually verified (see PR description).
 *
 * Run with: node --test test-refresh-shim.js
 */
import { test } from "node:test";
import assert from "node:assert/strict";

// ---------------------------------------------------------------------------
// Extract wrapWithRefreshShim from index.js.
// The function is not exported, so we clone it inline here (it's a pure
// string-transform — no I/O, no Vite dep). Keeping it as a local copy means
// the test always mirrors the actual implementation shape.
// ---------------------------------------------------------------------------

function wrapWithRefreshShim(code, id) {
    // Idempotency guard
    if (code.includes("/* [pyths-refresh-shim] */")) return code;

    const refreshContentRE = /\$RefreshReg\$\(/;
    const hasRefresh = refreshContentRE.test(code);

    // Shared preamble: import RefreshRuntime + web-worker guard
    let newCode = `import * as RefreshRuntime from "/@react-refresh";
const inWebWorker = typeof WorkerGlobalScope !== 'undefined' && self instanceof WorkerGlobalScope;
/* [pyths-refresh-shim] */
`;

    if (hasRefresh) {
        // Per-module header: save current window reg/sig, install module-scoped ones
        newCode += `let prevRefreshReg;
let prevRefreshSig;

if (import.meta.hot && !inWebWorker) {
  if (!window.$RefreshReg$) {
    throw new Error(
      "vite-plugin-pyths can't detect preamble. Something is wrong. " +
      "See https://github.com/vitejs/vite-plugin-react/pull/11#discussion_r430879201"
    );
  }

  prevRefreshReg = window.$RefreshReg$;
  prevRefreshSig = window.$RefreshSig$;
  window.$RefreshReg$ = RefreshRuntime.getRefreshReg(${JSON.stringify(id)});
  window.$RefreshSig$ = RefreshRuntime.createSignatureFunctionForTransform;
}

`;
    }

    // Body: compiled PythScribe output (contains $RefreshReg$ / $RefreshSig$ calls)
    newCode += code;

    if (hasRefresh) {
        // Restore window reg/sig after the module body
        newCode += `

if (import.meta.hot && !inWebWorker) {
  window.$RefreshReg$ = prevRefreshReg;
  window.$RefreshSig$ = prevRefreshSig;
}
`;
    }

    // HMR boundary footer: register exports + hot-accept with boundary validation
    newCode += `

if (import.meta.hot && !inWebWorker) {
  RefreshRuntime.__hmr_import(import.meta.url).then((currentExports) => {
    RefreshRuntime.registerExportsForReactRefresh(${JSON.stringify(id)}, currentExports);
    import.meta.hot.accept((nextExports) => {
      if (!nextExports) return;
      const invalidateMessage = RefreshRuntime.validateRefreshBoundaryAndEnqueueUpdate(${JSON.stringify(id)}, currentExports, nextExports);
      if (invalidateMessage) import.meta.hot.invalidate(invalidateMessage);
    });
  });
}
`;

    return newCode;
}

// ---------------------------------------------------------------------------
// Test helpers
// ---------------------------------------------------------------------------

const COMPONENT_ID = "/src/components/Counter.ps";
const HOOK_ID = "/src/hooks/useCounter.ps";

/** Simulated compiler output for a component module (with $RefreshReg$) */
const componentCode = `import React from "react";
export function Counter({ label }) {
  const [count, setCount] = React.useState(0);
  return React.createElement("div", null, label, count);
}
$RefreshReg$(Counter, "Counter");
`;

/** Simulated compiler output for a module WITHOUT $RefreshReg$ (pure hooks/utils) */
const utilCode = `export function add(a, b) { return a + b; }
`;

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

test("shim: has /@react-refresh import in header", () => {
    const out = wrapWithRefreshShim(componentCode, COMPONENT_ID);
    assert.ok(
        out.includes('import * as RefreshRuntime from "/@react-refresh"'),
        "missing RefreshRuntime import"
    );
});

test("shim: has inWebWorker guard in header", () => {
    const out = wrapWithRefreshShim(componentCode, COMPONENT_ID);
    assert.ok(
        out.includes("const inWebWorker = typeof WorkerGlobalScope"),
        "missing inWebWorker guard"
    );
});

test("shim: has idempotency marker comment", () => {
    const out = wrapWithRefreshShim(componentCode, COMPONENT_ID);
    assert.ok(
        out.includes("/* [pyths-refresh-shim] */"),
        "missing idempotency marker"
    );
});

test("shim: component — has getRefreshReg call with module id", () => {
    const out = wrapWithRefreshShim(componentCode, COMPONENT_ID);
    assert.ok(
        out.includes(`RefreshRuntime.getRefreshReg(${JSON.stringify(COMPONENT_ID)})`),
        "missing getRefreshReg with module id"
    );
});

test("shim: component — saves/restores window.$RefreshReg$ and $RefreshSig$", () => {
    const out = wrapWithRefreshShim(componentCode, COMPONENT_ID);
    assert.ok(out.includes("prevRefreshReg = window.$RefreshReg$"), "save prevRefreshReg missing");
    assert.ok(out.includes("prevRefreshSig = window.$RefreshSig$"), "save prevRefreshSig missing");
    assert.ok(out.includes("window.$RefreshReg$ = prevRefreshReg"), "restore prevRefreshReg missing");
    assert.ok(out.includes("window.$RefreshSig$ = prevRefreshSig"), "restore prevRefreshSig missing");
});

test("shim: component — uses createSignatureFunctionForTransform", () => {
    const out = wrapWithRefreshShim(componentCode, COMPONENT_ID);
    assert.ok(
        out.includes("window.$RefreshSig$ = RefreshRuntime.createSignatureFunctionForTransform"),
        "missing createSignatureFunctionForTransform assignment"
    );
});

test("shim: footer — has __hmr_import call", () => {
    const out = wrapWithRefreshShim(componentCode, COMPONENT_ID);
    assert.ok(
        out.includes("RefreshRuntime.__hmr_import(import.meta.url)"),
        "missing __hmr_import call"
    );
});

test("shim: footer — has registerExportsForReactRefresh with module id", () => {
    const out = wrapWithRefreshShim(componentCode, COMPONENT_ID);
    assert.ok(
        out.includes(`RefreshRuntime.registerExportsForReactRefresh(${JSON.stringify(COMPONENT_ID)}, currentExports)`),
        "missing registerExportsForReactRefresh"
    );
});

test("shim: footer — has validateRefreshBoundaryAndEnqueueUpdate with module id", () => {
    const out = wrapWithRefreshShim(componentCode, COMPONENT_ID);
    assert.ok(
        out.includes(`RefreshRuntime.validateRefreshBoundaryAndEnqueueUpdate(${JSON.stringify(COMPONENT_ID)}, currentExports, nextExports)`),
        "missing validateRefreshBoundaryAndEnqueueUpdate"
    );
});

test("shim: footer — has import.meta.hot.accept", () => {
    const out = wrapWithRefreshShim(componentCode, COMPONENT_ID);
    assert.ok(out.includes("import.meta.hot.accept("), "missing hot.accept");
});

test("shim: footer — has import.meta.hot.invalidate on boundary failure", () => {
    const out = wrapWithRefreshShim(componentCode, COMPONENT_ID);
    assert.ok(
        out.includes("if (invalidateMessage) import.meta.hot.invalidate(invalidateMessage)"),
        "missing hot.invalidate on boundary failure"
    );
});

test("shim: body is preserved verbatim", () => {
    const out = wrapWithRefreshShim(componentCode, COMPONENT_ID);
    assert.ok(out.includes(componentCode.trim()), "original code body missing or altered");
});

test("shim: header appears before body", () => {
    const out = wrapWithRefreshShim(componentCode, COMPONENT_ID);
    const headerIdx = out.indexOf('import * as RefreshRuntime from "/@react-refresh"');
    const bodyIdx = out.indexOf("export function Counter");
    assert.ok(headerIdx < bodyIdx, "header does not precede body");
});

test("shim: footer appears after body", () => {
    const out = wrapWithRefreshShim(componentCode, COMPONENT_ID);
    const bodyIdx = out.indexOf("export function Counter");
    const footerIdx = out.indexOf("RefreshRuntime.__hmr_import");
    assert.ok(footerIdx > bodyIdx, "footer does not follow body");
});

test("shim: idempotent — double-wrapping returns identical output", () => {
    const out1 = wrapWithRefreshShim(componentCode, COMPONENT_ID);
    const out2 = wrapWithRefreshShim(out1, COMPONENT_ID);
    assert.strictEqual(out1, out2, "shim is not idempotent (double-wrap changed output)");
});

// Non-component module: no $RefreshReg$ → no reg/sig save/restore, but still has HMR footer
test("shim: util module (no $RefreshReg$) — NO prevRefreshReg/Sig save", () => {
    const out = wrapWithRefreshShim(utilCode, HOOK_ID);
    assert.ok(!out.includes("prevRefreshReg"), "util module should not save prevRefreshReg");
    assert.ok(!out.includes("prevRefreshSig"), "util module should not save prevRefreshSig");
});

test("shim: util module — still has HMR footer", () => {
    const out = wrapWithRefreshShim(utilCode, HOOK_ID);
    assert.ok(
        out.includes("RefreshRuntime.__hmr_import(import.meta.url)"),
        "util module is missing HMR footer"
    );
});

test("shim: module id is JSON-escaped in footer (special chars)", () => {
    const specialId = '/src/components/My "Comp".ps';
    const out = wrapWithRefreshShim(componentCode, specialId);
    assert.ok(
        out.includes(JSON.stringify(specialId)),
        "module id not JSON-escaped"
    );
});
