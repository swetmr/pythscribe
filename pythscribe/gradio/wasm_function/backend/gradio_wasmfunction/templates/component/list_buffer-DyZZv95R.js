import { ft as e } from "./index-client-uwN5Rvra.js";
//#region frontend/list_buffer.mjs
var t = "pyths-0.2.4-list-v1", n = {
	"list[int]": 8,
	"list[float]": 8,
	"list[bool]": 4
}, r = /* @__PURE__ */ new Set([
	"int",
	"float",
	"bool"
]), i = {
	1: "ValueError",
	2: "TypeError",
	3: "IndexError",
	4: "KeyError",
	5: "ZeroDivisionError",
	6: "AssertionError",
	7: "RuntimeError"
};
function a(e) {
	return typeof e == "bigint" || Number.isFinite(e);
}
function o(e) {
	let t = typeof window < "u" ? window.gradio_config : null, n = t && typeof t.root == "string" && t.root ? t.root : null;
	if (!n || !e.startsWith("/")) return e;
	try {
		return new URL(e.replace(/^\/+/, ""), n.endsWith("/") ? n : n + "/").href;
	} catch {
		return e;
	}
}
var s = "pyths-0.2.5-array-v2", c = {
	int32: {
		ctor: Int32Array,
		esize: 4,
		tag: 0
	},
	int64: {
		ctor: BigInt64Array,
		esize: 8,
		tag: 1
	},
	float32: {
		ctor: Float32Array,
		esize: 4,
		tag: 2
	},
	float64: {
		ctor: Float64Array,
		esize: 8,
		tag: 3
	},
	uint8: {
		ctor: Uint8Array,
		esize: 1,
		tag: 4
	}
}, l = /^\s*Array\[\s*([A-Za-z0-9_]+)\s*(?:,\s*([0-9]+)\s*)?\]\s*$/;
function u(e) {
	let t = l.exec(e || "");
	if (!t) return null;
	let n = t[1];
	if (!Object.prototype.hasOwnProperty.call(c, n)) return null;
	let r = t[2] === void 0 ? 1 : parseInt(t[2], 10);
	return r !== 1 && r !== 2 ? null : {
		dtype: n,
		ndim: r
	};
}
function d(e, t) {
	return ArrayBuffer.isView(e) && e.constructor === t.ctor && e.BYTES_PER_ELEMENT === t.esize;
}
function f(e, t, n, r, i) {
	let a = c[n], o, s, l;
	if (r === 1) {
		if (!d(t, a)) {
			let e = t && t.constructor && t.constructor.name ? t.constructor.name : typeof t;
			throw RangeError(`pythscribe ffi: ${i}: Array[${n}] expects a ${a.ctor.name} buffer (dtype/width match); got ${e} — refused (never read at the wrong width)`);
		}
		o = t.length, s = 0, l = t.length;
	} else {
		if (!Array.isArray(t)) throw RangeError(`pythscribe ffi: ${i}: Array[${n}, 2] expects an array of ${a.ctor.name} rows; got ${typeof t} — refused`);
		o = t.length, s = 0;
		for (let e = 0; e < o; e++) {
			if (!d(t[e], a)) throw RangeError(`pythscribe ffi: ${i}: Array[${n}, 2] row ${e} must be a ${a.ctor.name} buffer (dtype/width match) — refused`);
			if (e === 0 && (s = t[0].length), t[e].length !== s) throw RangeError(`pythscribe ffi: ${i}: Array[${n}, 2] is ragged (row ${e} length ${t[e].length} != ${s}); only C-contiguous rectangular 2-D arrays cross — refused`);
		}
		l = o * s;
	}
	let u = (16 + l * a.esize + 7 & -8) + 8, f = e.__alloc(u) + 7 & -8, p = new DataView(e.memory.buffer);
	if (p.setInt32(f, a.tag, !0), p.setInt32(f + 4, r, !0), p.setInt32(f + 8, o, !0), p.setInt32(f + 12, s, !0), r === 1) new a.ctor(e.memory.buffer, f + 16, l).set(t);
	else for (let n = 0; n < o; n++) new a.ctor(e.memory.buffer, f + 16 + n * s * a.esize, s).set(t[n]);
	return f;
}
function p(e, t, n, r, i, a) {
	let o = c[r], s = new DataView(e.memory.buffer), l = s.getInt32(n, !0), u = s.getInt32(n + 4, !0), d = s.getInt32(n + 8, !0), f = s.getInt32(n + 12, !0);
	if (l !== o.tag) throw Error(`pythscribe ffi: ${a}: array write-back dtype tag ${l} != expected ${o.tag} (${r}); a wrong-width read would corrupt values — refused`);
	if (u !== i) throw Error(`pythscribe ffi: ${a}: array write-back ndim ${u} != expected ${i} — refused`);
	if (i === 1) {
		if (d !== t.length) throw Error(`pythscribe ffi: ${a}: array out-parameter length changed (${t.length} -> ${d}); a fixed-capacity buffer cannot reflect a length-changing mutation — refused`);
		t.set(new o.ctor(e.memory.buffer, n + 16, d));
	} else {
		if (d !== t.length) throw Error(`pythscribe ffi: ${a}: 2-D array out-parameter row count changed (${t.length} -> ${d}) — refused`);
		for (let r = 0; r < d; r++) {
			if (t[r].length !== f) throw Error(`pythscribe ffi: ${a}: 2-D array out-parameter row ${r} length changed (${t[r].length} -> ${f}) — refused`);
			t[r].set(new o.ctor(e.memory.buffer, n + 16 + r * f * o.esize, f));
		}
	}
}
var m = 9007199254740991n, h = -9007199254740991n;
function g() {
	return globalThis.process !== void 0 && globalThis.process.versions != null && typeof globalThis.process.versions.node == "string";
}
var _ = {
	pow: Math.pow,
	sqrt: Math.sqrt,
	sin: Math.sin,
	cos: Math.cos,
	tan: Math.tan,
	asin: Math.asin,
	acos: Math.acos,
	atan: Math.atan,
	atan2: Math.atan2,
	log: Math.log,
	log2: Math.log2,
	log10: Math.log10,
	exp: Math.exp,
	ceil: Math.ceil,
	floor: Math.floor,
	fabs: Math.abs
}, v = "pyths.abi";
function y(e) {
	let t = WebAssembly.Module.customSections(e, v);
	if (t.length !== 1) throw Error(`pythscribe ffi: WASM ABI check failed: expected exactly one \`${v}\` custom section, found ${t.length} (this .wasm was not emitted by a pyths compiler this runtime can load; rebuild the artifact with \`pyths build --force\`)`);
	let n;
	try {
		n = JSON.parse(new TextDecoder().decode(new Uint8Array(t[0])));
	} catch (e) {
		throw Error(`pythscribe ffi: WASM ABI check failed: \`${v}\` section is not valid JSON (${e.message})`);
	}
	if (!n || typeof n != "object") throw Error(`pythscribe ffi: WASM ABI check failed: \`${v}\` section is not a JSON object`);
	for (let e of [
		"abi",
		"list_layout",
		"array_layout"
	]) if (!(e in n)) throw Error(`pythscribe ffi: WASM ABI check failed: \`${v}\` section lacks the \`${e}\` field`);
	return n;
}
function b(e, t, n) {
	let r = /* @__PURE__ */ Error(`pythscribe ffi: WASM ABI mismatch on \`${e}\`: module=${JSON.stringify(t)} runtime=${JSON.stringify(n)} -- rebuild the artifact with the matching pyths compiler (a mismatched layout would be read at the wrong width; refused, never silently)`);
	return r.name = "AbiMismatchError", r.field = e, r;
}
function x(e) {
	let n = y(e);
	if (n.abi !== 1) throw b("abi", n.abi, 1);
	if (n.list_layout !== "pyths-0.2.4-list-v1") throw b("list_layout", n.list_layout, t);
	if (n.array_layout !== "pyths-0.2.5-array-v2") throw b("array_layout", n.array_layout, s);
	return n;
}
function S(e) {
	let t = {};
	for (let n of WebAssembly.Module.imports(e)) {
		if (n.module === "math" && n.kind === "function" && Object.prototype.hasOwnProperty.call(_, n.name)) {
			(t.math ??= {})[n.name] = _[n.name];
			continue;
		}
		throw Error(`pythscribe ffi: unsupported WASM import ${n.module}.${n.name} (${n.kind})`);
	}
	return t;
}
async function C(t) {
	let n, r;
	if (t instanceof WebAssembly.Module) n = t, r = "module";
	else if (t instanceof ArrayBuffer || ArrayBuffer.isView(t)) n = await WebAssembly.compile(t), r = "bytes";
	else if (typeof t == "string") {
		if (g()) {
			let { readFile: i } = await import("./__vite-browser-external-B4M1AP5M.js").then((t) => /* @__PURE__ */ e(t.default, 1)), { fileURLToPath: a } = await import("./__vite-browser-external-B4M1AP5M.js").then((t) => /* @__PURE__ */ e(t.default, 1)), o = t.startsWith("file:") ? a(t) : t;
			n = await WebAssembly.compile(await i(o)), r = "node-file";
		} else typeof WebAssembly.compileStreaming == "function" ? (n = await WebAssembly.compileStreaming(fetch(t)), r = "streaming") : (n = await WebAssembly.compile(await fetch(t).then((e) => e.arrayBuffer())), r = "fetch-bytes");
	} else throw TypeError("pythscribe ffi: instantiate() wants a URL/path, bytes, or a WebAssembly.Module");
	let i = x(n), a = await WebAssembly.instantiate(n, S(n)), o = WebAssembly.Module.exports(n).map((e) => e.name);
	return {
		module: n,
		instance: a,
		exports: a.exports,
		exportNames: o,
		how: r,
		abi: i
	};
}
var w = 9223372036854775807n, T = -9223372036854775808n;
function E(e, t) {
	if (typeof e == "bigint") {
		if (e > w || e < T) throw RangeError(`pythscribe ffi: ${t}: ${e}n is outside the i64 range`);
		return e;
	}
	if (typeof e == "number") {
		if (!Number.isSafeInteger(e)) throw RangeError(`pythscribe ffi: ${t}: ${e} is not a safe integer (|v| <= 2**53-1); pass a BigInt for larger ints`);
		return BigInt(e);
	}
	if (typeof e == "boolean") return e ? 1n : 0n;
	throw TypeError(`pythscribe ffi: ${t}: expected an int, got ${typeof e}`);
}
function D(e) {
	return e >= h && e <= m ? Number(e) : e;
}
function O(e, t, n) {
	let r = new Int32Array(e, t, n * 2), i = !0;
	for (let e = 0, t = 0; e < n; e++, t += 2) if (r[t + 1] !== r[t] >> 31) {
		i = !1;
		break;
	}
	if (i) {
		let e = new Int32Array(n);
		for (let t = 0, i = 0; t < n; t++, i += 2) e[t] = r[i];
		return e;
	}
	let a = new BigInt64Array(e, t, n), o = Array(n);
	for (let e = 0; e < n; e++) o[e] = D(a[e]);
	return o;
}
function k(e, t, a, o, s = {}) {
	let c = e.exports, l = c[t];
	if (typeof l != "function") throw Error(`pythscribe ffi: wasm exports no function named ${t}`);
	if (a.length !== o.length) throw Error(`pythscribe ffi: ${t} takes ${a.length} params, got ${o.length} args`);
	let d = new Set(s.readBack ?? []), m = [], h = /* @__PURE__ */ new Map();
	for (let e = 0; e < a.length; e++) {
		let i = a[e];
		if (i in n) {
			m.push(e);
			continue;
		}
		let o = u(i);
		if (o) {
			h.set(e, o);
			continue;
		}
		if (!r.has(i)) throw Error(`pythscribe ffi: unsupported param type ${i} (param ${e} of ${t})`);
	}
	for (let e of d) if (!(a[e] in n) && !h.has(e)) throw Error(`pythscribe ffi: readBack index ${e} is not a list or array param`);
	if ((m.length || h.size) && (typeof c.__alloc != "function" || !c.__heap_ptr)) throw Error(`pythscribe ffi: ${t} has list/array params but the wasm exports no __alloc/__heap_ptr`);
	let g = c.__heap_ptr ? c.__heap_ptr.value : 0;
	c.__ovf && (c.__ovf.value = 0), c.__err_code && (c.__err_code.value = 0);
	try {
		let e = /* @__PURE__ */ new Map();
		for (let r of m) {
			let i = o[r].length, s = c.__alloc(8 + i * n[a[r]] + 7 & -8);
			if ((s + 8) % 8 != 0) throw Error(`pythscribe ffi: list ${r} of ${t} landed at unaligned offset ${s}; the allocator layout changed`);
			e.set(r, s);
		}
		for (let n of m) {
			let r = e.get(n), i = o[n], s = i.length, l = new DataView(c.memory.buffer);
			l.setInt32(r, s, !0), l.setInt32(r + 4, s, !0);
			let u = a[n];
			if (u === "list[int]") {
				if (i instanceof Int32Array) {
					let e = new Int32Array(c.memory.buffer, r + 8, s * 2);
					for (let t = 0, n = 0; t < s; t++, n += 2) {
						let r = i[t];
						e[n] = r, e[n + 1] = r >> 31;
					}
				} else if (i instanceof BigInt64Array) new BigInt64Array(c.memory.buffer, r + 8, s).set(i);
				else {
					let e = new BigInt64Array(c.memory.buffer, r + 8, s);
					for (let r = 0; r < s; r++) e[r] = E(i[r], `${t} param ${n}[${r}]`);
				}
			} else if (u === "list[float]") new Float64Array(c.memory.buffer, r + 8, s).set(i);
			else {
				let e = new Int32Array(c.memory.buffer, r + 8, s);
				for (let t = 0; t < s; t++) e[t] = +!!i[t];
			}
		}
		for (let [n, r] of h) e.set(n, f(c, o[n], r.dtype, r.ndim, `${t} param ${n}`));
		let r = Array(o.length);
		for (let i = 0; i < o.length; i++) {
			let s = a[i];
			s in n || h.has(i) ? r[i] = e.get(i) : s === "int" ? r[i] = E(o[i], `${t} param ${i}`) : s === "float" ? r[i] = Number(o[i]) : r[i] = +!!o[i];
		}
		let u = l(...r);
		if (c.__ovf && c.__ovf.value) {
			c.__ovf.value = 0;
			let e = /* @__PURE__ */ Error(`pythscribe ffi: ${t}: i64 overflow inside the WASM kernel (refused; no JS re-run on this path)`);
			throw e.name = "OverflowError", e;
		}
		if (c.__err_code && c.__err_code.value) {
			let e = c.__err_code.value;
			c.__err_code.value = 0, c.__err_msg && (c.__err_msg.value = 0);
			let n = i[e] || "Exception", r = n === "Exception" ? ` (custom exception, code ${e})` : "", a = /* @__PURE__ */ Error(`pythscribe ffi: ${t}: ${n}${r} raised inside the WASM kernel (refused; no JS re-run on this path)`);
			throw a.name = n, a.code = e, a;
		}
		let g = {};
		for (let n of d) {
			let r = e.get(n), i = h.get(n);
			if (i) {
				p(c, o[n], r, i.dtype, i.ndim, `${t} param ${n}`), g[n] = o[n];
				continue;
			}
			let s = o[n].length, l = a[n];
			g[n] = l === "list[int]" ? O(c.memory.buffer, r + 8, s) : l === "list[float]" ? new Float64Array(c.memory.buffer, r + 8, s).slice() : new Int32Array(c.memory.buffer, r + 8, s).slice();
		}
		let _ = s.returnType ?? "int", v;
		return v = _ === "int" ? typeof u == "bigint" ? D(u) : u : _ === "float" ? Number(u) : _ === "bool" ? !!u : void 0, {
			value: v,
			outs: g,
			heapBytes: c.memory.buffer.byteLength
		};
	} finally {
		c.__heap_ptr && (c.__heap_ptr.value = g), c.__ovf && (c.__ovf.value = 0), c.__err_code && (c.__err_code.value = 0), c.__err_msg && (c.__err_msg.value = 0);
	}
}
//#endregion
export { o as a, a as i, k as n, C as r, t };
