// wasm_net: the DIRECT-WASM arm. Instantiates the compiled `.wasm` itself (no glue, no
// JS twin), marshals arguments in the compiler's own list layout, calls the export, and
// reads the outcome straight from the module: the return value, the `__err_code`
// global (a Python-level raise), the `__ovf` flag (the documented "re-run on the exact
// JS twin" signal) or a trap. NOTHING is masked: the glue's twin re-run would return
// the CPython answer after a trap (dual-track masking), which is exactly how the #485
// short-circuit trap stayed invisible on the glue path.
//
//   node run_wasm_direct.mjs <module.wasm> <spec.json>
//
// stdout: JSON { name: [outcome...] }; outcomes as in run_module.mjs plus
//   { kind: "trap", exc: "<engine message>" } and { kind: "ovf" }.
//
// Layout (mirrors `__list_to_wasm` / `__list_from_wasm` in the generated glue and
// pythscribe/ffi/list_buffer.mjs): ptr -> [len:i32][cap:i32][elements...], i64 for
// list[int], f64 for list[float], i32 for list[bool]; scalar int crosses as i64
// (BigInt), float as f64, bool as i32. Calls run under the per-call watchdog.
import { readFileSync } from "node:fs";
import { workerData } from "node:worker_threads";
import { isMainThread, orchestrate, serve } from "./watchdog.mjs";

// The host functions a kernel may import: the codegen's `math.*` list
// (crates/pyths_codegen_wasm/src/bridge.rs::math_import_js). `fabs` is `Math.abs`.
const HOST_MATH = {
  pow: Math.pow, sqrt: Math.sqrt, sin: Math.sin, cos: Math.cos, tan: Math.tan, asin: Math.asin,
  acos: Math.acos, atan: Math.atan, atan2: Math.atan2, log: Math.log, log2: Math.log2,
  log10: Math.log10, exp: Math.exp, ceil: Math.ceil, floor: Math.floor, fabs: Math.abs,
};
// crates/pyths_hir/src/wasm_analysis.rs::exception_code
const EXC = { 1: "ValueError", 2: "TypeError", 3: "IndexError", 4: "KeyError", 5: "ZeroDivisionError", 6: "AssertionError", 7: "RuntimeError" };

function bitsOf(n) {
  if (Number.isNaN(n)) return "nan";
  const b = Buffer.alloc(8);
  b.writeDoubleLE(n);
  return b.toString("hex");
}
function elemType(t) { return t.slice("list[".length, -1); }
function esize(t) { return t === "bool" ? 4 : 8; }

// ---- M2a-4b: 1-D numeric typed-array ABI (mirrors the emitted glue __array_to_wasm /
// __array_write_back and pythscribe/runtime/array_buffer.py: 16-byte x8 header
// [dtype:i32@0][ndim:i32@4][shape0:i32@8][pad:i32@12], elements @ptr+16, 8-aligned ptr).
// A param typed `Array[dtype]` is marshalled here; a scalar `int`/`float` param rides the
// existing scalar path unchanged. WASM_NET_ARRAY_MUTANT injects the paired false-worlds
// (the net's own negative controls): "no-marshalling" skips the element copy-in;
// "wrong-width" lays int32 elements at an 8-byte stride (a wrong-width misread).
const ARR = {
  int32:   { ctor: Int32Array,    esize: 4, tag: 0 },
  int64:   { ctor: BigInt64Array, esize: 8, tag: 1 },
  float32: { ctor: Float32Array,  esize: 4, tag: 2 },
  float64: { ctor: Float64Array,  esize: 8, tag: 3 },
  uint8:   { ctor: Uint8Array,    esize: 1, tag: 4 },
};
const ARRAY_MUTANT = process.env.WASM_NET_ARRAY_MUTANT || "";
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
function buildTyped(values, dtype) {
  const d = ARR[dtype];
  if (dtype === "int64") return d.ctor.from(values.map((v) => BigInt(v)));
  if (dtype === "float32" || dtype === "float64") return d.ctor.from(values.map(numFromToken));
  return d.ctor.from(values.map((v) => Number(v))); // int32 / uint8
}

function scalarIn(v, ty) {
  if (ty === "int") return BigInt(typeof v === "string" ? v : Math.trunc(v));
  if (ty === "float") {
    if (v === "inf") return Infinity;
    if (v === "-inf") return -Infinity;
    if (v === "nan") return NaN;
    if (v === "-0.0") return -0;
    return Number(v);
  }
  if (ty === "bool") return v ? 1 : 0;
  throw new Error(`unknown scalar type ${ty}`);
}

function normRet(raw, ty) {
  if (ty === "int") return (typeof raw === "bigint" ? raw : BigInt(raw)).toString();
  if (ty === "float") return bitsOf(Number(raw));
  // An i32 in a bool slot that is not 0/1 is TAGGED like the other arms (opus r2/NEW-4):
  // a wrapped/raw int in a list[bool] element must not be normalized to "True".
  if (ty === "bool") { const n = Number(raw); return n === 0 ? "False" : n === 1 ? "True" : "int:" + n; }
  if (ty === "None") return "None";
  throw new Error(`unknown return type ${ty}`);
}

if (isMainThread) {
  const [, , wasmPath, specPath] = process.argv;
  const spec = JSON.parse(readFileSync(specPath, "utf8"));
  const results = await orchestrate(spec, import.meta.url, { wasmPath, specPath });
  process.stdout.write(JSON.stringify(results));
} else {
  const spec = JSON.parse(readFileSync(workerData.specPath, "utf8"));
  const module = new WebAssembly.Module(readFileSync(workerData.wasmPath));
  const imports = {};
  for (const imp of WebAssembly.Module.imports(module)) {
    if (imp.module === "math" && imp.kind === "function" && Object.hasOwn(HOST_MATH, imp.name)) {
      (imports.math ??= {})[imp.name] = HOST_MATH[imp.name];
    } else {
      (imports[imp.module] ??= {})[imp.name] = () => { throw new Error(`unsupported import ${imp.module}.${imp.name}`); };
    }
  }
  const exportsList = WebAssembly.Module.exports(module).map((e) => e.name);
  const w = new WebAssembly.Instance(module, imports).exports;

  function listToWasm(arr, ety) {
    const n = arr.length;
    const es = esize(ety);
    const ptr = w.__alloc(8 + n * es);
    const view = new DataView(w.memory.buffer);
    view.setInt32(ptr, n, true);
    view.setInt32(ptr + 4, n, true);
    for (let i = 0; i < n; i++) {
      const off = ptr + 8 + i * es;
      const v = scalarIn(arr[i], ety);
      if (ety === "float") view.setFloat64(off, v, true);
      else if (ety === "int") view.setBigInt64(off, v, true);
      else view.setInt32(off, v, true);
    }
    return ptr;
  }

  function listFromWasm(ptr, ety) {
    const view = new DataView(w.memory.buffer);
    const n = view.getInt32(ptr, true);
    const es = esize(ety);
    const out = new Array(n);
    for (let i = 0; i < n; i++) {
      const off = ptr + 8 + i * es;
      if (ety === "float") out[i] = bitsOf(view.getFloat64(off, true));
      else if (ety === "int") out[i] = view.getBigInt64(off, true).toString();
      else { const n = view.getInt32(off, true); out[i] = n === 0 ? "False" : n === 1 ? "True" : "int:" + n; }
    }
    return out;
  }

  // Marshal a 1-D flat array OR a 2-D nested (array-of-rows) array into the
  // shipped x8 header (dtype@0, ndim@4, shape0/rows@8, shape1/cols@12, elems@16,
  // row-major). `values` is a flat list for ndim=1, a list-of-rows for ndim=2.
  function arrayToWasm(values, dtype, ndim) {
    const d = ARR[dtype];
    let nrows, ncols, flat;
    if (ndim === 2) {
      nrows = values.length;
      ncols = nrows > 0 ? values[0].length : 0;
      flat = [];
      for (const row of values) for (const v of row) flat.push(v);
    } else {
      nrows = values.length; ncols = 0; flat = values;
    }
    const n = ndim === 2 ? nrows * ncols : nrows;
    // EXACTLY the emitted glue's alloc + 8-alignment (k.glue.js __array_to_wasm).
    const allocSize = ((16 + n * d.esize + 7) & ~7) + 8;
    const raw = w.__alloc(allocSize);
    const ptr = (raw + 7) & ~7;
    const view = new DataView(w.memory.buffer);
    view.setInt32(ptr, d.tag, true);
    view.setInt32(ptr + 4, ndim, true);
    view.setInt32(ptr + 8, nrows, true);   // shape0 (rows)
    view.setInt32(ptr + 12, ncols, true);  // shape1 (cols; 0 for 1-D)
    if (ARRAY_MUTANT === "no-marshalling") {
      // false-world: leave the element region as the allocator handed it (skip the copy-in).
      return ptr;
    }
    if (ARRAY_MUTANT === "transpose" && ndim === 2 && ncols !== nrows) {
      // false-world (2-D offset): lay the element region in COLUMN-major order
      // while the header claims row-major (rows,cols). A correct row-major kernel
      // then reads the TRANSPOSED elements -> divergence from NumPy on a
      // non-square array (the `i*ncols+j` vs `i*nrows+j` class). The Lean twin is
      // `arrayTranspose_offsetStub_fails`.
      const t = new Array(n);
      for (let i = 0; i < nrows; i++) for (let j = 0; j < ncols; j++) t[j * nrows + i] = flat[i * ncols + j];
      new d.ctor(w.memory.buffer, ptr + 16, n).set(buildTyped(t, dtype));
      return ptr;
    }
    const arr = buildTyped(flat, dtype);
    if (ARRAY_MUTANT === "wrong-width" && dtype === "int32") {
      // false-world: lay int32 values at an 8-byte stride; the kernel reads 4-byte ints
      // -> a wrong-width misread (garbled values / wrong element count).
      new BigInt64Array(w.memory.buffer, ptr + 16, n).set(flat.map((v) => BigInt(Math.trunc(Number(v)))));
      return ptr;
    }
    new d.ctor(w.memory.buffer, ptr + 16, n).set(arr); // the single bulk copy in
    return ptr;
  }

  // Read back the element region as a FLAT row-major list (1-D or 2-D). The
  // oracle compares flattened row-major, so 2-D readback is flat too.
  function arrayFromWasm(ptr, dtype) {
    const d = ARR[dtype];
    const view = new DataView(w.memory.buffer);
    const tag = view.getInt32(ptr, true);
    const nrows = view.getInt32(ptr + 8, true);
    const ncols = view.getInt32(ptr + 12, true);
    const ndim = view.getInt32(ptr + 4, true);
    const n = ndim === 2 ? nrows * ncols : nrows;
    if (tag !== d.tag) throw new Error(`array read-back dtype tag ${tag} != expected ${d.tag} (${dtype})`);
    const region = new d.ctor(w.memory.buffer, ptr + 16, n);
    // ints (incl. uint8) -> decimal strings (exact); floats -> little-endian IEEE-754 hex
    // ("nan" for any NaN), the SAME lossless transport the scalar net uses. Raw JS numbers
    // would lose -0.0 and turn Infinity/NaN into JSON `null` (review B1/S1), masking a
    // sign-of-zero or finite<->non-finite miscompile.
    if (dtype === "int64") return Array.from(region, (x) => x.toString());
    if (dtype === "int32" || dtype === "uint8") return Array.from(region, (x) => String(x));
    return Array.from(region, (x) => bitsOf(x)); // float32 / float64 -> f64 IEEE bits
  }

  serve(spec, (name, i) => {
    const f = spec.functions[name];
    if (!exportsList.includes(name)) return { kind: "not-exported" };
    const args = f.calls[i];
    const sp = w.__heap_ptr ? w.__heap_ptr.value : null;
    if (w.__err_code) w.__err_code.value = 0;
    if (w.__ovf) w.__ovf.value = 0;
    const ptrs = {};
    const arrPtrs = {};
    try {
      // `alias: [[i, j], ...]` = argument j IS argument i (the same Python object): pass
      // the same pointer, as the raw export would see for `f(xs, xs)`.
      const alias = new Map((f.alias || []).map(([i, j]) => { if (!(i < j)) throw new Error(`alias (src, dst) needs src < dst: ${i},${j}`); return [j, i]; }));
      const wargs = args.map((a, k) => {
        const ty = f.params[k];
        if (ty.startsWith("Array[")) {
          const p = arrayToWasm(a, arrDtype(ty), arrNdim(ty)); arrPtrs[k] = p; return p;
        }
        if (ty.startsWith("list[")) {
          if (alias.has(k)) { const p = ptrs[alias.get(k)]; ptrs[k] = p; return p; }
          const p = listToWasm(a, elemType(ty)); ptrs[k] = p; return p;
        }
        return scalarIn(a, ty);
      });
      let raw;
      try {
        raw = w[name](...wargs);
      } catch (e) {
        return { kind: "trap", exc: String(e && e.message ? e.message : e).slice(0, 120) };
      }
      if (w.__ovf && w.__ovf.value) return { kind: "ovf" };
      if (w.__err_code && w.__err_code.value !== 0) {
        const code = w.__err_code.value;
        return { kind: "err", exc: EXC[code] ?? (code >= 100 ? "UserException" + code : "Exception" + code) };
      }
      const lists = {};
      for (const [k, p] of Object.entries(ptrs)) lists[k] = listFromWasm(p, elemType(f.params[k]));
      const arrays = {};
      for (const [k, p] of Object.entries(arrPtrs)) arrays[k] = arrayFromWasm(p, arrDtype(f.params[k]));
      return { kind: "ok", value: normRet(raw, f.ret), lists, arrays };
    } finally {
      if (sp !== null) w.__heap_ptr.value = sp;
      if (w.__err_code) w.__err_code.value = 0;
      if (w.__ovf) w.__ovf.value = 0;
    }
  });
}
