// wasm_net: the JS and GLUE arms. Imports a compiled ES module (either the plain
// `--target js` output, or the `--target js+wasm` entry whose wrappers call the .wasm
// and re-run the exact JS twin on overflow/trap) and drives every call of a spec.
//
//   node run_module.mjs <entry.js> <spec.json>
//
// spec = { functions: { name: { params: [type...], ret: type, calls: [[args...]...] } } }
// stdout: JSON { name: [outcome...] }.  An outcome is
//   { kind: "ok", value, lists } | { kind: "err", exc } | { kind: "hang" } | { kind: "not-exported" }
// with values normalized EXACTLY like the CPython driver (int -> decimal string,
// float -> little-endian IEEE-754 hex, bool -> "True"/"False") so the Python side can
// diff strings. `lists` = the list ARGUMENTS after the call (write-back witness).
// Calls run under the per-call watchdog (watchdog.mjs): a non-returning call is
// recorded as `hang`, never a stuck harness.
import { pathToFileURL } from "node:url";
import { readFileSync } from "node:fs";
import { workerData } from "node:worker_threads";
import { isMainThread, orchestrate, serve } from "./watchdog.mjs";

function bitsOf(n) {
  if (Number.isNaN(n)) return "nan";
  const b = Buffer.alloc(8);
  b.writeDoubleLE(n);
  return b.toString("hex");
}

// Normalize one scalar by its DECLARED Python type (the same rule the CPython driver
// applies), so `True` under `-> int` is "1" on both sides and a float that leaked into
// an int slot is visibly tagged rather than silently truncated.
function normScalar(v, ty) {
  // Under `-> None` a returned value is a divergence, tagged by KIND like the CPython arm.
  if (ty === "None") {
    if (v === undefined || v === null) return "None";
    const kind = Array.isArray(v) ? "list" : typeof v === "boolean" ? "bool" : typeof v === "bigint" ? "int"
      : typeof v === "number" ? (Number.isInteger(v) ? "int" : "float") : "other";
    return "nonnull:" + kind;
  }
  // A non-scalar result (a list where an int was declared) is tagged like the CPython arm.
  if (Array.isArray(v)) return "other:list";
  if (v !== null && typeof v === "object" && !("valueOf" in v && typeof v.valueOf() === "number")) return "other:" + (v.constructor ? v.constructor.name : typeof v);
  if (ty === "int") {
    if (typeof v === "bigint") return v.toString();
    if (typeof v === "boolean") return v ? "1" : "0";
    const n = Number(v);
    if (Number.isInteger(n)) return BigInt(n).toString();
    return "float:" + bitsOf(n);
  }
  if (ty === "float") return bitsOf(Number(v));
  if (ty === "bool") {
    // Only a real boolean is "True"/"False"; a number under `-> bool` is TAGGED like
    // the CPython arm tags an int (the boundary repr must be visible, opus r1/SF4).
    if (typeof v === "boolean") return v ? "True" : "False";
    if (typeof v === "bigint") return "int:" + v.toString();
    const n = Number(v);
    return Number.isInteger(n) && typeof v === "number" ? "int:" + BigInt(n).toString() : "float:" + bitsOf(n);
  }
  throw new Error(`unknown declared type ${ty}`);
}

function elemType(listTy) {
  return listTy.slice("list[".length, -1);
}

// ---- M2a-4b: the glue arm's typed-array marshalling. A param typed `Array[dtype]` is
// passed to the emitted glue as a real TypedArray (the glue's __array_to_wasm validates
// its constructor/width and marshals it); after the call the out-buffers are read back
// (the glue's __array_write_back has mutated them in place). Scalar/list params are
// unchanged.
const ARR_CTOR = {
  int32: Int32Array, int64: BigInt64Array, float32: Float32Array,
  float64: Float64Array, uint8: Uint8Array,
};
function arrDtype(t) { return t.slice("Array[".length, -1).split(",")[0].trim(); }
function arrNdim(t) {
  const parts = t.slice("Array[".length, -1).split(",");
  return parts.length > 1 ? Number(parts[1].trim()) : 1;
}
function numFromToken(v) {
  if (v === "inf") return Infinity;
  if (v === "-inf") return -Infinity;
  if (v === "nan") return NaN;
  if (v === "-0.0") return -0;
  return Number(v);
}
function buildTypedArray(values, dtype) {
  const Ctor = ARR_CTOR[dtype];
  if (dtype === "int64") return Ctor.from(values.map((v) => BigInt(v)));
  if (dtype === "float32" || dtype === "float64") return Ctor.from(values.map(numFromToken));
  return Ctor.from(values.map((v) => Number(v)));
}
function readTypedArray(ta, dtype) {
  if (dtype === "int64") return Array.from(ta, (x) => x.toString());
  if (dtype === "int32" || dtype === "uint8") return Array.from(ta, (x) => String(x));
  // floats -> little-endian IEEE-754 hex ("nan" for NaN): lossless across the JSON hop
  // (raw numbers lose -0.0 and map inf/nan to `null` — review B1/S1). bitsOf is defined above.
  return Array.from(ta, (x) => bitsOf(x)); // float32 / float64 -> f64 IEEE bits
}
// M2b: build the 2-D glue input — an Array of same-dtype TypedArray rows (what
// __array_to_wasm(arr, dtype, 2) expects). `nested` is a list-of-rows.
function build2dRows(nested, dtype) {
  return nested.map((row) => buildTypedArray(row, dtype));
}
// Read a 2-D glue out-param (Array of TypedArray rows) back as a FLAT row-major
// list (the oracle compares flattened row-major).
function read2dFlat(rows, dtype) {
  const out = [];
  for (const r of rows) for (const v of readTypedArray(r, dtype)) out.push(v);
  return out;
}

function argOf(v, ty) {
  if (ty === "int") return typeof v === "string" ? BigInt(v) : v; // big ints travel as strings
  if (ty === "float") {
    if (v === "inf") return Infinity;
    if (v === "-inf") return -Infinity;
    if (v === "nan") return NaN;
    if (v === "-0.0") return -0;
    return v;
  }
  if (ty === "bool") return !!v;
  if (ty.startsWith("list[")) return v.map((e) => argOf(e, elemType(ty)));
  throw new Error(`unknown param type ${ty}`);
}

// Pass safe ints as plain numbers (what a real caller does), only big ones as BigInt.
function jsInt(b) {
  if (typeof b !== "bigint") return b;
  return b >= -9007199254740991n && b <= 9007199254740991n ? Number(b) : b;
}

function normArg(v, ty) {
  const a = argOf(v, ty);
  if (ty === "int") return jsInt(a);
  if (ty.startsWith("list[") && elemType(ty) === "int") return a.map(jsInt);
  return a;
}

function excName(e) {
  if (e && typeof e.name === "string" && /^[A-Z][A-Za-z]*(Error|Exception|Exit|Interrupt)$/.test(e.name)) return e.name;
  const m = String(e && e.message !== undefined ? e.message : e).match(/([A-Z][A-Za-z]*(?:Error|Exception))/);
  if (m) return m[1];
  return "JSError:" + String(e).slice(0, 80);
}

if (isMainThread) {
  const [, , entry, specPath] = process.argv;
  const spec = JSON.parse(readFileSync(specPath, "utf8"));
  const results = await orchestrate(spec, import.meta.url, { entry, specPath });
  process.stdout.write(JSON.stringify(results));
} else {
  const spec = JSON.parse(readFileSync(workerData.specPath, "utf8"));
  const mod = await import(pathToFileURL(workerData.entry).href);
  serve(spec, (name, i) => {
    const f = spec.functions[name];
    const fn = mod[name];
    if (typeof fn !== "function") return { kind: "not-exported" };
    const hasArray = f.params.some((ty) => ty.startsWith("Array["));
    if (hasArray) {
      // The typed-array glue arm: build real TypedArrays, call, read the out-buffers back.
      // A wrong-dtype/ndim buffer (the negative-shape rows) makes the glue throw a
      // RangeError -> reroute to the JS twin; whatever it does, it is NEVER a silent
      // wrong-width misread, which array_net asserts.
      const args = f.calls[i].map((a, k) => {
        const ty = f.params[k];
        if (ty.startsWith("Array[")) {
          // A negative-shape probe overrides the buffer dtype: {"__buf__": "float32",
          // "v": [...]} builds a WRONG-dtype buffer for this Array[...] param, so the
          // glue's total check must refuse it (RangeError -> twin reroute), never a
          // silent wrong-width read.
          if (a && !Array.isArray(a) && typeof a === "object" && a.__buf__) {
            return buildTypedArray(a.v, a.__buf__);
          }
          // M2b: a 2-D array crosses as an Array of same-dtype TypedArray rows;
          // 1-D as a flat TypedArray.
          return arrNdim(ty) === 2 ? build2dRows(a, arrDtype(ty)) : buildTypedArray(a, arrDtype(ty));
        }
        return normArg(a, ty);
      });
      try {
        const r = fn(...args);
        const arrays = {};
        f.params.forEach((ty, k) => {
          if (ty.startsWith("Array[")) {
            arrays[k] = arrNdim(ty) === 2
              ? read2dFlat(args[k], arrDtype(ty))
              : readTypedArray(args[k], arrDtype(ty));
          }
        });
        return { kind: "ok", value: normScalar(r, f.ret), arrays };
      } catch (e) {
        return { kind: "err", exc: excName(e) };
      }
    }
    const jsArgs = f.calls[i].map((a, k) => normArg(a, f.params[k]));
    for (const [src, dst] of f.alias || []) jsArgs[dst] = jsArgs[src]; // the same array object
    try {
      const r = fn(...jsArgs);
      const lists = {};
      f.params.forEach((ty, k) => {
        if (ty.startsWith("list[")) lists[k] = jsArgs[k].map((e) => normScalar(e, elemType(ty)));
      });
      return { kind: "ok", value: normScalar(r, f.ret), lists };
    } catch (e) {
      return { kind: "err", exc: excName(e) };
    }
  });
}
