#!/usr/bin/env node
"use strict";
// prepack: copy the canonical authoring skills from the repo root into ./skills/
// so the published package can scaffold them into new apps. Single source of truth
// is the repo-root SKILL*.md; the generated ./skills/ dir is gitignored.
const fs = require("node:fs");
const path = require("node:path");

const root = path.resolve(__dirname, "..", "..", ".."); // <pkg>/scripts -> repo root
const outDir = path.join(__dirname, "..", "skills");
const map = [
  ["SKILL.md", "pythscribe-language.md"],
  ["SKILL.psc.md", "compressing-ps-to-psc.md"],
];

// S10: clear the generated skills/ dir FIRST so a stale skills/old.md from a previous prepack cannot
// survive into the packed payload (it would ship in the npm tarball AND the wheel-vendored copy, and
// -- being on both sides -- pass the raw-byte mirror compare). Recreate it clean, then copy.
fs.rmSync(outDir, { recursive: true, force: true });
fs.mkdirSync(outDir, { recursive: true });
for (const [src, dst] of map) {
  const from = path.join(root, src);
  if (!fs.existsSync(from)) {
    console.error(`copy-skills: missing source ${from}`);
    process.exit(1);
  }
  // Normalize CRLF -> LF on write so the packed payload is byte-stable across platforms:
  // the canonical SKILL*.md are LF in git but check out CRLF on a default-autocrlf Windows
  // clone, and this runs at `npm pack` time -> a raw copyFileSync would bake CRLF into the
  // wheel's vendored copy on Windows while a Linux CI leg packs LF, RED-ing the raw-byte
  // scaffolder mirror gate + the web-parity `git diff` (opus M7 review BLOCKER-1).
  const bytes = fs.readFileSync(from, "utf8").replace(/\r\n/g, "\n");
  fs.writeFileSync(path.join(outDir, dst), bytes);
  console.log(`copy-skills: skills/${dst}`);
}
