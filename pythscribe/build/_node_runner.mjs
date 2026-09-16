// Runs a built artifact under Node for the differential oracle (compiled vs CPython).
//   node _node_runner.mjs <entry.js> <fn>   (stdin: JSON array of argument arrays)
// stdout: JSON array of {ok, value, bits, jstype, ctor} | {ok:false, error}
// `bits` is the little-endian IEEE-754 hex of Number(value): the byte-for-byte witness.
import { pathToFileURL } from "node:url";

const [, , entry, fn] = process.argv;
if (!entry || !fn) {
  console.error("usage: node _node_runner.mjs <entry.js> <fn> < calls.json");
  process.exit(2);
}
const chunks = [];
for await (const c of process.stdin) chunks.push(c);
const calls = JSON.parse(Buffer.concat(chunks).toString("utf8") || "[]");

const mod = await import(pathToFileURL(entry).href);
const f = mod[fn];
if (typeof f !== "function") {
  console.error(`artifact ${entry} does not export a function named ${fn}`);
  process.exit(3);
}

function describe(v) {
  const out = { ok: true, jstype: typeof v, ctor: v && v.constructor ? v.constructor.name : null };
  if (typeof v === "bigint") {
    out.value = v.toString();
    out.bits = null;
    return out;
  }
  const n = Number(v);
  out.value = n;
  const b = Buffer.alloc(8);
  b.writeDoubleLE(n);
  out.bits = b.toString("hex");
  return out;
}

const results = calls.map((args) => {
  try {
    return describe(f(...args));
  } catch (e) {
    return { ok: false, error: String(e && e.stack ? e.stack.split("\n")[0] : e) };
  }
});
process.stdout.write(JSON.stringify(results));
