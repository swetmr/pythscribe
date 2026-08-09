// `npm run verify:psc` — every `.psc` under shared/ must round-trip
// byte-identically to its canonical `.ps` sibling via `pyths expand
// --verify`. Mirrors frontend/scripts/verify-psc.mjs in the reference-app
// repo (READ-ONLY reference for this workspace's patterns).
import { globSync } from 'node:fs';
import { spawnSync } from 'node:child_process';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

const PYTHS = process.env.PYTHS_BIN ?? 'pyths';
const __dirname = typeof import.meta.dirname !== 'undefined'
  ? import.meta.dirname
  : path.dirname(fileURLToPath(import.meta.url));
const root = path.resolve(__dirname, '..'); // examples/clones/

const files = globSync('shared/**/*.psc', { cwd: root });
if (files.length === 0) {
  console.error('no .psc files found under shared/');
  process.exit(1);
}

let fail = 0;
for (const f of files) {
  const r = spawnSync(PYTHS, ['expand', '--verify', f], { cwd: root, encoding: 'utf8' });
  if (r.status !== 0) {
    fail++;
    console.error(`FAIL ${f}\n${r.stderr || r.stdout}`);
  }
}
console.log(`${files.length - fail}/${files.length} .psc round-trip OK`);
process.exit(fail ? 1 : 0);
