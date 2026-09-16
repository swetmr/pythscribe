// M2c: run a compiled `Array[dtype, ndim]` kernel through the BROWSER shim
// (`pythscribe/ffi/list_buffer.mjs` — the exact typed-array marshaller the Gradio component
// imports) under Node. The "browser" arm of the isomorphic differential (shim-marshalled
// out-buffer vs the SERVER wasmtime path vs CPython/NumPy).
//
//   node _array_shim_runner.mjs <shim.mjs> <kernel.wasm> <fn>   (stdin: JSON call spec)
//
// The shim path is an argument so the paired RED control can point it at a MUTATED copy of the
// shim (transposed 2-D read / wrong element offset) and show the isomorphic equality breaks.
//
// spec = { paramTypes: [...], args: [arg, ...], readBack: [i,...], returnType: "int"|"None"|... }
//   arg = { s: <number|decimal-string> }                    scalar (string -> BigInt for `int`)
//       | { a1: "<dtype>", d: [...] }                        1-D TypedArray (int64 d = strings)
//       | { a2: "<dtype>", rows: [[...], ...] }              2-D: Array of TypedArray rows
// stdout: { ok, ret, readback: [dumped-arg, ...] } | { ok:false, error }
import { pathToFileURL } from "node:url";

const [, , shimPath, wasmPath, fn] = process.argv;
if (!shimPath || !wasmPath || !fn) {
  console.error("usage: node _array_shim_runner.mjs <shim.mjs> <kernel.wasm> <fn> < spec.json");
  process.exit(2);
}
const chunks = [];
for await (const c of process.stdin) chunks.push(c);
const spec = JSON.parse(Buffer.concat(chunks).toString("utf8") || "{}");

const ffi = await import(pathToFileURL(shimPath).href);

function ctorOf(dtype) {
  const d = ffi.ARR_DTYPES[dtype];
  if (!d) throw new Error(`unknown array dtype ${dtype}`);
  return d.ctor;
}

function build(arg) {
  if (Object.prototype.hasOwnProperty.call(arg, "s")) {
    return typeof arg.s === "string" ? BigInt(arg.s) : arg.s;
  }
  if (Object.prototype.hasOwnProperty.call(arg, "a1")) {
    const Ctor = ctorOf(arg.a1);
    const data = arg.a1 === "int64" ? arg.d.map((x) => BigInt(x)) : arg.d;
    return Ctor.from(data);
  }
  if (Object.prototype.hasOwnProperty.call(arg, "a2")) {
    const Ctor = ctorOf(arg.a2);
    return arg.rows.map((row) => Ctor.from(arg.a2 === "int64" ? row.map((x) => BigInt(x)) : row));
  }
  throw new Error(`bad arg ${JSON.stringify(arg)}`);
}

function dump(v) {
  if (ArrayBuffer.isView(v)) return Array.from(v, (x) => (typeof x === "bigint" ? x.toString() : x));
  if (Array.isArray(v)) return v.map(dump); // 2-D: array of rows
  return typeof v === "bigint" ? v.toString() : v;
}

try {
  const kernel = await ffi.instantiate(wasmPath);
  const args = spec.args.map(build);
  const r = ffi.call(kernel, fn, spec.paramTypes, args, {
    readBack: spec.readBack || [],
    returnType: spec.returnType ?? "int",
  });
  const readback = (spec.readBack || []).map((i) => dump(r.outs[i]));
  process.stdout.write(JSON.stringify({ ok: true, ret: dump(r.value), readback }));
} catch (e) {
  process.stdout.write(JSON.stringify({ ok: false, error: String(e && e.stack ? e.stack.split("\n")[0] : e) }));
}
