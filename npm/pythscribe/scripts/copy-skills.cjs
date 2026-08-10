#!/usr/bin/env node
"use strict";
// prepack: copy the canonical authoring skills from the repo root into ./skills/
// so the published package ships them for LLM/agent use. Single source of truth is
// the repo-root SKILL*.md; the generated ./skills/ dir is gitignored.
const fs = require("node:fs");
const path = require("node:path");

const root = path.resolve(__dirname, "..", "..", ".."); // <pkg>/scripts -> repo root
const outDir = path.join(__dirname, "..", "skills");
const map = [
  ["SKILL.md", "pythscribe-language.md"],
  ["SKILL.psc.md", "compressing-ps-to-psc.md"],
];

fs.mkdirSync(outDir, { recursive: true });
for (const [src, dst] of map) {
  const from = path.join(root, src);
  if (!fs.existsSync(from)) {
    console.error(`copy-skills: missing source ${from}`);
    process.exit(1);
  }
  fs.copyFileSync(from, path.join(outDir, dst));
  console.log(`copy-skills: skills/${dst}`);
}
