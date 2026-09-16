//#region frontend/node_modules/svelte/src/internal/shared/utils.js
var e = Array.isArray, t = Array.prototype.indexOf, n = Array.prototype.includes, r = Array.from, i = Object.defineProperty, a = Object.getOwnPropertyDescriptor, o = Object.prototype, s = Array.prototype, c = Object.getPrototypeOf, l = Object.isExtensible, u = () => {};
function d(e) {
	for (var t = 0; t < e.length; t++) e[t]();
}
function f() {
	var e, t;
	return {
		promise: new Promise((n, r) => {
			e = n, t = r;
		}),
		resolve: e,
		reject: t
	};
}
var p = 1024, m = 2048, h = 4096, g = 8192, ee = 16384, _ = 32768, v = 1 << 25, te = 65536, ne = 1 << 19, re = 1 << 20, ie = 65536, ae = 1 << 21, oe = 1 << 22, se = 1 << 23, ce = Symbol("$state"), le = Symbol("component"), ue = Symbol("legacy props"), de = Symbol("attributes"), fe = Symbol("class"), pe = Symbol("style"), me = Symbol("text"), y = new class extends Error {
	name = "StaleReactionError";
	message = "The reaction that called `getAbortSignal()` was re-run or destroyed";
}();
globalThis.document?.contentType;
//#endregion
//#region frontend/node_modules/svelte/src/constants.js
var b = Symbol("uninitialized");
function he() {
	console.warn("https://svelte.dev/e/derived_inert");
}
function ge() {
	console.warn("https://svelte.dev/e/svelte_boundary_reset_noop");
}
//#endregion
//#region frontend/node_modules/svelte/src/internal/client/reactivity/equality.js
function _e(e) {
	return e === this.v;
}
function ve(e, t) {
	return e == e ? e !== t || typeof e == "object" && !!e || typeof e == "function" : t == t;
}
function ye(e) {
	return !ve(e, this.v);
}
//#endregion
//#region frontend/node_modules/svelte/src/internal/client/errors.js
function be() {
	throw Error("https://svelte.dev/e/async_derived_orphan");
}
function xe() {
	throw Error("https://svelte.dev/e/effect_update_depth_exceeded");
}
function Se(e) {
	throw Error("https://svelte.dev/e/props_invalid_value");
}
function Ce(e) {
	throw Error("https://svelte.dev/e/rune_outside_svelte");
}
function we() {
	throw Error("https://svelte.dev/e/state_descriptors_fixed");
}
function Te() {
	throw Error("https://svelte.dev/e/state_prototype_fixed");
}
function Ee() {
	throw Error("https://svelte.dev/e/state_unsafe_mutation");
}
function De() {
	throw Error("https://svelte.dev/e/svelte_boundary_reset_onerror");
}
//#endregion
//#region frontend/node_modules/svelte/src/internal/client/context.js
var x = null;
function S(e) {
	x = e;
}
function Oe(e, t = !1, n) {
	x = {
		p: x,
		i: !1,
		c: null,
		e: null,
		s: e,
		x: null,
		r: W,
		l: null
	};
}
function ke(e) {
	var t = x, n = t.e;
	if (n !== null) {
		t.e = null;
		for (var r of n) jt(r);
	}
	return e !== void 0 && (t.x = e), t.i = !0, x = t.p, Ae(e);
}
function Ae(e = {}) {
	return i(e, le, { value: !0 }), e;
}
function C() {
	return !0;
}
//#endregion
//#region frontend/node_modules/svelte/src/internal/client/dom/task.js
var w = [];
function je() {
	var e = w;
	w = [], d(e);
}
function Me(e) {
	if (w.length === 0 && !et) {
		var t = w;
		queueMicrotask(() => {
			t === w && je();
		});
	}
	w.push(e);
}
//#endregion
//#region frontend/node_modules/svelte/src/internal/client/reactivity/status.js
var Ne = ~(m | h | p);
function T(e, t) {
	e.f = e.f & Ne | t;
}
function Pe(e) {
	e.f & 512 || e.deps === null ? T(e, p) : T(e, h);
}
//#endregion
//#region frontend/node_modules/svelte/src/internal/client/reactivity/utils.js
function Fe(e) {
	if (e !== null) for (let t of e) !(t.f & 2) || !(t.f & 65536) || (t.f ^= ie, Fe(t.deps));
}
function Ie(e, t, n) {
	e.f & 2048 ? t.add(e) : e.f & 4096 && n.add(e), Fe(e.deps), T(e, p);
}
//#endregion
//#region frontend/node_modules/svelte/src/internal/client/reactivity/store.js
var Le = !1;
function Re(e) {
	var t = Le;
	try {
		return Le = !1, [e(), Le];
	} finally {
		Le = t;
	}
}
//#endregion
//#region frontend/node_modules/svelte/src/internal/client/dom/elements/bindings/shared.js
function ze(e) {
	var t = V, n = W;
	U(null), G(null);
	try {
		return e();
	} finally {
		U(t), G(n);
	}
}
//#endregion
//#region frontend/node_modules/svelte/src/internal/client/reactivity/async.js
function Be(e, t, n, r) {
	let i = C() ? We : Ke;
	var a = e.filter((e) => !e.settled), o = t.map(i);
	if (n.length === 0 && a.length === 0) {
		r(o);
		return;
	}
	var s = W, c = Ve(), l = a.length === 1 ? a[0].promise : a.length > 1 ? Promise.all(a.map((e) => e.promise)) : null;
	function u(e) {
		if (!(s.f & 16384)) {
			c();
			try {
				r([...o, ...e]);
			} catch (e) {
				L(e, s);
			}
			He();
		}
	}
	var d = Ue();
	if (n.length === 0) {
		l.then(() => u([])).finally(d);
		return;
	}
	function f() {
		Promise.all(n.map((e) => /* @__PURE__ */ Ge(e))).then(u).catch((e) => L(e, s)).finally(d);
	}
	l ? l.then(() => {
		c(), f(), He();
	}) : f();
}
function Ve() {
	var e = W, t = V, n = x, r = O;
	return function(i = !0) {
		G(e), U(t), S(n), i && !(e.f & 16384) && (r?.activate(), r?.apply());
	};
}
function He(e = !0) {
	G(null), U(null), S(null), e && O?.deactivate();
}
function Ue() {
	var e = W, t = e.b, n = O, r = !!t?.is_rendered();
	return t?.update_pending_count(1, n), n.increment(r, e), () => {
		t?.update_pending_count(-1, n), n.decrement(r, e);
	};
}
/*#__NO_SIDE_EFFECTS__*/
function We(e) {
	var t = 2 | m;
	return W !== null && (W.f |= ne), {
		ctx: x,
		deps: null,
		effects: null,
		equals: _e,
		f: t,
		fn: e,
		reactions: null,
		rv: 0,
		v: b,
		wv: 0,
		parent: W,
		ac: null
	};
}
var E = Symbol("obsolete");
/*#__NO_SIDE_EFFECTS__*/
function Ge(e, t, n) {
	let r = W;
	r === null && be();
	var i = void 0, a = pt(b), o = !V, s = /* @__PURE__ */ new Set();
	return Nt(() => {
		var t = W, n = f();
		i = n.promise;
		try {
			Promise.resolve(e()).then(n.resolve, (e) => {
				e !== y && n.reject(e);
			}).finally(He);
		} catch (e) {
			n.reject(e), He();
		}
		var c = O;
		if (o) {
			if (t.f & 32768) var l = Ue();
			if (r.b?.is_rendered()) c.async_deriveds.get(t)?.reject(E);
			else for (let e of s.values()) e.reject(E);
			s.add(n), c.async_deriveds.set(t, n);
		}
		let u = (e, t = void 0) => {
			l?.(), s.delete(n), t !== E && (c.activate(), t ? (a.f |= se, mt(a, t)) : (a.f & 8388608 && (a.f ^= se), mt(a, e)), c.deactivate());
		};
		n.promise.then(u, (e) => u(null, e || "unknown"));
	}), At(() => {
		for (let e of s) e.reject(E);
	}), new Promise((e) => {
		function t(n) {
			function r() {
				n === i ? e(a) : t(i);
			}
			n.then(r, r);
		}
		t(i);
	});
}
/*#__NO_SIDE_EFFECTS__*/
function Ke(e) {
	let t = /* @__PURE__ */ We(e);
	return t.equals = ye, t;
}
function qe(e) {
	var t = e.effects;
	if (t !== null) {
		e.effects = null;
		for (var n = 0; n < t.length; n += 1) z(t[n]);
	}
}
function Je(e) {
	var t, n = W, r = e.parent;
	if (!B && r !== null && e.v !== b && r.f & 24576) return he(), e.v;
	G(r);
	try {
		e.f &= ~ie, qe(e), t = nn(e);
	} finally {
		G(n);
	}
	return t;
}
function Ye(e) {
	var t = Je(e);
	if (!e.equals(t) && (e.wv = $t(), (!O?.is_fork || e.deps === null) && (O === null ? e.v = t : (O.capture(e, t, !0), Qe?.capture(e, t, !0)), e.deps === null))) {
		T(e, p);
		return;
	}
	B || (k === null ? Pe(e) : (kt() || O?.is_fork) && k.set(e, t));
}
function Xe(e) {
	if (e.effects !== null) for (let t of e.effects) (t.teardown || t.ac) && (t.teardown?.(), t.ac !== null && ze(() => {
		t.ac.abort(y), t.ac = null;
	}), t.fn !== null && (t.teardown = u), on(t, 0), zt(t));
}
function Ze(e) {
	if (e.effects !== null) for (let t of e.effects) t.teardown && t.fn !== null && Q(t);
}
//#endregion
//#region frontend/node_modules/svelte/src/internal/client/reactivity/batch.js
var D = null, O = null, Qe = null, k = null, $e = null, et = !1, tt = !1, A = null, nt = null, rt = 0, it = 1, at = class e {
	id = it++;
	#e = !1;
	linked = !0;
	#t = null;
	#n = null;
	async_deriveds = /* @__PURE__ */ new Map();
	current = /* @__PURE__ */ new Map();
	previous = /* @__PURE__ */ new Map();
	#r = /* @__PURE__ */ new Set();
	#i = /* @__PURE__ */ new Set();
	#a = 0;
	#o = /* @__PURE__ */ new Map();
	#s = null;
	#c = [];
	#l = [];
	#u = /* @__PURE__ */ new Set();
	#d = /* @__PURE__ */ new Set();
	#f = /* @__PURE__ */ new Map();
	#p = /* @__PURE__ */ new Set();
	is_fork = !1;
	#m = !1;
	constructor() {
		D === null ? D = this : (D.#n = this, this.#t = D), D = this;
	}
	#h() {
		if (this.is_fork) return !0;
		for (let n of this.#o.keys()) {
			for (var e = n, t = !1; e.parent !== null;) {
				if (this.#f.has(e)) {
					t = !0;
					break;
				}
				e = e.parent;
			}
			if (!t) return !0;
		}
		return !1;
	}
	skip_effect(e) {
		this.#f.has(e) || this.#f.set(e, {
			d: [],
			m: []
		}), this.#p.delete(e);
	}
	unskip_effect(e, t = (e) => this.schedule(e)) {
		var n = this.#f.get(e);
		if (n) {
			this.#f.delete(e);
			for (var r of n.d) T(r, m), t(r);
			for (r of n.m) T(r, h), t(r);
		}
		this.#p.add(e);
	}
	#g() {
		this.#e = !0, rt++ > 1e3 && (this.#x(), ot());
		for (let e of this.#u) this.#d.delete(e), T(e, m), this.schedule(e);
		for (let e of this.#d) T(e, h), this.schedule(e);
		let t = this.#c;
		this.#c = [], this.apply();
		var n = A = [], r = [], i = nt = [];
		for (let e of t) try {
			this.#_(e, n, r);
		} catch (t) {
			throw ut(e), this.#h() || this.discard(), t;
		}
		if (O = null, i.length > 0) {
			var a = e.ensure();
			for (let e of i) a.schedule(e);
		}
		if (A = null, nt = null, this.#h()) {
			this.#b(r), this.#b(n);
			for (let [e, t] of this.#f) lt(e, t);
			i.length > 0 && O.#g();
			return;
		}
		let o = this.#v();
		if (o) {
			this.#b(r), this.#b(n), o.#y(this);
			return;
		}
		this.#u.clear(), this.#d.clear();
		for (let e of this.#r) e(this);
		this.#r.clear(), Qe = this, st(r), st(n), Qe = null, this.#s?.resolve();
		var s = O;
		if (this.#a === 0 && (this.#c.length === 0 || s !== null) && this.#x(), this.#c.length > 0) {
			if (s !== null) {
				let e = s;
				e.#c.push(...this.#c.filter((t) => !e.#c.includes(t)));
			} else s = this;
		}
		s !== null && (M.clear(), s.#g());
	}
	#_(e, t, n) {
		e.f ^= p;
		for (var r = e.first; r !== null;) {
			var i = r.f, a = !!(i & 96);
			if (!(a && i & 1024 || i & 8192 || this.#f.has(r)) && r.fn !== null) {
				a ? r.f ^= p : i & 4 ? t.push(r) : en(r) && (i & 16 && this.#d.add(r), Q(r));
				var o = r.first;
				if (o !== null) {
					r = o;
					continue;
				}
			}
			for (; r !== null;) {
				var s = r.next;
				if (s !== null) {
					r = s;
					break;
				}
				r = r.parent;
			}
		}
	}
	#v() {
		for (var e = this.#t; e !== null;) {
			if (!e.is_fork) {
				for (let [t, [, n]] of this.current) if (e.current.has(t) && !n) return e;
			}
			e = e.#t;
		}
		return null;
	}
	#y(e) {
		for (let [t, n] of e.current) !this.previous.has(t) && e.previous.has(t) && this.previous.set(t, e.previous.get(t)), this.current.set(t, n);
		for (let [t, n] of e.async_deriveds) {
			let e = this.async_deriveds.get(t);
			e && n.promise.then(e.resolve).catch(e.reject);
		}
		e.async_deriveds.clear(), this.transfer_effects(e.#u, e.#d);
		let t = (e) => {
			var n = e.reactions;
			if (n !== null && !(e.f & 2 && !(e.f & 6144))) for (let e of n) {
				var r = e.f;
				if (r & 2) t(e);
				else {
					var i = e;
					r & 4194320 && !this.async_deriveds.has(i) && (this.#d.delete(i), T(i, m), this.schedule(i));
				}
			}
		};
		for (let e of this.current.keys()) t(e);
		this.oncommit(() => e.discard()), e.#x(), O = this, this.#g();
	}
	#b(e) {
		for (var t = 0; t < e.length; t += 1) Ie(e[t], this.#u, this.#d);
	}
	capture(e, t, n = !1) {
		e.v !== b && !this.previous.has(e) && this.previous.set(e, e.v), e.f & 8388608 || (this.current.set(e, [t, n]), k?.set(e, t)), this.is_fork || (e.v = t);
	}
	activate() {
		O = this;
	}
	deactivate() {
		O = null, k = null;
	}
	flush() {
		try {
			tt = !0, O = this, this.#g();
		} finally {
			rt = 0, $e = null, A = null, nt = null, tt = !1, O = null, k = null, M.clear();
		}
	}
	discard() {
		for (let e of this.#i) e(this);
		this.#i.clear();
		for (let e of this.async_deriveds.values()) e.reject(E);
		this.#x(), this.#s?.resolve();
	}
	register_created_effect(e) {
		this.#l.push(e);
	}
	increment(e, t) {
		if (this.#a += 1, e) {
			let e = this.#o.get(t) ?? 0;
			this.#o.set(t, e + 1);
		}
	}
	decrement(e, t) {
		if (--this.#a, e) {
			let e = this.#o.get(t) ?? 0;
			e === 1 ? this.#o.delete(t) : this.#o.set(t, e - 1);
		}
		this.#m || (this.#m = !0, Me(() => {
			this.#m = !1, this.linked && this.flush();
		}));
	}
	transfer_effects(e, t) {
		for (let t of e) this.#u.add(t);
		for (let e of t) this.#d.add(e);
		e.clear(), t.clear();
	}
	oncommit(e) {
		this.#r.add(e);
	}
	ondiscard(e) {
		this.#i.add(e);
	}
	settled() {
		return (this.#s ??= f()).promise;
	}
	static ensure() {
		if (O === null) {
			let t = O = new e();
			!tt && Me(() => {
				t.#e || t.flush();
			});
		}
		return O;
	}
	apply() {
		k = null;
	}
	schedule(e) {
		if ($e = e, e.b?.is_pending && e.f & 16777228 && !(e.f & 32768)) {
			e.b.defer_effect(e);
			return;
		}
		for (var t = e; t.parent !== null;) {
			t = t.parent;
			var n = t.f;
			if (A !== null && t === W && (V === null || !(V.f & 2))) return;
			if (n & 96) {
				if (!(n & 1024)) return;
				t.f ^= p;
			}
		}
		this.#c.push(t);
	}
	#x() {
		if (this.linked) {
			var e = this.#t, t = this.#n;
			e === null || (e.#n = t), t === null ? D = e : t.#t = e, this.linked = !1;
		}
	}
};
function ot() {
	try {
		xe();
	} catch (e) {
		L(e, $e);
	}
}
var j = null;
function st(e) {
	var t = e.length;
	if (t !== 0) {
		for (var n = 0; n < t;) {
			var r = e[n++];
			if (!(r.f & 24576) && en(r) && (j = /* @__PURE__ */ new Set(), Q(r), r.deps === null && r.first === null && r.nodes === null && r.teardown === null && r.ac === null && Ht(r), j?.size > 0)) {
				M.clear();
				for (let e of j) {
					if (e.f & 24576) continue;
					let t = [e], n = e.parent;
					for (; n !== null;) j.has(n) && (j.delete(n), t.push(n)), n = n.parent;
					for (let e = t.length - 1; e >= 0; e--) {
						let n = t[e];
						n.f & 24576 || Q(n);
					}
				}
				j.clear();
			}
		}
		j = null;
	}
}
function ct(e) {
	O.schedule(e);
}
function lt(e, t) {
	if (!(e.f & 32 && e.f & 1024)) {
		e.f & 2048 ? t.d.push(e) : e.f & 4096 && t.m.push(e), T(e, p);
		for (var n = e.first; n !== null;) lt(n, t), n = n.next;
	}
}
function ut(e) {
	T(e, p);
	for (var t = e.first; t !== null;) ut(t), t = t.next;
}
//#endregion
//#region frontend/node_modules/svelte/src/internal/client/reactivity/sources.js
var dt = /* @__PURE__ */ new Set(), M = /* @__PURE__ */ new Map(), ft = !1;
function pt(e, t) {
	return {
		f: 0,
		v: e,
		reactions: null,
		equals: _e,
		rv: 0,
		wv: 0
	};
}
/*#__NO_SIDE_EFFECTS__*/
function N(e, t) {
	let n = pt(e, t);
	return Yt(n), n;
}
function P(e, t, n = !1) {
	return V !== null && (!H || V.f & 131072) && C() && V.f & 4325394 && (K === null || !K.has(e)) && Ee(), mt(e, n ? I(t) : t, nt);
}
function mt(e, t, n = null) {
	if (!e.equals(t)) {
		B ? M.set(e, t) : M.has(e) || M.set(e, e.v);
		var r = at.ensure();
		if (r.capture(e, t), e.f & 2) {
			let t = e;
			e.f & 2048 && Je(t), k === null && Pe(t);
		}
		e.wv = $t(), gt(e, m, n), C() && W !== null && W.f & 1024 && !(W.f & 96) && (Y === null ? Xt([e]) : Y.push(e)), !r.is_fork && dt.size > 0 && !ft && ht();
	}
	return t;
}
function ht() {
	ft = !1;
	for (let e of dt) {
		e.f & 1024 && T(e, h);
		let t;
		try {
			t = en(e);
		} catch {
			t = !0;
		}
		t && Q(e);
	}
	dt.clear();
}
function F(e) {
	P(e, e.v + 1);
}
function gt(e, t, n) {
	var r = e.reactions;
	if (r !== null) for (var i = C(), a = r.length, o = 0; o < a; o++) {
		var s = r[o], c = s.f;
		if (!(!i && s === W)) {
			var l = (c & m) === 0;
			if (l && T(s, t), c & 131072) dt.add(s);
			else if (c & 2) {
				var u = s;
				k?.delete(u), c & 65536 || (c & 512 && (W === null || !(W.f & 2097152)) && (s.f |= ie), gt(u, h, n));
			} else if (l) {
				var d = s;
				c & 16 && j !== null && j.add(d), n === null ? ct(d) : n.push(d);
			}
		}
	}
}
function I(t) {
	if (typeof t != "object" || !t || ce in t || le in t) return t;
	let n = c(t);
	if (n !== o && n !== s) return t;
	var r = /* @__PURE__ */ new Map(), i = e(t), l = /* @__PURE__ */ N(0), u = null, d = Z, f = (e) => {
		if (Z === d) return e();
		var t = V, n = Z;
		U(null), Qt(d);
		var r = e();
		return U(t), Qt(n), r;
	};
	return i && r.set("length", /* @__PURE__ */ N(t.length, u)), new Proxy(t, {
		defineProperty(e, t, n) {
			(!("value" in n) || n.configurable === !1 || n.enumerable === !1 || n.writable === !1) && we();
			var i = r.get(t);
			return i === void 0 ? f(() => {
				var e = /* @__PURE__ */ N(n.value, u);
				return r.set(t, e), e;
			}) : P(i, n.value, !0), !0;
		},
		deleteProperty(e, t) {
			var n = r.get(t);
			if (n === void 0) {
				if (t in e) {
					let e = f(() => /* @__PURE__ */ N(b, u));
					r.set(t, e), F(l);
				}
			} else P(n, b), F(l);
			return !0;
		},
		get(e, n, i) {
			if (n === ce) return t;
			var o = r.get(n), s = n in e;
			if (o === void 0 && (!s || a(e, n)?.writable) && (o = f(() => /* @__PURE__ */ N(I(s ? e[n] : b), u)), r.set(n, o)), o !== void 0) {
				var c = $(o);
				return c === b ? void 0 : c;
			}
			return Reflect.get(e, n, i);
		},
		getOwnPropertyDescriptor(e, t) {
			var n = Reflect.getOwnPropertyDescriptor(e, t);
			if (n && "value" in n) {
				var i = r.get(t);
				i && (n.value = $(i));
			} else if (n === void 0) {
				var a = r.get(t), o = a?.v;
				if (a !== void 0 && o !== b) return {
					enumerable: !0,
					configurable: !0,
					value: o,
					writable: !0
				};
			}
			return n;
		},
		has(e, t) {
			if (t === ce) return !0;
			var n = r.get(t), i = n !== void 0 && n.v !== b || Reflect.has(e, t);
			return (n !== void 0 || W !== null && (!i || a(e, t)?.writable)) && (n === void 0 && (n = f(() => /* @__PURE__ */ N(i ? I(e[t]) : b, u)), r.set(t, n)), $(n) === b) ? !1 : i;
		},
		set(e, t, n, o) {
			var s = r.get(t), c = t in e;
			if (i && t === "length") for (var d = n; d < s.v; d += 1) {
				var p = r.get(d + "");
				p === void 0 ? d in e && (p = f(() => /* @__PURE__ */ N(b, u)), r.set(d + "", p)) : P(p, b);
			}
			if (s === void 0) (!c || a(e, t)?.writable) && (s = f(() => /* @__PURE__ */ N(void 0, u)), P(s, I(n)), r.set(t, s));
			else {
				c = s.v !== b;
				var m = f(() => I(n));
				P(s, m);
			}
			var h = Reflect.getOwnPropertyDescriptor(e, t);
			if (h?.set && h.set.call(o, n), !c) {
				if (i && typeof t == "string") {
					var g = r.get("length"), ee = Number(t);
					Number.isInteger(ee) && ee >= g.v && P(g, ee + 1);
				}
				F(l);
			}
			return !0;
		},
		ownKeys(e) {
			$(l);
			var t = Reflect.ownKeys(e).filter((e) => {
				var t = r.get(e);
				return t === void 0 || t.v !== b;
			});
			for (var [n, i] of r) i.v !== b && !(n in e) && t.push(n);
			return t;
		},
		setPrototypeOf() {
			Te();
		}
	});
}
var _t, vt, yt, bt;
function xt() {
	if (_t === void 0) {
		_t = window, vt = /Firefox/.test(navigator.userAgent);
		var e = Element.prototype, t = Node.prototype, n = Text.prototype;
		yt = a(t, "firstChild").get, bt = a(t, "nextSibling").get, l(e) && (e[fe] = void 0, e[de] = null, e[pe] = void 0, e.__e = void 0), l(n) && (n[me] = void 0);
	}
}
function St(e = "") {
	return document.createTextNode(e);
}
/*@__NO_SIDE_EFFECTS__*/
function Ct(e) {
	return yt.call(e);
}
/*@__NO_SIDE_EFFECTS__*/
function wt(e) {
	return bt.call(e);
}
function Tt(e, t = !1) {
	return /* @__PURE__ */ Ct(e);
}
function Et(e, t, n) {
	return t == null || t === "http://www.w3.org/1999/xhtml" ? n ? document.createElement(e, { is: n }) : document.createElement(e) : n ? document.createElementNS(t, e, { is: n }) : document.createElementNS(t, e);
}
function Dt(e) {
	var t = W;
	if (t === null) return V.f |= se, e;
	if (!(t.f & 32768) && !(t.f & 4)) throw e;
	L(e, t);
}
function L(e, t) {
	if (!(t !== null && t.f & 16384)) {
		for (; t !== null;) {
			if (t.f & 128 && !(t.f & 33570816)) {
				if (!(t.f & 32768)) throw e;
				try {
					t.b.error(e);
					return;
				} catch (t) {
					e = t;
				}
			}
			t = t.parent;
		}
		throw e;
	}
}
//#endregion
//#region frontend/node_modules/svelte/src/internal/client/reactivity/effects.js
function Ot(e, t) {
	var n = t.last;
	n === null ? t.last = t.first = e : (n.next = e, e.prev = n, t.last = e);
}
function R(e, t) {
	var n = W;
	n !== null && n.f & 8192 && (e |= g);
	var r = {
		ctx: x,
		deps: null,
		nodes: null,
		f: e | m | 512,
		first: null,
		fn: t,
		last: null,
		next: null,
		parent: n,
		b: n && n.b,
		prev: null,
		teardown: null,
		wv: 0,
		ac: null
	};
	O?.register_created_effect(r);
	var i = r;
	if (e & 4) A === null ? at.ensure().schedule(r) : A.push(r);
	else if (t !== null) {
		try {
			Q(r);
		} catch (e) {
			throw z(r), e;
		}
		i.deps === null && i.teardown === null && i.nodes === null && i.first === i.last && !(i.f & 524288) && (i = i.first, e & 16 && e & 65536 && i !== null && (i.f |= te));
	}
	if (i !== null && (i.parent = n, n !== null && Ot(i, n), V !== null && V.f & 2 && !(e & 64))) {
		var a = V;
		(a.effects ??= []).push(i);
	}
	return r;
}
function kt() {
	return V !== null && !H;
}
function At(e) {
	let t = R(8, null);
	return T(t, p), t.teardown = e, t;
}
function jt(e) {
	return R(4 | re, e);
}
function Mt(e) {
	at.ensure();
	let t = R(64 | ne, e);
	return (e = {}) => new Promise((n) => {
		e.outro ? Ut(t, () => {
			z(t), n(void 0);
		}) : (z(t), n(void 0));
	});
}
function Nt(e) {
	return R(oe | ne, e);
}
function Pt(e, t = 0) {
	return R(8 | t, e);
}
function Ft(e, t = [], n = [], r = []) {
	Be(r, t, n, (t) => {
		R(8, () => {
			e(...t.map($));
		});
	});
}
function It(e, t = 0) {
	return R(16 | t, e);
}
function Lt(e) {
	return R(32 | ne, e);
}
function Rt(e) {
	var t = e.teardown;
	if (t !== null) {
		let n = B, r = V;
		Jt(!0), U(null);
		try {
			t.call(null);
		} catch (t) {
			L(t, e.parent);
		} finally {
			Jt(n), U(r);
		}
	}
}
function zt(e, t = !1) {
	var n = e.first;
	for (e.first = e.last = null; n !== null;) {
		let e = n.ac;
		e !== null && ze(() => {
			e.abort(y);
		});
		var r = n.next;
		n.f & 64 ? n.parent = null : z(n, t), n = r;
	}
}
function Bt(e) {
	for (var t = e.first; t !== null;) {
		var n = t.next;
		t.f & 32 || z(t), t = n;
	}
}
function z(e, t = !0) {
	var n = !1;
	(t || e.f & 262144) && e.nodes !== null && e.nodes.end !== null && (Vt(e.nodes.start, e.nodes.end), n = !0), e.f |= v, zt(e, t && !n), on(e, 0);
	var r = e.nodes && e.nodes.t;
	if (r !== null) for (let e of r) e.stop();
	Rt(e), e.f ^= v, e.f |= ee;
	var i = e.parent;
	i !== null && i.first !== null && Ht(e), e.next = e.prev = e.teardown = e.ctx = e.deps = e.fn = e.nodes = e.ac = e.b = null;
}
function Vt(e, t) {
	for (; e !== null;) {
		var n = e === t ? null : /* @__PURE__ */ wt(e);
		e.remove(), e = n;
	}
}
function Ht(e) {
	var t = e.parent, n = e.prev, r = e.next;
	n !== null && (n.next = r), r !== null && (r.prev = n), t !== null && (t.first === e && (t.first = r), t.last === e && (t.last = n));
}
function Ut(e, t, n = !0) {
	var r = [];
	e.f |= 256, Wt(e, r, !0);
	var i = () => {
		n && z(e), t && t();
	}, a = r.length;
	if (a > 0) {
		var o = () => --a || i();
		for (var s of r) s.out(o);
	} else i();
}
function Wt(e, t, n) {
	if (!(e.f & 8192)) {
		e.f ^= g;
		var r = e.nodes && e.nodes.t;
		if (r !== null) for (let e of r) (e.is_global || n) && t.push(e);
		for (var i = e.first; i !== null;) {
			var a = i.next;
			if (!(i.f & 64)) {
				var o = !!(i.f & 65536) || !!(i.f & 32) && !!(e.f & 16);
				Wt(i, t, o ? n : !1);
			}
			i = a;
		}
	}
}
function Gt(e, t) {
	if (e.nodes) for (var n = e.nodes.start, r = e.nodes.end; n !== null;) {
		var i = n === r ? null : /* @__PURE__ */ wt(n);
		t.append(n), n = i;
	}
}
//#endregion
//#region frontend/node_modules/svelte/src/internal/client/legacy.js
var Kt = null, qt = !1, B = !1;
function Jt(e) {
	B = e;
}
var V = null, H = !1;
function U(e) {
	V = e;
}
var W = null;
function G(e) {
	W = e;
}
var K = null;
function Yt(e) {
	V !== null && (K ??= /* @__PURE__ */ new Set()).add(e);
}
var q = null, J = 0, Y = null;
function Xt(e) {
	Y = e;
}
var Zt = 1, X = 0, Z = X;
function Qt(e) {
	Z = e;
}
function $t() {
	return ++Zt;
}
function en(e) {
	var t = e.f;
	if (t & 2048) return !0;
	if (t & 2 && (e.f &= ~ie), t & 4096) {
		for (var n = e.deps, r = n.length, i = 0; i < r; i++) {
			var a = n[i];
			if (en(a) && Ye(a), a.wv > e.wv) return !0;
		}
		t & 512 && k === null && T(e, p);
	}
	return !1;
}
function tn(e, t, n = !0) {
	var r = e.reactions;
	if (r !== null && !(K !== null && K.has(e))) for (var i = 0; i < r.length; i++) {
		var a = r[i];
		a.f & 2 ? tn(a, t, !1) : t === a && (n ? T(a, m) : a.f & 1024 && T(a, h), ct(a));
	}
}
function nn(e) {
	var t = q, n = J, r = Y, i = V, a = K, o = x, s = H, c = Z, l = e.f;
	q = null, J = 0, Y = null, V = l & 96 ? null : e, K = null, S(e.ctx), H = !1, Z = ++X, e.ac !== null && (ze(() => {
		e.ac.abort(y);
	}), e.ac = null);
	try {
		e.f |= ae;
		var u = e.fn, d = u();
		e.f |= _;
		var f = rn(e);
		if (C() && Y !== null && !H && f !== null && !(e.f & 6146)) for (var p = 0; p < Y.length; p++) tn(Y[p], e);
		if (i !== null && i !== e) {
			if (X++, i.deps !== null) for (let e = 0; e < n; e += 1) i.deps[e].rv = X;
			if (t !== null) for (let e of t) e.rv = X;
			Y !== null && (r === null ? r = Y : r.push(...Y));
		}
		return e.f & 8388608 && (e.f ^= se), d;
	} catch (t) {
		return rn(e), Dt(t);
	} finally {
		e.f ^= ae, q = t, J = n, Y = r, V = i, K = a, S(o), H = s, Z = c;
	}
}
function rn(e) {
	var t = e.deps, n = O?.is_fork;
	if (q !== null) {
		var r;
		if (n || on(e, J), t !== null && J > 0) for (t.length = J + q.length, r = 0; r < q.length; r++) t[J + r] = q[r];
		else e.deps = t = q;
		if (kt() && e.f & 512) for (r = J; r < t.length; r++) (t[r].reactions ??= []).push(e);
	} else !n && t !== null && J < t.length && (on(e, J), t.length = J);
	return t;
}
function an(e, r) {
	let i = r.reactions;
	if (i !== null) {
		var a = t.call(i, e);
		if (a !== -1) {
			var o = i.length - 1;
			o === 0 ? i = r.reactions = null : (i[a] = i[o], i.pop());
		}
	}
	if (i === null && r.f & 2 && (q === null || !n.call(q, r))) {
		var s = r;
		s.f & 512 && (s.f ^= 512, s.f &= ~ie), s.v !== b && Pe(s), s.ac !== null && ze(() => {
			s.ac.abort(y), s.ac = null, T(s, m);
		}), Xe(s), on(s, 0);
	}
}
function on(e, t) {
	var n = e.deps;
	if (n !== null) for (var r = t; r < n.length; r++) an(e, n[r]);
}
function Q(e) {
	var t = e.f;
	if (!(t & 16384)) {
		T(e, p);
		var n = W, r = qt;
		W = e, qt = !(t & 96);
		try {
			t & 16777232 ? Bt(e) : zt(e), Rt(e);
			var i = nn(e);
			e.teardown = typeof i == "function" ? i : null, e.wv = Zt;
		} finally {
			qt = r, W = n;
		}
	}
}
function $(e) {
	var t = !!(e.f & 2);
	if (Kt?.add(e), V !== null && !H && !(W !== null && W.f & 16384) && (K === null || !K.has(e))) {
		var r = V.deps;
		if (V.f & 2097152) e.rv < X && (e.rv = X, q === null && r !== null && r[J] === e ? J++ : q === null ? q = [e] : q.push(e));
		else {
			V.deps ??= [], n.call(V.deps, e) || V.deps.push(e);
			var i = e.reactions;
			i === null ? e.reactions = [V] : n.call(i, V) || i.push(V);
		}
	}
	if (B && M.has(e)) return M.get(e);
	if (t) {
		var a = e;
		if (B) {
			var o = a.v;
			return (!(a.f & 1024) && a.reactions !== null || cn(a)) && (o = Je(a)), M.set(a, o), o;
		}
		var s = !(a.f & 512) && !H && V !== null && (qt || !!(V.f & 512)), c = (a.f & _) === 0;
		en(a) && (s && (a.f |= 512), Ye(a)), s && !c && (Ze(a), sn(a));
	}
	if (k?.has(e)) return k.get(e);
	if (e.f & 8388608) throw e.v;
	return e.v;
}
function sn(e) {
	if (e.f |= 512, e.deps !== null) for (let t of e.deps) (t.reactions ??= []).push(e), t.f & 2 && !(t.f & 512) && (Ze(t), sn(t));
}
function cn(e) {
	if (e.v === b) return !0;
	if (e.deps === null) return !1;
	for (let t of e.deps) if (M.has(t) || t.f & 2 && cn(t)) return !0;
	return !1;
}
function ln(e) {
	var t = H;
	try {
		return H = !0, e();
	} finally {
		H = t;
	}
}
[.../* @__PURE__ */ "allowfullscreen.async.autofocus.autoplay.checked.controls.default.disabled.formnovalidate.indeterminate.inert.ismap.loop.multiple.muted.nomodule.novalidate.open.playsinline.readonly.required.reversed.seamless.selected.webkitdirectory.defer.disablepictureinpicture.disableremoteplayback".split(".")];
var un = ["touchstart", "touchmove"];
function dn(e) {
	return un.includes(e);
}
//#endregion
//#region frontend/node_modules/svelte/src/internal/client/dom/elements/events.js
var fn = Symbol("events"), pn = /* @__PURE__ */ new Set(), mn = /* @__PURE__ */ new Set(), hn = null, gn = !1;
function _n(e) {
	var t = this, n = t.ownerDocument, r = e.type, a = e.composedPath?.() || [], o = a[0] || e.target;
	hn = e, gn || (gn = !0, setTimeout(() => {
		gn = !1, hn = null;
	}));
	var s = 0, c = hn === e && e[fn];
	if (c) {
		var l = a.indexOf(c);
		if (l !== -1 && (t === document || t === window)) {
			e[fn] = t;
			return;
		}
		var u = a.indexOf(t);
		if (u === -1) return;
		l <= u && (s = l);
	}
	if (o = a[s] || e.target, o !== t) {
		i(e, "currentTarget", {
			configurable: !0,
			get() {
				return o || n;
			}
		});
		var d = V, f = W;
		U(null), G(null);
		try {
			for (var p, m = []; o !== null && o !== t;) {
				try {
					var h = o[fn]?.[r];
					h != null && (!o.disabled || e.target === o) && h.call(o, e);
				} catch (e) {
					p ? m.push(e) : p = e;
				}
				if (e.cancelBubble) break;
				s++, o = s < a.length ? a[s] : null;
			}
			if (p) {
				for (let e of m) queueMicrotask(() => {
					throw e;
				});
				throw p;
			}
		} finally {
			e[fn] = t, delete e.currentTarget, U(d), G(f);
		}
	}
}
//#endregion
//#region frontend/node_modules/svelte/src/internal/client/dom/reconciler.js
var vn = globalThis?.window?.trustedTypes && /* @__PURE__ */ globalThis.window.trustedTypes.createPolicy("svelte-trusted-html", { createHTML: (e) => e });
function yn(e) {
	return vn?.createHTML(e) ?? e;
}
function bn(e) {
	var t = Et("template");
	return t.innerHTML = yn(e.replaceAll("<!>", "<!---->")), t.content;
}
//#endregion
//#region frontend/node_modules/svelte/src/internal/client/dom/template.js
function xn(e, t) {
	var n = W;
	n.nodes === null && (n.nodes = {
		start: e,
		end: t,
		a: null,
		t: null
	});
}
/*#__NO_SIDE_EFFECTS__*/
function Sn(e, t) {
	var n = !!(t & 1), r = !!(t & 2), i, a = !e.startsWith("<!>");
	return () => {
		i === void 0 && (i = bn(a ? e : "<!>" + e), n || (i = /* @__PURE__ */ Ct(i)));
		var t = r || vt ? document.importNode(i, !0) : i.cloneNode(!0);
		if (n) {
			var o = /* @__PURE__ */ Ct(t), s = t.lastChild;
			xn(o, s);
		} else xn(t, t);
		return t;
	};
}
function Cn(e, t) {
	e !== null && e.before(t);
}
//#endregion
//#region frontend/node_modules/svelte/src/reactivity/create-subscriber.js
function wn(e) {
	let t = 0, n = pt(0), r;
	return () => {
		kt() && ($(n), Pt(() => (t === 0 && (r = ln(() => e(() => F(n)))), t += 1, () => {
			Me(() => {
				--t, t === 0 && (r?.(), r = void 0, F(n));
			});
		})));
	};
}
//#endregion
//#region frontend/node_modules/svelte/src/internal/client/dom/blocks/boundary.js
var Tn = te | ne;
function En(e, t, n, r) {
	new Dn(e, t, n, r);
}
var Dn = class {
	parent;
	is_pending = !1;
	transform_error;
	#e;
	#t;
	#n;
	#r;
	#i = null;
	#a = null;
	#o = null;
	#s = null;
	#c = 0;
	#l = 0;
	#u = !1;
	#d = /* @__PURE__ */ new Set();
	#f = /* @__PURE__ */ new Set();
	#p = null;
	#m = wn(() => (this.#p = pt(this.#c), () => {
		this.#p = null;
	}));
	constructor(e, t, n, r) {
		this.#e = e, this.#t = t, this.#n = (e) => {
			var t = W;
			t.b = this, t.f |= 128, n(e);
		}, this.parent = W.b, this.transform_error = r ?? this.parent?.transform_error ?? ((e) => e), this.#r = It(() => {
			this.#g();
		}, Tn);
	}
	#h(e) {
		var t = !1, n = !1;
		let r = () => {
			if (t) {
				ge();
				return;
			}
			t = !0, n && De(), this.#o !== null && Ut(this.#o, () => {
				this.#o = null;
			}), this.#v(() => {
				this.#g();
			});
		};
		return {
			reset: r,
			invoke_onerror: () => {
				try {
					n = !0, this.#t.onerror?.(e, r), n = !1;
				} catch (e) {
					L(e, this.#r && this.#r.parent);
				}
			}
		};
	}
	#g() {
		try {
			if (this.is_pending = this.has_pending_snippet(), this.#l = 0, this.#c = 0, this.#i = Lt(() => {
				this.#n(this.#e);
			}), this.#l > 0) {
				var e = this.#s = document.createDocumentFragment();
				Gt(this.#i, e);
				let t = this.#t.pending;
				this.#a = Lt(() => t(this.#e));
			} else this.#_(O);
		} catch (e) {
			this.error(e);
		}
	}
	#_(e) {
		this.is_pending = !1, e.transfer_effects(this.#d, this.#f);
	}
	defer_effect(e) {
		Ie(e, this.#d, this.#f);
	}
	is_rendered() {
		return !this.is_pending && (!this.parent || this.parent.is_rendered());
	}
	has_pending_snippet() {
		return !!this.#t.pending;
	}
	#v(e) {
		var t = W, n = V, r = x;
		G(this.#r), U(this.#r), S(this.#r.ctx);
		try {
			return at.ensure(), e();
		} finally {
			G(t), U(n), S(r);
		}
	}
	#y(e, t) {
		if (!this.has_pending_snippet()) {
			this.parent && this.parent.#y(e, t);
			return;
		}
		this.#l += e, this.#l === 0 && (this.#_(t), this.#a && Ut(this.#a, () => {
			this.#a = null;
		}), this.#s &&= (this.#e.before(this.#s), null));
	}
	update_pending_count(e, t) {
		this.#y(e, t), this.#c += e, !(!this.#p || this.#u) && (this.#u = !0, Me(() => {
			this.#u = !1, this.#p && mt(this.#p, this.#c);
		}));
	}
	get_effect_pending() {
		return this.#m(), $(this.#p);
	}
	error(e) {
		if (!this.#t.onerror && !this.#t.failed) throw e;
		O?.is_fork ? (this.#i && O.skip_effect(this.#i), this.#a && O.skip_effect(this.#a), this.#o && O.skip_effect(this.#o), O.oncommit(() => {
			this.#b(e);
		})) : this.#b(e);
	}
	#b(e) {
		this.#i &&= (z(this.#i), null), this.#a &&= (z(this.#a), null), this.#o &&= (z(this.#o), null);
		let t = this.#t.failed, n = (e) => {
			let { reset: n, invoke_onerror: r } = this.#h(e);
			r(), t && (this.#o = this.#v(() => {
				try {
					return Lt(() => {
						var r = W;
						r.b = this, r.f |= 128, t(this.#e, () => e, () => n);
					});
				} catch (e) {
					return L(e, this.#r.parent), null;
				}
			}));
		};
		Me(() => {
			var t;
			try {
				t = this.transform_error(e);
			} catch (e) {
				L(e, this.#r && this.#r.parent);
				return;
			}
			typeof t == "object" && t && typeof t.then == "function" ? t.then(n, (e) => L(e, this.#r && this.#r.parent)) : n(t);
		});
	}
};
function On(e, t) {
	var n = t == null ? "" : typeof t == "object" ? `${t}` : t;
	n !== (e[me] ??= e.nodeValue) && (e[me] = n, e.nodeValue = `${n}`);
}
function kn(e, t) {
	return jn(e, t);
}
var An = /* @__PURE__ */ new Map();
function jn(e, { target: t, anchor: n, props: i = {}, events: a, context: o, intro: s = !0, transformError: c }) {
	xt();
	var l = void 0, u = Mt(() => {
		var s = n ?? t.appendChild(St());
		En(s, { pending: () => {} }, (t) => {
			Oe({});
			var n = x;
			o && (n.c = o), a && (i.$$events = a), l = e(t, i) || Ae(), ke();
		}, c);
		var u = /* @__PURE__ */ new Set(), d = (e) => {
			for (var n = 0; n < e.length; n++) {
				var r = e[n];
				if (!u.has(r)) {
					u.add(r);
					var i = dn(r);
					for (let e of [t, document]) {
						var a = An.get(e);
						a === void 0 && (a = /* @__PURE__ */ new Map(), An.set(e, a));
						var o = a.get(r);
						o === void 0 ? (e.addEventListener(r, _n, { passive: i }), a.set(r, 1)) : a.set(r, o + 1);
					}
				}
			}
		};
		return d(r(pn)), mn.add(d), () => {
			for (var e of u) for (let n of [t, document]) {
				var r = An.get(n), i = r.get(e);
				--i == 0 ? (n.removeEventListener(e, _n), r.delete(e), r.size === 0 && An.delete(n)) : r.set(e, i);
			}
			mn.delete(d), s !== n && s.parentNode?.removeChild(s);
		};
	});
	return Mn.set(l, u), l;
}
var Mn = /* @__PURE__ */ new WeakMap();
function Nn(e, t) {
	let n = Mn.get(e);
	return n ? (Mn.delete(e), n(t)) : Promise.resolve();
}
//#endregion
//#region frontend/node_modules/svelte/src/internal/shared/attributes.js
var Pn = [..." 	\n\r\f\xA0\v﻿"];
function Fn(e, t, n) {
	var r = e == null ? "" : "" + e;
	if (t && (r = r ? r + " " + t : t), n) {
		for (var i of Object.keys(n)) if (n[i]) r = r ? r + " " + i : i;
		else if (r.length) for (var a = i.length, o = 0; (o = r.indexOf(i, o)) >= 0;) {
			var s = o + a;
			(o === 0 || Pn.includes(r[o - 1])) && (s === r.length || Pn.includes(r[s])) ? r = (o === 0 ? "" : r.substring(0, o)) + r.substring(s + 1) : o = s;
		}
	}
	return r === "" ? null : r;
}
//#endregion
//#region frontend/node_modules/svelte/src/internal/client/dom/elements/class.js
function In(e, t, n, r, i, a) {
	var o = e[fe];
	if (o !== n || o === void 0) {
		var s = Fn(n, r, a);
		s == null ? e.removeAttribute("class") : t ? e.className = s : e.setAttribute("class", s), e[fe] = n;
	} else if (a && i !== a) for (var c in a) {
		var l = !!a[c];
		(i == null || l !== !!i[c]) && e.classList.toggle(c, l);
	}
	return a;
}
//#endregion
//#region frontend/node_modules/svelte/src/internal/client/reactivity/props.js
function Ln(e, t, n, r) {
	var i = !0, o = !!(n & 8), s = !!(n & 16), c = r, l = !0, u = void 0, d = () => s && i ? (u ??= /* @__PURE__ */ We(r), $(u)) : (l && (l = !1, c = s ? ln(r) : r), c);
	let f;
	if (o) {
		var p = ce in e || ue in e;
		f = a(e, t)?.set ?? (p && t in e ? (n) => e[t] = n : void 0);
	}
	var m, h = !1;
	o ? [m, h] = Re(() => e[t]) : m = e[t], m === void 0 && r !== void 0 && (m = d(), f && (i && Se(t), f(m)));
	var g = i ? () => {
		var n = e[t];
		return n === void 0 ? d() : (l = !0, n);
	} : () => {
		var n = e[t];
		return n !== void 0 && (c = void 0), n === void 0 ? c : n;
	};
	if (i && !(n & 4)) return g;
	if (f) {
		var ee = e.$$legacy;
		return (function(e, t) {
			return arguments.length > 0 ? ((!i || !t || ee || h) && f(t ? g() : e), e) : g();
		});
	}
	var _ = !1, v = (n & 1 ? We : Ke)(() => (_ = !1, g()));
	o && $(v);
	var te = W;
	return (function(e, t) {
		if (arguments.length > 0) {
			let n = t ? $(v) : i && o ? I(e) : e;
			return P(v, n), _ = !0, c !== void 0 && (c = n), e;
		}
		return B && _ || te.f & 16384 ? v.v : $(v);
	});
}
//#endregion
export { Nn as a, Ft as c, On as i, Tt as l, In as n, Cn as o, kn as r, Sn as s, Ln as t, Ce as u };
