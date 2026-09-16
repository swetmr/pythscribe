// pythscribe.gradio -- the IN-TAB scalar client for float-returning `@wasm` kernels (v0.2.6 fix A).
//
// NOT an ES module: a plain script body defining `makeScalarClient(ffi, spec)`, inlined by
// `pythscribe.gradio.browser.client_side(...).loader_js` into a Gradio `js=` load hook right after
// the FFI shim (`pythscribe/ffi/list_buffer.mjs`) has been imported and `resolveUrl` defined.
// Nothing here computes a result: the handler's Gradio input values -> the built-in transforms
// below (ONE copy; `Arg.<name>` on the Python side indexes this table) -> the kernel's OWN .wasm
// through the shim (`ffi.call`, which REFUSES rather than re-running on a JS twin) -> one float.
// The server does nothing for the computation. The INPUTS are Gradio component values (state the
// server may hold); "the computation never leaves the tab" is the claim, not "the input never
// reaches the server".
//
// `spec` (built server-side by browser.py from the kernels' statically-read signatures):
//   { <name>: { fn: <wasm export>, wasm: <url>, param_types: [...], return_type: "float", params: [names] } }

/** Several `client_side(...)` objects may share one `global_name` in one app (each contributes a load
 * hook): the hooks COMPOSE -- a later hook merges its spec into the live client's instead of replacing
 * it -- and a display name already bound to a DIFFERENT kernel (fn / wasm / signature) is refused
 * (thrown inside the hook -> `loadError`, surfaced by every handler), never silently overridden. */
function mergeScalarSpec(existing, spec) {
  const prior = existing && existing.spec ? existing.spec : null;
  if (!prior) return spec;
  const merged = Object.assign({}, prior);
  for (const name of Object.keys(spec)) {
    const a = prior[name], b = spec[name];
    if (a && JSON.stringify(a) !== JSON.stringify(b)) {
      throw new Error("pythscribe scalar client: kernel name `" + name + "` is already bound to a different kernel on this page (" + a.fn + " @ " + a.wasm + " vs " + b.fn + " @ " + b.wasm + "); give the second client_side(...) another display name or global_name");
    }
    merged[name] = b;
  }
  return merged;
}

function makeScalarClient(ffi, spec) {
  const kernels = new Map();   // name -> instantiated kernel (once per kernel)

  const _DECIMAL = /^[+-]?(\d+\.?\d*|\.\d+)([eE][+-]?\d+)?$/;
  const num = (v, what) => {
    // Only a number or a decimal-shaped string is a number here. Arrays/objects/bool/null and
    // non-decimal strings ("0x10", "1,2", "") are REFUSED, not coerced (Number([])===0,
    // Number("0x10")===16 would silently corrupt the call otherwise -- e.g. an empty multiselect).
    if (typeof v === "number") { if (!Number.isFinite(v)) throw new Error("expected " + what + ", got " + JSON.stringify(v)); return v; }
    if (typeof v !== "string" || !_DECIMAL.test(v.trim())) throw new Error("expected " + what + ", got " + JSON.stringify(v));
    const x = Number(v.trim());
    if (!Number.isFinite(x)) throw new Error("expected " + what + ", got " + JSON.stringify(v));
    return x;
  };
  const items = (v) => Array.isArray(v) ? v : String(v == null ? "" : v).split(/[\s,;]+/).filter((s) => s !== "");
  // The built-in input transforms. Every refusal THROWS (surfaced by `invoke`); nothing is
  // silently coerced: `int` refuses 12.5 (never truncates), `bool` refuses "false" (a string).
  const T = {
    float: (v) => num(v, "a finite number"),
    int: (v) => { const x = num(v, "an integer"); if (!Number.isSafeInteger(x)) throw new Error("expected an integer (|v| <= 2**53-1), got " + JSON.stringify(v)); return x; },
    bool: (v) => { if (typeof v === "boolean") return v; if (v === 0 || v === 1) return v === 1; throw new Error("expected a boolean, got " + JSON.stringify(v)); },
    digits: (v) => Array.from(String(v == null ? "" : v)).filter((c) => c >= "0" && c <= "9").map(Number),
    utf8_bytes: (v) => Array.from(new TextEncoder().encode(String(v == null ? "" : v))),
    codepoints: (v) => Array.from(String(v == null ? "" : v), (c) => c.codePointAt(0)),
    floats: (v) => items(v).map((x) => T.float(x)),
    ints: (v) => items(v).map((x) => T.int(x)),
  };

  const bitsOf = (x) => { // the IEEE-754 bit pattern, little-endian hex (16 chars) -- the same form `pythscribe.build.runner.float_bits` emits
    const dv = new DataView(new ArrayBuffer(8)); dv.setFloat64(0, x, true);
    let s = ""; for (let i = 0; i < 8; i++) s += dv.getUint8(i).toString(16).padStart(2, "0"); return s;
  };

  const client = {
    spec, layout: ffi.LAYOUT_VERSION, path: "browser-wasm", transforms: T, last: null, lastError: null, loadError: null, calls: 0,

    async kernel(name) {
      const s = spec[name];
      if (!s) throw new Error("pythscribe scalar client: unknown kernel " + name + " (have: " + Object.keys(spec).join(", ") + ")");
      let k = kernels.get(name);
      if (!k) { k = await ffi.instantiate(resolveUrl(s.wasm)); kernels.set(name, k); }
      return k;
    },

    /** The kernel's positional args from the handler's inputs: `argSpecs[i]` produces parameter i
     * -- {t: "const", v} embeds a value, {t: "js", f} runs author JS on the next input, {t: <name>}
     * a built-in transform on the next input. The handler's input COUNT must equal the number of
     * consuming specs (a count mismatch is refused). ORDER is NOT checked here: a positional
     * `inputs=[...]` in the wrong order (right length) is a shifted call -- use `call_js`'s named
     * `{param: (Arg, component)}` form, which orders the inputs by signature, to prevent it. */
    args(name, inputs, argSpecs) {
      const s = spec[name];
      if (argSpecs.length !== s.param_types.length) throw new Error("pythscribe scalar client: " + name + " takes " + s.param_types.length + " parameter(s), got " + argSpecs.length + " arg spec(s)");
      const need = argSpecs.filter((a) => a.t !== "const").length;
      if (inputs.length !== need) throw new Error("pythscribe scalar client: " + name + " expects " + need + " Gradio input(s) for [" + s.params.join(", ") + "], the handler got " + inputs.length + " (check inputs=[...])");
      const out = new Array(argSpecs.length);
      let j = 0;
      for (let i = 0; i < argSpecs.length; i++) {
        const a = argSpecs[i];
        if (a.t === "const") { out[i] = a.v; continue; }
        const v = inputs[j++];
        try {
          if (a.t === "js") out[i] = a.f(v);
          else if (T[a.t]) out[i] = T[a.t](v);
          else throw new Error("unknown transform " + JSON.stringify(a.t));
        } catch (e) {
          throw new Error("pythscribe scalar client: " + name + " parameter `" + s.params[i] + "` (" + a.t + "): " + String(e && e.message ? e.message : e));
        }
      }
      return out;
    },

    /** Run kernel `name` on already-produced positional `args`. Returns { value, bits, ms } --
     * `value` is the export's float return converted by the shim (`returnType: "float"`). */
    async run(name, args) {
      const s = spec[name];
      const k = await this.kernel(name);
      const t0 = performance.now();
      const res = ffi.call(k, s.fn, s.param_types, args, { returnType: s.return_type });
      const ms = performance.now() - t0;
      this.calls += 1;
      const rec = { name, value: res.value, bits: bitsOf(res.value), ms, args };
      this.last = rec;
      return rec;
    },

    /** The one-call form for a `js=` handler (emitted by `ClientSide.call_js`): inputs -> args ->
     * run -> `fmt(value, info)` or the raw Number; `[that, statusLine]` when `withStatus`. A
     * failure (a refused input, a mis-wired handler, a trap, an i64 overflow) is SURFACED as an
     * 'error (...)' string in the output and in `lastError` -- Gradio's event runner would
     * otherwise swallow a thrown handler silently -- and nothing falls back to anything. */
    async invoke(name, inputs, argSpecs, fmt, withStatus) {
      try {
        const args = this.args(name, inputs, argSpecs);
        const r = await this.run(name, args);
        const info = { kernel: name, ms: r.ms, args, bits: r.bits };
        const value = fmt ? fmt(r.value, info) : r.value;
        const status = name + ": " + r.ms.toFixed(2) + " ms @wasm in-tab (call " + this.calls + "), 0 round-trips, layout " + this.layout;
        this.lastError = null;
        return withStatus ? [value, status] : value;
      } catch (e) {
        this.lastError = String(e && e.message ? e.message : e);
        const msg = "error (in-tab @wasm, NO fallback): " + this.lastError;
        return withStatus ? [msg, msg] : msg;
      }
    },
  };
  return client;
}
