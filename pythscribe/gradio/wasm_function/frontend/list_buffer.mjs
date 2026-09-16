// pythscribe list-buffer FFI shim (v0.2.5 M1).
//
// A `@wasm` kernel may only RETURN a scalar (M0 / compiler admission #364), and the compiler's
// generated glue copies `list` parameters INTO linear memory but never back out -- so a
// kernel that fills an `out: list[int]` in place (ordinary Python semantics, exactly what the
// CPython fallback does) hands nothing to JS through the glue. This shim is the honest
// channel: it instantiates the kernel's own `.wasm`, lays list parameters out in linear
// memory in the compiler's OWN list layout, calls the WASM export directly, and reads the
// `out` lists back from memory. Nothing is computed here; the resize is the WASM export.
//
// DECLARED BINDING (plan §7 -- three representations that must agree): the layout below
// mirrors `__list_to_wasm` / `__list_from_wasm` in the generated glue
//     ptr -> [len:i32][cap:i32][elements...]   i64 for list[int], f64 for list[float]
//     ints cross as i64 (BigInt), floats as f64, bools as i32
// `tests/pythscribe/test_image_kernel.py::test_layout_binding_glue_matches_shim` pins the
// glue's marshaller text so a compiler-side layout change goes RED here instead of
// silently corrupting buffers.
//
// There is NO JS twin on this path. The glue re-runs a call on an arbitrary-precision JS
// twin when the WASM path overflows or traps; this shim REFUSES instead (throws), so the
// caller always knows which path executed (the dual-track-masking guard).
//
// Integer boundary (plan §7 watch item, review r1/B1+B2): ints cross as EXACT i64 or not at
// all -- a JS Number must be a safe integer, a BigInt must fit i64, and an i64 list is read
// back exactly (Int32Array when every element fits int32, else Numbers/BigInts). Nothing here
// rounds or wraps silently.

export const LAYOUT_VERSION = "pyths-0.2.4-list-v1";
export const HEADER_BYTES = 8;
export const ELEM_BYTES = { "list[int]": 8, "list[float]": 8, "list[bool]": 4 };
export const SCALAR_PARAMS = new Set(["int", "float", "bool"]);
export const SCALAR_RETURNS = new Set(["int", "float", "bool", "None"]);

// v0.2.5 callback path (B3): the compiler emits an exported `__err_code` global (only when a body
// uses `raise`/`assert`/`try` -- `needs_errors`) that a raising kernel SETS while returning a
// SENTINEL value. On the js+wasm glue path `bridge.rs::__check_err` reads it after every call and
// THROWS; on THIS shim path there is no glue, so `call()` checks+resets it beside `__ovf` and
// throws too (else a raising kernel returns its sentinel silently -- the silent-wrong-value class).
// `__ERROR_NAMES` mirrors the glue's map (bridge.rs L323-325): codes 1-7 are the fixed builtins;
// codes >= 100 are per-module CUSTOM exception names that live only in the glue's table (the shim
// cannot see them) -> the shim throws a generic `Exception` carrying `.code = <n>`.
export const ERROR_NAMES = {
  1: "ValueError", 2: "TypeError", 3: "IndexError", 4: "KeyError",
  5: "ZeroDivisionError", 6: "AssertionError", 7: "RuntimeError",
};

// The callback path treats a non-finite SCALAR result (NaN, +/-inf) as an ERROR STATE, never a
// rendered value (S-r2-1): every HOST_MATH import is JS `Math.*`, so `math.sqrt(-1)->NaN`,
// `math.log(0)->-inf`, `math.exp(1000)->inf` and raw `f64.div` `1.0/0.0->inf` where CPython
// raises. A `-> int` result outside +/-2**53 crosses as a BigInt (`fromI64`) -- a LEGITIMATE
// large int, NOT an error -- so `Number.isFinite(1n)` (false) must not misclassify it.
export function isRenderableScalar(r) {
  return typeof r === "bigint" || Number.isFinite(r);
}

// `wasm_url` is root-absolute (`/gradio_api/file=...`); resolve it against Gradio's configured app
// root (`window.gradio_config.root`) so a sub-path mount / proxy prefix works wherever Gradio's own
// file route does. Exported HERE (one copy, under the shim byte-identity gate) so the Gradio load
// hook (`browser.py`) and the callback island share ONE resolver -- never a second inlined copy.
// A relative root (never emitted by Gradio today) or a non-root-absolute URL falls back to the raw
// path, never a hard error.
export function resolveUrl(u) {
  const cfg = typeof window !== "undefined" ? window.gradio_config : null;
  const root = cfg && typeof cfg.root === "string" && cfg.root ? cfg.root : null;
  if (!root || !u.startsWith("/")) return u;
  try { return new URL(u.replace(/^\/+/, ""), root.endsWith("/") ? root : root + "/").href; } catch (e) { return u; }
}

// ---------------------------------------------------------------------------------------------
// v0.2.5 M2c: TYPED-ARRAY marshalling (the browser leg of the numeric `Array[dtype, ndim]` ABI).
//
// A `@wasm` kernel may take fixed-width numeric arrays (`Array[dtype]` / `Array[dtype, 2]`).
// The SAME 16-byte ×8 header + row-major element layout the compiler already emits on the
// js+wasm glue path (`crates/pyths_codegen_wasm/src/bridge.rs::emit_array_helpers`) and on the
// server (`pythscribe/runtime/array_buffer.py`) is used here, so a typed array marshalled by
// this shim in the browser is byte-identical to one the server/glue crosses:
//
//     ptr -> [dtype tag:i32 @0][ndim:i32 @4][shape0:i32 @8][shape1:i32 @12][elements @16 ...]
//
//   dtype tag: int32=0 int64=1 float32=2 float64=3 uint8=4  (== array_buffer.DTYPE_TAG /
//              bridge.rs __ARR_DTYPES; the wire codes are single-sourced, DECLARED here to match)
//   ndim=1: `arr` is a flat TypedArray; ndim=2: `arr` is an Array of `nrows` TypedArray rows,
//           all of the compiled-for dtype and equal length `ncols` (rectangular / C-contiguous).
//   element region at ptr+16 is 8-aligned (the ×8 header + an 8-aligned base), MANDATORY:
//   a JS Float64Array/BigInt64Array view over WASM memory THROWS on a non-width-aligned offset.
//
// int64 crosses as BigInt64Array (its elements are BigInt), the four others as their Number
// TypedArray. This is "buffer, not elements": one bulk `.set()` per row each way, never per-item.
//
// TOTAL runtime check (requirements B1): the JS buffer MUST be a TypedArray whose constructor
// and BYTES_PER_ELEMENT match the compiled-for dtype (and, for 2-D, a rectangular Array of such
// rows). A mismatch is REFUSED (throws) — this shim has NO JS twin (its docstring above), so it
// refuses loudly rather than silently reading at the wrong width/stride. `ARRAY_LAYOUT_VERSION`
// is the array header's own version (the list header keeps `LAYOUT_VERSION`), bumped when the
// array layout changes; it must equal `array_buffer.ARRAY_LAYOUT_VERSION`.
export const ARRAY_LAYOUT_VERSION = "pyths-0.2.5-array-v2";
export const ARRAY_HEADER_BYTES = 16;
export const ARRAY_ELEM_OFFSET = 16;
export const ARR_DTYPES = {
  int32:   { ctor: Int32Array,    esize: 4, tag: 0 },
  int64:   { ctor: BigInt64Array, esize: 8, tag: 1 },
  float32: { ctor: Float32Array,  esize: 4, tag: 2 },
  float64: { ctor: Float64Array,  esize: 8, tag: 3 },
  uint8:   { ctor: Uint8Array,    esize: 1, tag: 4 },
};

// `Array[dtype]` / `Array[dtype, 1|2]` -> {dtype, ndim}, else null (mirrors
// pythscribe.runtime.array_param_spec and the ffi grammar in pythscribe/ffi/__init__.py).
const _ARRAY_ANN = /^\s*Array\[\s*([A-Za-z0-9_]+)\s*(?:,\s*([0-9]+)\s*)?\]\s*$/;
export function arrayParamSpec(t) {
  const m = _ARRAY_ANN.exec(t || "");
  if (!m) return null;
  const dtype = m[1];
  if (!Object.prototype.hasOwnProperty.call(ARR_DTYPES, dtype)) return null;
  const ndim = m[2] === undefined ? 1 : parseInt(m[2], 10);
  if (ndim !== 1 && ndim !== 2) return null; // ndim>2 never admitted (refuse, never misread)
  return { dtype, ndim };
}

// One admitted-dtype TypedArray row check (shared by 1-D and each 2-D row): the exact
// compiled-for constructor + element width. A plain Array, a wrong-dtype TypedArray, or a
// DataView is refused. Matches bridge.rs __arrRowOk / the server check_array_buffer.
function arrRowOk(row, d) {
  return ArrayBuffer.isView(row) && row.constructor === d.ctor && row.BYTES_PER_ELEMENT === d.esize;
}

/** The TOTAL runtime check + bulk copy IN. Returns the 8-aligned `ptr`. Mirrors
 * bridge.rs::__array_to_wasm byte-for-byte. */
function arrayToWasm(ex, arr, dtype, ndim, where) {
  const d = ARR_DTYPES[dtype];
  let nrows, ncols, n;
  if (ndim === 1) {
    if (!arrRowOk(arr, d)) {
      const got = (arr && arr.constructor && arr.constructor.name) ? arr.constructor.name : typeof arr;
      throw new RangeError(`pythscribe ffi: ${where}: Array[${dtype}] expects a ${d.ctor.name} buffer (dtype/width match); got ${got} — refused (never read at the wrong width)`);
    }
    nrows = arr.length; ncols = 0; n = arr.length;
  } else {
    if (!Array.isArray(arr)) throw new RangeError(`pythscribe ffi: ${where}: Array[${dtype}, 2] expects an array of ${d.ctor.name} rows; got ${typeof arr} — refused`);
    nrows = arr.length;
    ncols = 0;
    for (let r = 0; r < nrows; r++) {
      if (!arrRowOk(arr[r], d)) throw new RangeError(`pythscribe ffi: ${where}: Array[${dtype}, 2] row ${r} must be a ${d.ctor.name} buffer (dtype/width match) — refused`);
      if (r === 0) ncols = arr[0].length;
      if (arr[r].length !== ncols) throw new RangeError(`pythscribe ffi: ${where}: Array[${dtype}, 2] is ragged (row ${r} length ${arr[r].length} != ${ncols}); only C-contiguous rectangular 2-D arrays cross — refused`);
    }
    n = nrows * ncols;
  }
  // header + elements, rounded to ×8, plus 8 slack so the base can be aligned UP (a TypedArray
  // view throws on a non-width-aligned byte offset; the ×8 header keeps ptr+16 8-aligned).
  const allocSize = ((ARRAY_HEADER_BYTES + n * d.esize + 7) & ~7) + 8;
  const raw = ex.__alloc(allocSize);
  const ptr = (raw + 7) & ~7;
  const view = new DataView(ex.memory.buffer); // AFTER __alloc (which may grow + detach memory)
  view.setInt32(ptr, d.tag, true);
  view.setInt32(ptr + 4, ndim, true);
  view.setInt32(ptr + 8, nrows, true);
  view.setInt32(ptr + 12, ncols, true);
  if (ndim === 1) {
    new d.ctor(ex.memory.buffer, ptr + ARRAY_ELEM_OFFSET, n).set(arr);
  } else {
    for (let r = 0; r < nrows; r++) new d.ctor(ex.memory.buffer, ptr + ARRAY_ELEM_OFFSET + r * ncols * d.esize, ncols).set(arr[r]);
  }
  return ptr;
}

/** Self-checking bulk copy OUT: re-parse + validate the header against the compiled-for
 * (tag, ndim, shape) before trusting the element region, then bulk-copy into the caller's
 * TypedArray(s) in place. Mirrors bridge.rs::__array_write_back / array_buffer.read_back +
 * unpack_array_into (a drifted/corrupted header is a genuine corruption -> loud throw). */
function arrayWriteBack(ex, arr, ptr, dtype, ndim, where) {
  const d = ARR_DTYPES[dtype];
  const view = new DataView(ex.memory.buffer);
  const tag = view.getInt32(ptr, true);
  const hdrNdim = view.getInt32(ptr + 4, true);
  const nrows = view.getInt32(ptr + 8, true);
  const ncols = view.getInt32(ptr + 12, true);
  if (tag !== d.tag) throw new Error(`pythscribe ffi: ${where}: array write-back dtype tag ${tag} != expected ${d.tag} (${dtype}); a wrong-width read would corrupt values — refused`);
  if (hdrNdim !== ndim) throw new Error(`pythscribe ffi: ${where}: array write-back ndim ${hdrNdim} != expected ${ndim} — refused`);
  if (ndim === 1) {
    if (nrows !== arr.length) throw new Error(`pythscribe ffi: ${where}: array out-parameter length changed (${arr.length} -> ${nrows}); a fixed-capacity buffer cannot reflect a length-changing mutation — refused`);
    arr.set(new d.ctor(ex.memory.buffer, ptr + ARRAY_ELEM_OFFSET, nrows));
  } else {
    if (nrows !== arr.length) throw new Error(`pythscribe ffi: ${where}: 2-D array out-parameter row count changed (${arr.length} -> ${nrows}) — refused`);
    for (let r = 0; r < nrows; r++) {
      if (arr[r].length !== ncols) throw new Error(`pythscribe ffi: ${where}: 2-D array out-parameter row ${r} length changed (${arr[r].length} -> ${ncols}) — refused`);
      arr[r].set(new d.ctor(ex.memory.buffer, ptr + ARRAY_ELEM_OFFSET + r * ncols * d.esize, ncols));
    }
  }
}

const SAFE_MAX = 9007199254740991n;
const SAFE_MIN = -9007199254740991n;

function isNode() {
  return typeof globalThis.process !== "undefined" && globalThis.process.versions != null
    && typeof globalThis.process.versions.node === "string";
}

/** The host functions a kernel may import: EXACTLY the codegen's `math.*` list
 * (crates/pyths_codegen_wasm/src/bridge.rs::math_import_js), name -> implementation. Never
 * `Math[name]`: `math.fabs` is `Math.abs`, and a name lookup would refuse a legal kernel
 * (opus m1.5 r2/NEW-3). The server twin is pythscribe/runtime/_jsmath.py::HOST_MATH;
 * tests/pythscribe/test_runtime.py::test_r5_host_imports_match_codegen pins all three. */
export const HOST_MATH = {
  pow: Math.pow, sqrt: Math.sqrt, sin: Math.sin, cos: Math.cos, tan: Math.tan, asin: Math.asin, acos: Math.acos,
  atan: Math.atan, atan2: Math.atan2, log: Math.log, log2: Math.log2, log10: Math.log10, exp: Math.exp,
  ceil: Math.ceil, floor: Math.floor, fabs: Math.abs,
};

// ---------------------------------------------------------------------------------------------
// v0.2.5 M2.1 (spec 13-09-26 §5.6): the WASM ABI loud-fail contract, tab side. Every module the
// pyths compiler emits carries a `pyths.abi` custom section `{abi, list_layout, array_layout,
// compiler, history}` (single source: crates/pyths_codegen_wasm/src/abi.rs) and the exported
// immutable global `__pyths_abi`. `checkAbi(module)` reads the section via
// `WebAssembly.Module.customSections` and compares the major AND BOTH layout strings -- each
// field independently, none informational -- against this shim's DECLARED MIRRORS
// (`SUPPORTED_ABI_MAJOR` here; `LAYOUT_VERSION` / `ARRAY_LAYOUT_VERSION` above, which K7 pins
// byte-identical to the server runtime). A module built by a compiler with a different layout
// is refused LOUDLY before instantiation (a thrown Error naming the field and both sides), never
// marshalled at the wrong width. The server twin is pythscribe/runtime/abi.py::check_module; the
// Rust<->Python<->JS labels are bound by tests/pythscribe/test_abi_binding.py.
export const ABI_SECTION_NAME = "pyths.abi";
export const SUPPORTED_ABI_MAJOR = 1;

export function readAbiSection(module) {
  const secs = WebAssembly.Module.customSections(module, ABI_SECTION_NAME);
  if (secs.length !== 1) {
    throw new Error(`pythscribe ffi: WASM ABI check failed: expected exactly one \`${ABI_SECTION_NAME}\` custom section, found ${secs.length} (this .wasm was not emitted by a pyths compiler this runtime can load; rebuild the artifact with \`pyths build --force\`)`);
  }
  let got;
  try { got = JSON.parse(new TextDecoder().decode(new Uint8Array(secs[0]))); }
  catch (e) { throw new Error(`pythscribe ffi: WASM ABI check failed: \`${ABI_SECTION_NAME}\` section is not valid JSON (${e.message})`); }
  if (!got || typeof got !== "object") throw new Error(`pythscribe ffi: WASM ABI check failed: \`${ABI_SECTION_NAME}\` section is not a JSON object`);
  for (const f of ["abi", "list_layout", "array_layout"]) {
    if (!(f in got)) throw new Error(`pythscribe ffi: WASM ABI check failed: \`${ABI_SECTION_NAME}\` section lacks the \`${f}\` field`);
  }
  return got;
}

function abiMismatch(field, got, want) {
  const e = new Error(`pythscribe ffi: WASM ABI mismatch on \`${field}\`: module=${JSON.stringify(got)} runtime=${JSON.stringify(want)} -- rebuild the artifact with the matching pyths compiler (a mismatched layout would be read at the wrong width; refused, never silently)`);
  e.name = "AbiMismatchError";
  e.field = field;
  return e;
}

/** Layer 1, tab side: throws unless the module's section agrees on major + both layouts.
 * THREE explicit comparisons (not a deep-equal) so each field has its own absent-comparison
 * mutant and its own patched-module control (validation §G-ABI-1 / §G-ABI-3 (v)(vi)). */
export function checkAbi(module) {
  const got = readAbiSection(module);
  if (got.abi !== SUPPORTED_ABI_MAJOR) throw abiMismatch("abi", got.abi, SUPPORTED_ABI_MAJOR);
  if (got.list_layout !== LAYOUT_VERSION) throw abiMismatch("list_layout", got.list_layout, LAYOUT_VERSION);
  if (got.array_layout !== ARRAY_LAYOUT_VERSION) throw abiMismatch("array_layout", got.array_layout, ARRAY_LAYOUT_VERSION);
  return got;
}

function buildImports(module) {
  const imports = {};
  for (const imp of WebAssembly.Module.imports(module)) {
    if (imp.module === "math" && imp.kind === "function" && Object.prototype.hasOwnProperty.call(HOST_MATH, imp.name)) {
      (imports.math ??= {})[imp.name] = HOST_MATH[imp.name];
      continue;
    }
    throw new Error(`pythscribe ffi: unsupported WASM import ${imp.module}.${imp.name} (${imp.kind})`);
  }
  return imports;
}

/** Instantiate a kernel's `.wasm`. `source`: a URL string (browser: streaming fetch; Node: file
 * path / file URL), an ArrayBuffer/Uint8Array of bytes, or a compiled WebAssembly.Module. */
export async function instantiate(source) {
  let module;
  let how;
  if (source instanceof WebAssembly.Module) {
    module = source;
    how = "module";
  } else if (source instanceof ArrayBuffer || ArrayBuffer.isView(source)) {
    module = await WebAssembly.compile(source);
    how = "bytes";
  } else if (typeof source === "string") {
    if (isNode()) {
      const { readFile } = await import("node:fs/promises");
      const { fileURLToPath } = await import("node:url");
      const p = source.startsWith("file:") ? fileURLToPath(source) : source;
      module = await WebAssembly.compile(await readFile(p));
      how = "node-file";
    } else if (typeof WebAssembly.compileStreaming === "function") {
      module = await WebAssembly.compileStreaming(fetch(source));
      how = "streaming";
    } else {
      module = await WebAssembly.compile(await fetch(source).then((r) => r.arrayBuffer()));
      how = "fetch-bytes";
    }
  } else {
    throw new TypeError("pythscribe ffi: instantiate() wants a URL/path, bytes, or a WebAssembly.Module");
  }
  const abi = checkAbi(module); // M2.1: BEFORE instantiation -- a mismatched module never runs
  const instance = await WebAssembly.instantiate(module, buildImports(module));
  const exportNames = WebAssembly.Module.exports(module).map((e) => e.name);
  return { module, instance, exports: instance.exports, exportNames, how, abi };
}

const I64_MAX = 9223372036854775807n;
const I64_MIN = -9223372036854775808n;

/** Exact i64 ingress or a loud refusal -- never a silent round/wrap (review r1/B2):
 * a Number must be a SAFE integer (|v| <= 2**53-1; anything larger has already lost
 * precision before it reached us), a BigInt must lie within signed i64. */
function toI64(v, where) {
  if (typeof v === "bigint") {
    if (v > I64_MAX || v < I64_MIN) throw new RangeError(`pythscribe ffi: ${where}: ${v}n is outside the i64 range`);
    return v;
  }
  if (typeof v === "number") {
    if (!Number.isSafeInteger(v)) throw new RangeError(`pythscribe ffi: ${where}: ${v} is not a safe integer (|v| <= 2**53-1); pass a BigInt for larger ints`);
    return BigInt(v);
  }
  if (typeof v === "boolean") return v ? 1n : 0n;
  throw new TypeError(`pythscribe ffi: ${where}: expected an int, got ${typeof v}`);
}

function fromI64(v) {
  return v >= SAFE_MIN && v <= SAFE_MAX ? Number(v) : v;
}

/** Read an i64 list back EXACTLY (review r1/B1): Int32Array when every element fits int32
 * (the image fast path), otherwise a plain Array of Numbers (safe range) / BigInts. */
function readI64List(buffer, byteOffset, n) {
  const words = new Int32Array(buffer, byteOffset, n * 2);
  let fits = true;
  for (let k = 0, j = 0; k < n; k++, j += 2) {
    if (words[j + 1] !== (words[j] >> 31)) { fits = false; break; } // hi word must be lo's sign extension
  }
  if (fits) {
    const out = new Int32Array(n);
    for (let k = 0, j = 0; k < n; k++, j += 2) out[k] = words[j];
    return out;
  }
  const big = new BigInt64Array(buffer, byteOffset, n);
  const out = new Array(n);
  for (let k = 0; k < n; k++) out[k] = fromI64(big[k]);
  return out;
}

/** Call `fn` on an instantiated kernel.
 *   paramTypes: the kernel's annotations in order, e.g. ["list[int]","int","int","int","list[int]"]
 *               or, for M2c typed arrays, ["Array[uint8, 2]","int","int","int","Array[uint8, 2]"]
 *   args:       matching JS values -- int: number|bigint, float: number, bool: boolean,
 *               list[int]: Int32Array | BigInt64Array | number[]; list[float]: Float64Array | number[];
 *               Array[dtype]: the dtype's TypedArray (uint8->Uint8Array, int32->Int32Array,
 *               int64->BigInt64Array, float32->Float32Array, float64->Float64Array);
 *               Array[dtype, 2]: an Array of `nrows` such TypedArray rows (rectangular)
 *   opts.readBack: indices of list/array params to read back from memory after the call (the `out`
 *               buffers); an array out-param is written back into the SAME TypedArray(s) in place
 *   opts.returnType: "int" | "float" | "bool" | "None" (default "int")
 * Returns { value, outs: {index: TypedArray}, heapBytes }. Throws on a WASM trap or an i64
 * overflow flag -- never falls back to anything. */
export function call(kernel, fn, paramTypes, args, opts = {}) {
  const ex = kernel.exports;
  const f = ex[fn];
  if (typeof f !== "function") throw new Error(`pythscribe ffi: wasm exports no function named ${fn}`);
  if (paramTypes.length !== args.length) throw new Error(`pythscribe ffi: ${fn} takes ${paramTypes.length} params, got ${args.length} args`);
  const readBack = new Set(opts.readBack ?? []);
  const listIdx = [];
  const arraySpecs = new Map(); // i -> {dtype, ndim} (M2c typed arrays)
  for (let i = 0; i < paramTypes.length; i++) {
    const t = paramTypes[i];
    if (t in ELEM_BYTES) { listIdx.push(i); continue; }
    const spec = arrayParamSpec(t);
    if (spec) { arraySpecs.set(i, spec); continue; }
    if (!SCALAR_PARAMS.has(t)) throw new Error(`pythscribe ffi: unsupported param type ${t} (param ${i} of ${fn})`);
  }
  for (const i of readBack) if (!(paramTypes[i] in ELEM_BYTES) && !arraySpecs.has(i)) throw new Error(`pythscribe ffi: readBack index ${i} is not a list or array param`);
  if ((listIdx.length || arraySpecs.size) && (typeof ex.__alloc !== "function" || !ex.__heap_ptr)) {
    throw new Error(`pythscribe ffi: ${fn} has list/array params but the wasm exports no __alloc/__heap_ptr`);
  }
  const sp = ex.__heap_ptr ? ex.__heap_ptr.value : 0;
  if (ex.__ovf) ex.__ovf.value = 0; // a previous call that overflowed AND trapped must not poison this one (r1/S3)
  if (ex.__err_code) ex.__err_code.value = 0; // ditto for a previous raising call (B3); guard absence like __ovf
  try {
    // 1. allocate EVERY list first: __alloc may grow memory, which detaches earlier views
    const ptrs = new Map();
    for (const i of listIdx) {
      const n = args[i].length;
      // the typed-array views below need 8-byte alignment of the element block, and the
      // compiler's bump allocator does NOT align (an odd-length list[bool] leaves the heap at
      // 4 mod 8): round every request up to a multiple of 8 so the NEXT list stays aligned
      // (opus r3/NB-2 -- align, don't assert around it); the assert is the paired invariant
      const p = ex.__alloc((HEADER_BYTES + n * ELEM_BYTES[paramTypes[i]] + 7) & ~7);
      if ((p + HEADER_BYTES) % 8 !== 0) throw new Error(`pythscribe ffi: list ${i} of ${fn} landed at unaligned offset ${p}; the allocator layout changed`);
      ptrs.set(i, p);
    }
    // 2. write headers + elements
    for (const i of listIdx) {
      const p = ptrs.get(i);
      const a = args[i];
      const n = a.length;
      const dv = new DataView(ex.memory.buffer);
      dv.setInt32(p, n, true);
      dv.setInt32(p + 4, n, true);
      const t = paramTypes[i];
      if (t === "list[int]") {
        if (a instanceof Int32Array) {
          const v = new Int32Array(ex.memory.buffer, p + HEADER_BYTES, n * 2); // (lo, hi) pairs, no BigInt
          for (let k = 0, j = 0; k < n; k++, j += 2) { const x = a[k]; v[j] = x; v[j + 1] = x >> 31; } // hi = sign extension
        } else if (a instanceof BigInt64Array) {
          new BigInt64Array(ex.memory.buffer, p + HEADER_BYTES, n).set(a);
        } else {
          const v = new BigInt64Array(ex.memory.buffer, p + HEADER_BYTES, n);
          for (let k = 0; k < n; k++) v[k] = toI64(a[k], `${fn} param ${i}[${k}]`);
        }
      } else if (t === "list[float]") {
        new Float64Array(ex.memory.buffer, p + HEADER_BYTES, n).set(a);
      } else {
        const v = new Int32Array(ex.memory.buffer, p + HEADER_BYTES, n);
        for (let k = 0; k < n; k++) v[k] = a[k] ? 1 : 0;
      }
    }
    // 2b. M2c typed arrays: each marshals through the TOTAL runtime check + ×8-header
    // bulk copy in (arrayToWasm allocs + writes with a fresh post-alloc DataView; list
    // element DATA already written above survives any memory.grow here). A wrong-dtype /
    // ragged / wrong-ndim buffer throws HERE (loud refusal — no JS twin on this path).
    for (const [i, spec] of arraySpecs) {
      ptrs.set(i, arrayToWasm(ex, args[i], spec.dtype, spec.ndim, `${fn} param ${i}`));
    }
    // 3. scalars
    const callArgs = new Array(args.length);
    for (let i = 0; i < args.length; i++) {
      const t = paramTypes[i];
      if (t in ELEM_BYTES || arraySpecs.has(i)) callArgs[i] = ptrs.get(i);
      else if (t === "int") callArgs[i] = toI64(args[i], `${fn} param ${i}`);
      else if (t === "float") callArgs[i] = Number(args[i]);
      else callArgs[i] = args[i] ? 1 : 0;
    }
    // 4. the WASM export itself -- a trap propagates as an error
    const raw = f(...callArgs);
    if (ex.__ovf && ex.__ovf.value) {
      ex.__ovf.value = 0;
      const e = new Error(`pythscribe ffi: ${fn}: i64 overflow inside the WASM kernel (refused; no JS re-run on this path)`);
      e.name = "OverflowError";
      throw e;
    }
    // B3: a `raise`/`assert`/failed-`try` body set the exported __err_code global and RETURNED a
    // sentinel; there is no glue on this path to check it, so throw here (never surface the
    // sentinel as a value). Guard its ABSENCE exactly as __ovf (a kernel without needs_errors
    // exports no __err_code). __err_msg is a glue-internal memory pointer the shim does NOT read.
    if (ex.__err_code && ex.__err_code.value) {
      const code = ex.__err_code.value;
      ex.__err_code.value = 0;
      if (ex.__err_msg) ex.__err_msg.value = 0; // reset the glue-internal pointer; never read it
      const name = ERROR_NAMES[code] || "Exception"; // codes >=100 are per-module custom names the shim can't map
      const detail = name === "Exception" ? ` (custom exception, code ${code})` : "";
      const e = new Error(`pythscribe ffi: ${fn}: ${name}${detail} raised inside the WASM kernel (refused; no JS re-run on this path)`);
      e.name = name;
      e.code = code;
      throw e;
    }
    // 5. read the out-buffers back BEFORE releasing the heap
    const outs = {};
    for (const i of readBack) {
      const p = ptrs.get(i);
      const spec = arraySpecs.get(i);
      if (spec) {
        // typed-array out-parameter: self-checking bulk copy back INTO the caller's
        // TypedArray(s) in place (arrayWriteBack validates the header first). `outs[i]`
        // is the caller's own array (mutated), the out-buffer contract.
        arrayWriteBack(ex, args[i], p, spec.dtype, spec.ndim, `${fn} param ${i}`);
        outs[i] = args[i];
        continue;
      }
      const n = args[i].length;
      const t = paramTypes[i];
      if (t === "list[int]") {
        outs[i] = readI64List(ex.memory.buffer, p + HEADER_BYTES, n);
      } else if (t === "list[float]") {
        outs[i] = new Float64Array(ex.memory.buffer, p + HEADER_BYTES, n).slice();
      } else {
        outs[i] = new Int32Array(ex.memory.buffer, p + HEADER_BYTES, n).slice();
      }
    }
    const rt = opts.returnType ?? "int";
    let value;
    if (rt === "int") value = typeof raw === "bigint" ? fromI64(raw) : raw;
    else if (rt === "float") value = Number(raw);
    else if (rt === "bool") value = Boolean(raw);
    else value = undefined;
    return { value, outs, heapBytes: ex.memory.buffer.byteLength };
  } finally {
    if (ex.__heap_ptr) ex.__heap_ptr.value = sp;
    if (ex.__ovf) ex.__ovf.value = 0;
    if (ex.__err_code) ex.__err_code.value = 0; // B3: never leak a set code across calls (mirrors __ovf)
    if (ex.__err_msg) ex.__err_msg.value = 0;
  }
}
