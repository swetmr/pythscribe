#!/usr/bin/env node
// M0.1 (spec 13-09-26; codex M0 SF1): verify that a PREPARED payload TARBALL -- the artifact
// `publish.mjs` actually hands to `npm publish` -- carries exactly the prepared payload:
//   * member set == the keys of `dist/<x>-payload.files.json`,
//   * every member's sha256 == its recorded sha256 (raw bytes, no normalization),
//   * the tarball's package.json is `<name>@<version>`.
// A stale / divergent .tgz beside a valid extracted dir is RED HERE, before npm sees it (the M6.2
// identity gate stays as the downstream backstop). Dependency-free: gunzip + a ustar/pax walk.
//
//   node npm/payload-check.mjs <payload.tgz> <payload.files.json> <name> <version>   # exit 0 / 1
import { createHash } from "node:crypto";
import { readFileSync } from "node:fs";
import { gunzipSync } from "node:zlib";

/** `{path: Buffer}` of every regular file in an npm tarball (`package/` prefix stripped). */
export function tarballMembers(tgzPath) {
  const buf = gunzipSync(readFileSync(tgzPath));
  const out = {};
  let off = 0, paxPath = null;
  while (off + 512 <= buf.length) {
    const hdr = buf.subarray(off, off + 512);
    if (hdr.every((b) => b === 0)) break;                      // end-of-archive zero blocks
    const str = (a, n) => hdr.subarray(a, a + n).toString("utf8").replace(/\0.*$/s, "");
    const size = parseInt(str(124, 12).trim() || "0", 8);
    const type = String.fromCharCode(hdr[156] || 48);
    let name = str(0, 100);
    const prefix = str(345, 155);
    if (prefix && str(257, 6) === "ustar") name = `${prefix}/${name}`;
    const data = buf.subarray(off + 512, off + 512 + size);
    off += 512 + Math.ceil(size / 512) * 512;
    if (type === "x") {                                        // pax extended header: `<len> path=<p>\n`
      for (const rec of data.toString("utf8").split("\n")) {
        const m = rec.match(/^\d+ path=(.*)$/);
        if (m) paxPath = m[1];
      }
      continue;
    }
    if (paxPath) { name = paxPath; paxPath = null; }
    if (type === "g" || type === "L" || type === "K") continue; // global pax / GNU long-name records (unused by npm)
    if (type !== "0" && type !== "\0") continue;               // directories etc.
    if (!name.startsWith("package/")) throw new Error(`tarball member ${name} is not under package/`);
    const rel = name.slice("package/".length);
    if (!rel || rel.split("/").some((p) => p === "" || p === "." || p === "..")) throw new Error(`refusing tarball member path ${name}`);
    out[rel] = Buffer.from(data);
  }
  return out;
}

const sha256 = (b) => createHash("sha256").update(b).digest("hex");

/** Returns a list of problems (empty == the tarball IS the prepared payload). */
export function verifyPreparedTarball(tgzPath, filesJsonPath, name, version) {
  const expected = JSON.parse(readFileSync(filesJsonPath, "utf8"));
  const members = tarballMembers(tgzPath);
  const problems = [];
  for (const rel of Object.keys(expected).sort()) if (!(rel in members)) problems.push(`MISSING from the tarball: ${rel}`);
  for (const rel of Object.keys(members).sort()) if (!(rel in expected)) problems.push(`EXTRA in the tarball (not in the prepared payload): ${rel}`);
  for (const rel of Object.keys(expected).sort()) {
    if (rel in members && sha256(members[rel]) !== expected[rel]) problems.push(`CHANGED bytes: ${rel} (tarball sha256 != prepared payload)`);
  }
  if (members["package.json"]) {
    const j = JSON.parse(members["package.json"].toString("utf8"));
    if (j.name !== name || j.version !== version) problems.push(`tarball package.json is ${j.name}@${j.version}, expected ${name}@${version}`);
  } else {
    problems.push("tarball has no package.json");
  }
  return problems;
}

if (import.meta.url === new URL(`file:///${process.argv[1].replace(/\\/g, "/")}`).href || process.argv[1]?.endsWith("payload-check.mjs")) {
  const [tgz, filesJson, name, version] = process.argv.slice(2);
  if (!tgz || !filesJson || !name || !version) {
    console.error("usage: node npm/payload-check.mjs <payload.tgz> <payload.files.json> <name> <version>");
    process.exit(2);
  }
  const problems = verifyPreparedTarball(tgz, filesJson, name, version);
  if (problems.length) {
    console.error(`payload-check: RED -- ${tgz} is not the prepared payload:\n  ${problems.join("\n  ")}`);
    process.exit(1);
  }
  console.log(`payload-check: GREEN -- ${tgz} == prepared payload (${Object.keys(JSON.parse(readFileSync(filesJson, "utf8"))).length} files, sha256-verified), ${name}@${version}`);
}
