// M2a-3b: run a compiled js+wasm array kernel under Node — the EMITTED-GLUE arm
// of the array differential (glue-path out-buffer vs NumPy).
//   node _array_node_runner.mjs <entry.js> <fn>   (stdin: JSON call spec)
// spec = { args: [ {t:"Int32Array", d:[...]} | {s: <scalar>}, ... ], readback: [i,...] }
//   a "t" arg is a TypedArray (d holds its values; BigInt64Array values are
//   decimal STRINGS to preserve precision); an "s" arg is a plain scalar (a JS
//   number, or a decimal string for a BigInt/`int` param). readback = indices of
//   the (TypedArray) args to read back after the call (the out-buffers filled in
//   place). stdout: { ok, ret, readback: [[...], ...] } | { ok:false, error }.
import { pathToFileURL } from "node:url";

const [, , entry, fn] = process.argv;
if (!entry || !fn) {
  console.error("usage: node _array_node_runner.mjs <entry.js> <fn> < spec.json");
  process.exit(2);
}
const chunks = [];
for await (const c of process.stdin) chunks.push(c);
const spec = JSON.parse(Buffer.concat(chunks).toString("utf8") || "{}");

const CTORS = { Int32Array, BigInt64Array, Float32Array, Float64Array, Uint8Array };

function build(arg) {
  if (Object.prototype.hasOwnProperty.call(arg, "s")) {
    // a scalar: decimal string -> BigInt (an `int` param), else a Number.
    return typeof arg.s === "string" ? BigInt(arg.s) : arg.s;
  }
  const Ctor = CTORS[arg.t];
  if (!Ctor) throw new Error(`unknown TypedArray ${arg.t}`);
  const data = arg.t === "BigInt64Array" ? arg.d.map((x) => BigInt(x)) : arg.d;
  return Ctor.from(data);
}

function dump(v) {
  // a TypedArray -> array (BigInt elements -> decimal strings); a bigint -> string.
  if (ArrayBuffer.isView(v)) {
    return Array.from(v, (x) => (typeof x === "bigint" ? x.toString() : x));
  }
  return typeof v === "bigint" ? v.toString() : v;
}

const mod = await import(pathToFileURL(entry).href);
const f = mod[fn];
if (typeof f !== "function") {
  console.error(`artifact ${entry} does not export a function named ${fn}`);
  process.exit(3);
}

try {
  const args = spec.args.map(build);
  const ret = f(...args);
  const readback = (spec.readback || []).map((i) => dump(args[i]));
  process.stdout.write(JSON.stringify({ ok: true, ret: dump(ret), readback }));
} catch (e) {
  process.stdout.write(
    JSON.stringify({ ok: false, error: String(e && e.stack ? e.stack.split("\n")[0] : e) })
  );
}
