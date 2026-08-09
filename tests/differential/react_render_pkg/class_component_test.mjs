// A3 regression: class components / error boundaries actually render with
// REAL react + react-dom (not just JS-string assertions on codegen output).
//
// `class Boundary(Component)` compiles fine, but the risky part is runtime:
// does the cooperative `__pyClass` MRO wrap (built for pure-PythScribe class
// hierarchies, B-026) survive being asked to wrap a NATIVE React base class?
// It did not — see docs/language-reference.md "Class components & error
// boundaries" for the finding and the fix (a native/external first base now
// skips the cooperative model and keeps a plain JS `constructor`, matching
// the existing exception-subclass path).
//
// This test lives in its own npm package (react_render_pkg/) with real
// `react` + `react-dom` + `jsdom` devDependencies — `npm install` here
// first (see CI job "Node + browser" in .github/workflows/ci.yml).
//
// Run:  cd tests/differential/react_render_pkg && npm install && node --test class_component_test.mjs

import { spawnSync } from "node:child_process";
import { readFileSync } from "node:fs";
import path from "node:path";
import { fileURLToPath, pathToFileURL } from "node:url";
import { test } from "node:test";
import assert from "node:assert/strict";
import React from "react";
import { renderToStaticMarkup } from "react-dom/server";
import { createRoot } from "react-dom/client";
import { JSDOM } from "jsdom";

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const REPO_ROOT = path.resolve(__dirname, "..", "..", "..");
const PYTHS_BIN = path.join(REPO_ROOT, "target", "release",
    process.platform === "win32" ? "pyths.exe" : "pyths");
const FIXTURE_PS = path.join(__dirname, "..", "class_component_demo.ps");
const OUT_MJS = path.join(__dirname, "class_component_demo.mjs");

// Compile fresh, into this package, so the compiled module's bare `"react"`
// / `"pyths-runtime"` imports resolve against this package's node_modules.
const compile = spawnSync(PYTHS_BIN, [
    "compile", FIXTURE_PS, "-o", OUT_MJS, "--quiet",
], { encoding: "utf8", env: { ...process.env, PYTHS_NO_CACHE: "1" } });
if (compile.status !== 0) {
    console.error(compile.stderr || compile.stdout);
    process.exit(1);
}

const compiledSource = readFileSync(OUT_MJS, "utf8");

test("compiled output: external-base classes use a native constructor, not the cooperative MRO wrap", () => {
    // Codegen-level guard for the fix, colocated with the runtime proof below
    // so a regression here fails loudly next to the behavior it protects.
    assert.match(compiledSource, /class Hello extends Component/);
    assert.match(compiledSource, /class Counter extends Component \{\s*constructor\(props\)/);
    assert.match(compiledSource, /class Boundary extends Component \{\s*constructor\(props\)/);
    assert.doesNotMatch(compiledSource, /__pyClass\(/,
        "no cooperative-MRO wrap for classes whose first base is external/native");
});

const { Hello, Counter, Boundary, Boom } = await import(pathToFileURL(OUT_MJS));

test("(a) plain class component renders via render()", () => {
    const html = renderToStaticMarkup(React.createElement(Hello, { name: "world" }));
    assert.equal(html, "<h1>hi world</h1>");
});

test("(b) constructor-set self.state reaches render() (setState wiring intact)", () => {
    // Direct construction proves __init__ actually ran (this.state must not
    // be undefined/null — the exact way this was broken before the fix).
    const inst = new Counter({});
    assert.deepEqual(inst.state, { count: 0 });

    const html = renderToStaticMarkup(React.createElement(Counter, {}));
    assert.equal(html, "<div>count=0</div>");
});

test("(c) error boundary: static getDerivedStateFromError + componentDidCatch catch a throwing child", async () => {
    const dom = new JSDOM("<!doctype html><div id='root'></div>");
    globalThis.window = dom.window;
    globalThis.document = dom.window.document;
    Object.defineProperty(globalThis, "navigator", { value: dom.window.navigator, configurable: true });
    globalThis.IS_REACT_ACT_ENVIRONMENT = true;

    const container = document.getElementById("root");
    const root = createRoot(container);

    const origError = console.error;
    console.error = (...args) => {
        const msg = String(args[0] ?? "");
        if (msg.includes("kaboom") || msg.includes("above error occurred")) return;
        origError(...args);
    };
    try {
        await React.act(async () => {
            root.render(React.createElement(Boundary, null, React.createElement(Boom, null)));
        });
    } finally {
        console.error = origError;
    }

    assert.equal(container.innerHTML, "<p>fallback</p>");
});
