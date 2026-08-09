// Tests for rewritePsImports — the importer-aware specifier rewrite that
// makes PythScribe importers resolve PythScribe siblings (and precompiled
// .client.js islands) instead of same-named .tsx/.ts oracles.
import { test } from "node:test";
import assert from "node:assert";
import { mkdtempSync, writeFileSync, mkdirSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { rewritePsImports } from "./loader.js";

const dir = mkdtempSync(join(tmpdir(), "pyths-rewrite-"));
mkdirSync(join(dir, "components"), { recursive: true });
// siblings on disk
writeFileSync(join(dir, "components", "Counter.psc"), "");
writeFileSync(join(dir, "components", "Counter.ps"), "");
writeFileSync(join(dir, "components", "Island.client.js"), "");
writeFileSync(join(dir, "components", "Island.psc"), "");
writeFileSync(join(dir, "components", "Plain.ps"), "");
const importer = join(dir, "app", "page.psc");
mkdirSync(join(dir, "app"), { recursive: true });

const rw = (code) => rewritePsImports(code, importer);

test("prefers .psc over .ps when both exist", () => {
  assert.match(rw('import { C } from "../components/Counter";'), /Counter\.psc"/);
});
test("prefers precompiled .client.js over .psc", () => {
  assert.match(rw('import { I } from "../components/Island";'), /Island\.client\.js"/);
});
test("falls back to .ps when only .ps exists", () => {
  assert.match(rw('import { P } from "../components/Plain";'), /Plain\.ps"/);
});
test("leaves bare/package imports alone", () => {
  const s = 'import { x } from "pyths-runtime";';
  assert.equal(rw(s), s);
});
test("leaves extensioned imports alone", () => {
  const s = 'import { x } from "../components/Counter.tsx";';
  assert.equal(rw(s), s);
});
test("leaves relative imports with no PythScribe sibling alone", () => {
  const s = 'import { x } from "../components/NoSibling";';
  assert.equal(rw(s), s);
});
test("rewrites export-from too", () => {
  assert.match(rw('export { C } from "../components/Counter";'), /Counter\.psc"/);
});
