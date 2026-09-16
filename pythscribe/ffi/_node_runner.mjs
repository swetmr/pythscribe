// Runs a `@wasm` kernel through the list-buffer FFI shim under Node -- the compiled arm of
// the M1 differential oracle (the SAME shim the Gradio component uses in the tab).
//   node _node_runner.mjs <kernel.wasm>     (stdin: JSON request)
// request: {fn, param_types, return_type, calls: [{args: [...], read_back: [i,...]}]}
//   a list arg may be given inline (JSON array), as {"i32_file": path} (little-endian int32),
//   {"f64_file": path}, or {"zeros": n} (a zero-filled `out` buffer of n elements); an int
//   beyond 2**53 is passed as {"bigint": "<decimal string>"} (the STRING form only -- a JSON
//   number would already be rounded). A read-back is returned inline (exact: BigInts as
//   {"bigint"}, non-finite floats as {"nonfinite"}) or, when the call names `out_files:
//   {index: path}`, written to that path as the element type ACTUALLY read back and labelled
//   accordingly: little-endian int32 -> {"i32_file": path} (a list[int] whose elements all fit
//   int32), little-endian float64 -> {"f64_file": path} (a list[float]); an i64 read-back with
//   elements beyond int32 and an out_file requested comes back as {"inline": true, "values":
//   [...]} which the Python wrapper turns into an explicit error.
// stdout: JSON array of {ok, value, outs: {index: array | {"i32_file": path} | {"f64_file": path} | {"inline": true, ...}}, heap_bytes}
//         | {ok: false, error}
import { readFile, writeFile } from "node:fs/promises";
import { instantiate, call } from "./list_buffer.mjs";

const [, , wasmPath] = process.argv;
if (!wasmPath) {
  console.error("usage: node _node_runner.mjs <kernel.wasm> < request.json");
  process.exit(2);
}
const chunks = [];
for await (const c of process.stdin) chunks.push(c);
const req = JSON.parse(Buffer.concat(chunks).toString("utf8"));
const kernel = await instantiate(wasmPath);

async function loadArg(a, t) {
  if (a && typeof a === "object" && !Array.isArray(a)) {
    if (a.i32_file) { const b = await readFile(a.i32_file); return new Int32Array(b.buffer.slice(b.byteOffset, b.byteOffset + b.byteLength)); }
    if (a.f64_file) { const b = await readFile(a.f64_file); return new Float64Array(b.buffer.slice(b.byteOffset, b.byteOffset + b.byteLength)); }
    if (a.bigint !== undefined) return bigintDescriptor(a);
    if (a.zeros !== undefined) return t === "list[float]" ? new Float64Array(a.zeros) : new Int32Array(a.zeros); // an `out` buffer
    throw new Error(`unrecognised argument object for type ${t}`);
  }
  if (t === "list[float]") return Float64Array.from(a);
  if (Array.isArray(a)) return a.map((x) => (x && typeof x === "object" && x.bigint !== undefined ? bigintDescriptor(x) : x));
  return a;
}

/** {"bigint": "<decimal>"} -- the STRING form only: a JSON number in the descriptor has already
 * been rounded by the parser, so accepting it would make the exact escape hatch lossy (codex r3). */
function bigintDescriptor(x) {
  if (typeof x.bigint !== "string" || !/^-?\d+$/.test(x.bigint)) throw new TypeError(`bigint descriptor must be a decimal string, got ${JSON.stringify(x.bigint)}`);
  return BigInt(x.bigint);
}

// exact scalars out: BigInts past 2**53 as {bigint}, non-finite floats as {nonfinite} -- JSON
// would otherwise turn NaN/Infinity into null, indistinguishable from a None return (opus r1/B3)
const jsonInt = (x) => (typeof x === "bigint" ? { bigint: x.toString() } : (typeof x === "number" && !Number.isFinite(x) ? { nonfinite: String(x) } : x));

const results = [];
for (const c of req.calls) {
  try {
    const args = [];
    for (let i = 0; i < c.args.length; i++) args.push(await loadArg(c.args[i], req.param_types[i]));
    if (c.poison_ovf && kernel.exports.__ovf) kernel.exports.__ovf.value = 1; // test hook: a stale overflow flag from an earlier trap
    const r = call(kernel, req.fn, req.param_types, args, { readBack: c.read_back ?? [], returnType: req.return_type ?? "int" });
    const outs = {};
    for (const [i, arr] of Object.entries(r.outs)) {
      const of = c.out_files && c.out_files[i];
      // the file label names the element type ACTUALLY written (opus r3/NB-3): i32 for an int32
      // read-back, f64 for a list[float]; anything else is reported inline, never mislabelled
      if (of && arr instanceof Int32Array) { await writeFile(of, Buffer.from(arr.buffer, arr.byteOffset, arr.byteLength)); outs[i] = { i32_file: of }; }
      else if (of && arr instanceof Float64Array) { await writeFile(of, Buffer.from(arr.buffer, arr.byteOffset, arr.byteLength)); outs[i] = { f64_file: of }; }
      else if (of) outs[i] = { inline: true, values: Array.from(arr, jsonInt), note: "out_file not written: read-back has elements beyond int32, returned inline exactly" };
      else outs[i] = Array.from(arr, jsonInt); // exact: BigInts (beyond 2**53) as {bigint}
    }
    results.push({ ok: true, value: jsonInt(r.value), outs, heap_bytes: r.heapBytes, exports: kernel.exportNames, how: kernel.how });
  } catch (e) {
    results.push({ ok: false, error: `${e && e.name ? e.name + ": " : ""}${e && e.message ? e.message : String(e)}` });
  }
}
process.stdout.write(JSON.stringify(results));
