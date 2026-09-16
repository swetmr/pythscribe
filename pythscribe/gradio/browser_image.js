// pythscribe.gradio -- the IN-TAB image client for `@wasm` typed-array kernels (v0.2.6 fix B).
//
// NOT an ES module: a plain script body defining `makeImageClient(ffi, spec)`, inlined by
// `pythscribe.gradio.browser.browser_image_loader_js` into a Gradio `js=` load hook right after
// the FFI shim (`pythscribe/ffi/list_buffer.mjs`) has been imported. Nothing here computes a
// pixel: decode -> typed arrays -> the kernel's OWN .wasm (through the shim's typed-array path,
// which REFUSES rather than re-running on a JS twin) -> read `out` back -> canvas -> data URL.
// The server does nothing for the transform. The INPUT is a Gradio component value (a `file=`
// URL the server serves; an upload reaches the server first) -- it is fetched ONCE per URL and
// the decoded rows are cached, so a slider move never touches the server.
//
// `spec` (built server-side by browser.py from the kernels' statically-read signatures):
//   { <name>: { fn: <wasm export>, wasm: <url>, param_types: [...], return_type: "int",
//               params: [names], roles: { img, out[, h, w, oh, ow] }  (parameter INDICES),
//               extras: [indices of the remaining scalar params], extra_names: [their names] } }
function makeImageClient(ffi, spec) {
  const kernels = new Map();   // name -> instantiated kernel (once per kernel)
  const decoded = new Map();   // file url -> { rows, h, w } (bounded)
  // `resolveUrl` (root-absolute `/gradio_api/file=...` -> Gradio's configured app root) is the ONE
  // copy the load hook (`browser.py::_loader_hook`) defines in the enclosing scope, shared with
  // the scalar client.

  const client = {
    spec, layout: ffi.ARRAY_LAYOUT_VERSION, path: "browser-wasm", last: null, lastError: null, loadError: null, calls: 0,

    async kernel(name) {
      const s = spec[name];
      if (!s) throw new Error("pythscribe image client: unknown kernel " + name + " (have: " + Object.keys(spec).join(", ") + ")");
      let k = kernels.get(name);
      if (!k) { k = await ffi.instantiate(resolveUrl(s.wasm)); kernels.set(name, k); }
      return k;
    },

    /** Gradio's image value (FileData: {url, path, ...}) or a plain URL/data URL -> H rows of
     * Uint8Array(W*3) (row-major RGB, alpha stripped), decoded with colour management OFF so
     * the bytes are the file's own. Cached per URL (the 4 most recent): a slider move never
     * re-fetches. */
    async decode(file) {
      const url = typeof file === "string" ? file : file && (file.url || file.path);
      if (!url) throw new Error("pythscribe image client: no image");
      const hit = decoded.get(url);
      if (hit) return hit;
      const blob = await (await fetch(url)).blob();
      const bmp = await createImageBitmap(blob, { imageOrientation: "none", colorSpaceConversion: "none", premultiplyAlpha: "none" });
      const w = bmp.width, h = bmp.height;
      const cv = document.createElement("canvas"); cv.width = w; cv.height = h;
      const ctx = cv.getContext("2d", { willReadFrequently: true });
      ctx.drawImage(bmp, 0, 0); bmp.close();
      const rgba = ctx.getImageData(0, 0, w, h).data;
      const rows = new Array(h);
      for (let y = 0; y < h; y++) {
        const row = new Uint8Array(w * 3);
        for (let x = 0, j = y * w * 4, o = 0; x < w; x++, j += 4, o += 3) {
          row[o] = rgba[j]; row[o + 1] = rgba[j + 1]; row[o + 2] = rgba[j + 2];
        }
        rows[y] = row;
      }
      const d = { rows, h, w, url };
      decoded.set(url, d);
      while (decoded.size > 4) decoded.delete(decoded.keys().next().value);
      return d;
    },

    /** The kernel's extra scalars: an object keyed by PARAMETER NAME (preferred -- two same-typed
     * extras cannot swap) or an array in declaration order. Every extra must be supplied exactly
     * once; unknown / missing names are refused with the expected names spelled out. */
    _extras(s, name, extras) {
      const want = s.extra_names;
      const out = new Array(want.length);
      if (extras == null) extras = [];
      if (Array.isArray(extras)) {
        if (extras.length !== want.length) throw new Error("pythscribe image client: " + name + " wants extras [" + want.join(", ") + "] (" + want.length + "), got " + extras.length + " value(s)");
        for (let i = 0; i < want.length; i++) out[i] = extras[i];
        return out;
      }
      if (typeof extras !== "object") throw new Error("pythscribe image client: " + name + " extras must be an object keyed by name {" + want.join(", ") + "} or an array");
      const keys = Object.keys(extras);
      for (const k of keys) if (!want.includes(k)) throw new Error("pythscribe image client: " + name + " has no extra parameter `" + k + "` (extras: [" + want.join(", ") + "])");
      for (let i = 0; i < want.length; i++) {
        if (!(want[i] in extras)) throw new Error("pythscribe image client: " + name + " is missing extra `" + want[i] + "` (extras: [" + want.join(", ") + "])");
        out[i] = extras[want[i]];
      }
      return out;
    },

    /** Run kernel `name` on decoded `img` ({rows,h,w}). `extras`: see `_extras`; `dims`: {oh, ow}
     * (or a function of the decoded image returning it) for a kernel whose `out` has its own
     * shape (roles oh/ow), else `out` is [h, w*3]. Returns { ret, rows, h, w, ms } -- `rows` is
     * the kernel's `out`, read back from WASM memory by the shim's self-checking write-back;
     * `ret` is the export's scalar return. */
    async run(name, img, extras, dims) {
      const s = spec[name];
      const k = await this.kernel(name);
      const r = s.roles;
      if (typeof dims === "function") dims = dims(img);
      if (r.oh !== undefined && !(dims && Number.isInteger(dims.oh) && Number.isInteger(dims.ow) && dims.oh > 0 && dims.ow > 0)) {
        throw new Error("pythscribe image client: " + name + " needs dims {oh, ow} (positive integers)");
      }
      const oh = r.oh !== undefined ? dims.oh : img.h;
      const ow = r.ow !== undefined ? dims.ow : img.w;
      const out = new Array(oh);
      for (let y = 0; y < oh; y++) out[y] = new Uint8Array(ow * 3);
      const args = new Array(s.param_types.length);
      args[r.img] = img.rows; args[r.out] = out;
      if (r.h !== undefined) args[r.h] = img.h;
      if (r.w !== undefined) args[r.w] = img.w;
      if (r.oh !== undefined) args[r.oh] = oh;
      if (r.ow !== undefined) args[r.ow] = ow;
      const ex = this._extras(s, name, extras);
      for (let i = 0; i < s.extras.length; i++) args[s.extras[i]] = ex[i];
      const t0 = performance.now();
      const res = ffi.call(k, s.fn, s.param_types, args, { readBack: [r.out], returnType: s.return_type });
      const ms = performance.now() - t0;
      this.calls += 1;
      const rec = { name, ret: res.value, rows: res.outs[r.out], h: oh, w: ow, ms, extras: ex, src: img.url };
      this.last = rec;
      return rec;
    },

    /** H rows of Uint8Array(W*3) -> a PNG data URL (lossless), via a canvas. */
    encode(rows, h, w) {
      const cv = document.createElement("canvas"); cv.width = w; cv.height = h;
      const im = new ImageData(w, h);
      const d = im.data;
      for (let y = 0; y < h; y++) {
        const row = rows[y];
        for (let x = 0, o = 0, j = y * w * 4; x < w; x++, o += 3, j += 4) {
          d[j] = row[o]; d[j + 1] = row[o + 1]; d[j + 2] = row[o + 2]; d[j + 3] = 255;
        }
      }
      cv.getContext("2d").putImageData(im, 0, 0);
      return cv.toDataURL("image/png");
    },

    /** The one-call form for a `js=` handler: decode (cached) -> run -> encode. Returns
     * [ <gr.Image value: FileData with a data: URL>, <status line> ]. A failure (a refused
     * buffer, a trap, a bad image, bad extras) is SURFACED in the status line and `lastError`
     * -- Gradio's event runner would otherwise swallow a thrown handler silently -- and clears
     * the output; nothing falls back to anything. */
    async filter(file, name, extras, dims) {
      const t0 = performance.now();
      try {
        const img = await this.decode(file);
        const r = await this.run(name, img, extras, dims);
        const url = this.encode(r.rows, r.h, r.w);
        const total = performance.now() - t0;
        const value = { url, path: null, orig_name: name + ".png", mime_type: "image/png", size: null, is_stream: false, meta: { _type: "gradio.FileData" } };
        const status = name + ": " + r.ms.toFixed(1) + " ms @wasm in-tab (" + total.toFixed(1) + " ms incl. decode/encode), " + r.w + "x" + r.h + ", 0 round-trips, layout " + this.layout;
        this.lastError = null;
        return [value, status];
      } catch (e) {
        this.lastError = String(e && e.message ? e.message : e);
        return [null, "error (in-tab @wasm, NO fallback): " + this.lastError];
      }
    },
  };
  return client;
}
