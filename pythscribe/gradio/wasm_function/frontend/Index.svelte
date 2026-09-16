<script lang="ts">
	// pythscribe WasmFunction -- runs a `@wasm` kernel's compiled bundle IN THE BROWSER TAB
	// and hands the value back to the Gradio script through the component's value
	// (the return path). The Python side dispatches {fn, args, bundle, nonce, result:null};
	// this component imports `bundle` (an ES module whose glue instantiates the .wasm),
	// calls `fn`, and sets `result` with the IEEE-754 bit pattern of the float (the
	// authoritative value -- a JSON number would corrupt -0.0/inf/nan), a path marker, and
	// the resource-timing evidence that a .wasm was actually fetched in this tab. Everything
	// written here is CLIENT data; the server re-derives the path marker from its own counter.
	//
	// M1 (kind: "image"): the user picks an image HERE; it is decoded in the tab, box-downscaled
	// by the `@wasm` kernel (its own .wasm, called directly through the list-buffer FFI shim --
	// no JS twin on that path), encoded by the canvas, and the SMALL blob is uploaded as a
	// Gradio file. Only that blob crosses to Python. Without an artifact (or if the WASM path
	// fails) the ORIGINAL is uploaded and the server runs the Python fallback.
	import type { WasmFunctionProps, WasmFunctionEvents, WasmPayload, ImageResult, GradioFileData } from "./types";
	import { Gradio } from "@gradio/utils";
	import { Block, BlockTitle } from "@gradio/atoms";
	import { StatusTracker } from "@gradio/statustracker";
	import * as ffi from "./list_buffer.mjs";

	const props = $props();
	const gradio = new Gradio<WasmFunctionEvents, WasmFunctionProps>(props);
	// dispatches "change" whenever value changes (Python-set payload OR our result write)
	gradio.watch_for_change();

	const RUN_TIMEOUT_MS = 15000;

	// Status is DERIVED from the value (one source of truth): no separate state to drift.
	const status = $derived.by(() => {
		const v = gradio.props.value;
		if (!v || typeof v !== "object") return "idle";
		if (v.error) return "error: " + String(v.error);
		if (v.result) return `done (${v.result.path})`;
		if (v.kind === "image") return busy ? "running in browser" : v.wasm ? "pick an image: it is resized in this tab by @wasm before upload" : "pick an image: no artifact, the original is uploaded (Python fallback)";
		// v0.2.5 callback path: the React/ReactFlow island computes in-tab; no server round-trip.
		// SF-6: an explicit error state (a blocked/failed .wasm fetch), distinct from "still loading".
			if (v.kind === "flowgraph") {
				if (flow_marker && flow_marker.path === "error") return "the in-tab @wasm graph failed to load (see the node error)";
				return flow_marker && flow_marker.ready ? "graph computing in browser (@wasm, 0 round-trips)" : "loading the in-tab @wasm graph...";
			}
		if (v.bundle) return "running in browser";
		return "idle";
	});
	let last_key: string | null = null;
	let busy = $state(false);
	let file_input: HTMLInputElement | null = $state(null);
	let last_nonce_seen: number | null = null;
	// v0.2.5 callback path (kind: "flowgraph"): the lazily-loaded React island + its marker.
	let flow_el: HTMLDivElement | null = $state(null);
	let flow_marker: { path: string; ready: boolean } | null = $state(null);
	let flow_island: typeof import("./flow_island") | null = null;

	function f64bits(n: number): string {
		const dv = new DataView(new ArrayBuffer(8));
		dv.setFloat64(0, n, true);
		let s = "";
		for (let i = 0; i < 8; i++) s += dv.getUint8(i).toString(16).padStart(2, "0");
		return s;
	}

	/** true iff `payload` is still the value the component holds (guards a stale in-flight run) */
	function still_current(payload: WasmPayload): boolean {
		const v = gradio.props.value as WasmPayload | null;
		return !!v && v.nonce === payload.nonce;
	}

	function with_timeout<T>(p: Promise<T>, ms: number): Promise<T> {
		return new Promise<T>((resolve, reject) => {
			const t = setTimeout(() => reject(new Error(`timeout after ${ms} ms (bundle did not load/run)`)), ms);
			p.then(
				(v) => { clearTimeout(t); resolve(v); },
				(e) => { clearTimeout(t); reject(e); }
			);
		});
	}

	/** .wasm resource-timing entries observed in this tab, optionally only those that STARTED
	 * after `since` (performance.now()) so a run reports its own fetches, not the tab's history
	 * (opus r1/S5). */
	function wasm_fetched(since = 0): string[] {
		return performance
			.getEntriesByType("resource")
			.filter((e) => e.startTime >= since && /\.wasm(\?|$)/.test(e.name))
			.map((e) => e.name);
	}

	// Above this the in-tab path is refused up front and the original is uploaded (server
	// fallback): the i64 list costs 8 B/pixel of WASM heap on top of the RGBA + packed
	// copies, and Safari/iOS canvases silently go blank past ~16.7 MP (opus r1/S3, S12).
	const MAX_PIXELS_IN_TAB = 16_000_000;

	async function run(payload: WasmPayload): Promise<void> {
		const t_start = performance.now();
		try {
			const mod = await with_timeout(import(/* @vite-ignore */ payload.bundle as string), RUN_TIMEOUT_MS);
			const f = mod[payload.fn];
			if (typeof f !== "function") {
				throw new Error(`bundle exports no function named ${payload.fn}`);
			}
			const raw = f(...(payload.args as unknown[]));
			if (typeof raw === "bigint") {
				// plan §7 watch item: an int result outside 2**53 crosses as BigInt, which the
				// JSON return path cannot carry. Refuse loudly rather than round.
				throw new Error("BigInt result cannot cross the Gradio return path (int kernels are not admitted in M0)");
			}
			const value = Number(raw); // unboxes the glue's __PyFloatW wrapper for integer-valued floats
			if (!still_current(payload)) return; // a newer dispatch superseded this run
			gradio.props.value = {
				...payload,
				error: null,
				result: {
					value: Number.isFinite(value) ? value : null,
					bits: f64bits(value),
					path: "browser-wasm",
					python_calls: 0,
					wasm_fetched: wasm_fetched(t_start), // this run's fetches only (same meaning on both paths -- opus r2/NS-11)
					wasm_supported: typeof WebAssembly === "object",
					user_agent: navigator.userAgent
				}
			};
		} catch (e) {
			if (!still_current(payload)) return;
			gradio.props.value = { ...payload, result: null, error: String(e) };
		}
	}

	// ---------------------------------------------------------------- M1: image mode

	const CRC_TABLE = (() => {
		const t = new Uint32Array(256);
		for (let n = 0; n < 256; n++) {
			let c = n;
			for (let k = 0; k < 8; k++) c = c & 1 ? 0xedb88320 ^ (c >>> 1) : c >>> 1;
			t[n] = c >>> 0;
		}
		return t;
	})();
	/** CRC-32 (IEEE, zlib-compatible) over bytes -- the checksum the Python reference computes too */
	function crc32(bytes: Uint8Array): number {
		let c = 0xffffffff;
		for (let i = 0; i < bytes.length; i++) c = CRC_TABLE[(c ^ bytes[i]) & 0xff] ^ (c >>> 8);
		return (c ^ 0xffffffff) >>> 0;
	}

	function make_canvas(w: number, h: number): OffscreenCanvas | HTMLCanvasElement {
		if (typeof OffscreenCanvas !== "undefined") return new OffscreenCanvas(w, h);
		const c = document.createElement("canvas");
		c.width = w;
		c.height = h;
		return c;
	}

	async function decode_in_tab(file: File): Promise<{ w: number; h: number; data: Uint8ClampedArray }> {
		// raw decode: no EXIF orientation, no colour-management -- the same pixels Pillow's
		// Image.open gives the server-side reference (opus r1/S1)
		const bmp = await createImageBitmap(file, { imageOrientation: "none", colorSpaceConversion: "none" } as ImageBitmapOptions);
		const w = bmp.width, h = bmp.height; // read BEFORE close(): a closed bitmap reports 0x0
		if (!(w > 0 && h > 0)) throw new Error(`decoded image has no pixels (${w}x${h})`);
		if (w * h > MAX_PIXELS_IN_TAB) {
			bmp.close();
			throw new Error(`image has ${w * h} pixels, above the in-tab cap of ${MAX_PIXELS_IN_TAB}; uploading the original for the server path`);
		}
		const cv = make_canvas(w, h);
		const ctx = cv.getContext("2d", { willReadFrequently: true }) as OffscreenCanvasRenderingContext2D | CanvasRenderingContext2D | null;
		if (!ctx) throw new Error("no 2d canvas context");
		ctx.drawImage(bmp, 0, 0);
		const img = ctx.getImageData(0, 0, w, h);
		bmp.close();
		return { w, h, data: img.data };
	}

	async function encode_in_tab(rgba: Uint8ClampedArray, w: number, h: number, quality: number): Promise<Blob> {
		const cv = make_canvas(w, h);
		const ctx = cv.getContext("2d") as OffscreenCanvasRenderingContext2D | CanvasRenderingContext2D | null;
		if (!ctx) throw new Error("no 2d canvas context");
		ctx.putImageData(new ImageData(rgba, w, h), 0, 0);
		if ("convertToBlob" in cv) return (cv as OffscreenCanvas).convertToBlob({ type: "image/jpeg", quality });
		return new Promise<Blob>((res, rej) =>
			(cv as HTMLCanvasElement).toBlob((b) => (b ? res(b) : rej(new Error("toBlob failed"))), "image/jpeg", quality)
		);
	}

	async function upload_blob(blob: Blob, name: string, type: string): Promise<GradioFileData> {
		const client = gradio.shared.client as { upload?: (files: unknown[], root: string) => Promise<(GradioFileData | null)[]> } | undefined;
		if (!client || typeof client.upload !== "function") throw new Error("gradio client upload is unavailable in this component context");
		const file = new File([blob], name, { type });
		const fd = { path: name, orig_name: name, blob: file, size: file.size, mime_type: type, meta: { _type: "gradio.FileData" } };
		const [up] = await client.upload([fd], gradio.shared.root);
		if (!up || !up.path) throw new Error("upload returned no file");
		return { ...up, meta: { _type: "gradio.FileData" } };
	}

	function base_name(name: string): string {
		const i = name.lastIndexOf(".");
		return i > 0 ? name.slice(0, i) : name;
	}

	/** The image flow. Every failure of the WASM path falls back to uploading the ORIGINAL
	 * (path "upload-original", browser_error set) so the app keeps working; only a failed
	 * upload of the original is reported as `error`. */
	async function run_image(file: File, payload: WasmPayload): Promise<void> {
		busy = true;
		const t_start = performance.now();
		const base: Omit<ImageResult, "path" | "file" | "upload_bytes_client"> = {
			orig_name: file.name,
			orig_bytes: file.size,
			orig_type: file.type,
			python_calls: 0,
			wasm_supported: typeof WebAssembly === "object",
			user_agent: navigator.userAgent
		};
		let browser_error: string | null = null;
		try {
			if (!payload.wasm || !payload.scale_wasm) throw new Error("no usable artifact (fallback)");
			const t0 = performance.now();
			const dec = await decode_in_tab(file);
			const t1 = performance.now();
			const scale_kernel = await with_timeout(ffi.instantiate(payload.scale_wasm), RUN_TIMEOUT_MS);
			const scale = Number(
				ffi.call(scale_kernel, payload.scale_fn as string, payload.scale_param_types as string[], [dec.w, dec.h, payload.max_dim as number], { returnType: "int" }).value
			);
			const t2 = performance.now();
			if (!(scale >= 1)) throw new Error(`box_scale returned ${scale}`);
			const ow = Math.floor(dec.w / scale);
			const oh = Math.floor(dec.h / scale);
			// pack RGBA -> 0xRRGGBB int32 (the kernel's input format)
			const px = new Int32Array(dec.w * dec.h);
			const d = dec.data;
			for (let i = 0, j = 0; i < px.length; i++, j += 4) px[i] = (d[j] << 16) | (d[j + 1] << 8) | d[j + 2];
			const t3 = performance.now();
			const kernel = await with_timeout(ffi.instantiate(payload.wasm), RUN_TIMEOUT_MS);
			const t4 = performance.now();
			const out = new Int32Array(ow * oh);
			const r = ffi.call(kernel, payload.fn, payload.param_types as string[], [px, dec.w, dec.h, scale, out], {
				readBack: [4],
				returnType: payload.return_type ?? "int"
			});
			const t5 = performance.now();
			if (Number(r.value) !== ow * oh) throw new Error(`kernel returned ${String(r.value)} output pixels, expected ${ow * oh}`);
			const res = r.outs[4] as Int32Array;
			// unpack -> RGBA for the canvas + RGB bytes for the checksum
			const rgba = new Uint8ClampedArray(ow * oh * 4);
			const rgb = new Uint8Array(ow * oh * 3);
			for (let i = 0, j = 0, k = 0; i < res.length; i++, j += 4, k += 3) {
				const v = res[i];
				const rr = (v >> 16) & 255, gg = (v >> 8) & 255, bb = v & 255;
				rgba[j] = rr; rgba[j + 1] = gg; rgba[j + 2] = bb; rgba[j + 3] = 255;
				rgb[k] = rr; rgb[k + 1] = gg; rgb[k + 2] = bb;
			}
			const checksum = crc32(rgb);
			const t6 = performance.now();
			const quality = payload.quality ?? 0.85;
			const blob = await encode_in_tab(rgba, ow, oh, quality);
			const t7 = performance.now();
			const name = `${base_name(file.name)}.${ow}x${oh}.q${Math.round(quality * 100)}.jpg`;
			const up = await upload_blob(blob, name, "image/jpeg");
			const t8 = performance.now();
			if (!still_current(payload)) {
				console.warn(`pythscribe: result for nonce ${payload.nonce} dropped: a newer dispatch superseded it after the upload`);
				return;
			}
			const result: ImageResult = {
				...base,
				path: "browser-wasm",
				file: up,
				upload_bytes_client: blob.size,
				in_w: dec.w, in_h: dec.h, scale, out_w: ow, out_h: oh, checksum,
				decode_ms: t1 - t0, scale_ms: t2 - t1, pack_ms: t3 - t2, instantiate_ms: t4 - t3,
				wasm_call_ms: t5 - t4, unpack_ms: t6 - t5, encode_ms: t7 - t6, upload_ms: t8 - t7,
				total_ms: t8 - t_start, heap_bytes: r.heapBytes,
				wasm_exports: kernel.exportNames, wasm_how: kernel.how, wasm_fetched: wasm_fetched(t_start),
				layout_version: ffi.LAYOUT_VERSION, // the list layout this bundle marshalled with (provenance)
				browser_error: null
			};
			gradio.props.value = { ...payload, error: null, result };
			return;
		} catch (e) {
			browser_error = String(e && (e as Error).message ? (e as Error).message : e);
		}
		// fallback: upload the ORIGINAL, the server resizes in Python
		try {
			const t0 = performance.now();
			const up = await upload_blob(file, file.name, file.type || "application/octet-stream");
			if (!still_current(payload)) {
				console.warn(`pythscribe: fallback result for nonce ${payload.nonce} dropped: a newer dispatch superseded it`);
				return;
			}
			const result: ImageResult = {
				...base,
				path: "upload-original",
				file: up,
				upload_bytes_client: file.size,
				upload_ms: performance.now() - t0,
				total_ms: performance.now() - t_start,
				wasm_fetched: wasm_fetched(t_start),
				browser_error
			};
			gradio.props.value = { ...payload, error: null, result };
		} catch (e) {
			if (!still_current(payload)) return;
			gradio.props.value = { ...payload, result: null, error: `${browser_error ? browser_error + "; then " : ""}upload failed: ${String(e)}` };
		} finally {
			busy = false;
		}
	}

	function on_file(ev: Event): void {
		const input = ev.currentTarget as HTMLInputElement;
		const file = input.files && input.files[0];
		const v = gradio.props.value as WasmPayload | null;
		if (!file || !v || v.kind !== "image" || busy) return;
		void run_image(file, $state.snapshot(v) as WasmPayload).finally(() => { busy = false; });
	}

	$effect(() => {
		const v = gradio.props.value;
		if (!v || typeof v !== "object") return;
		if (v.kind === "image") {
			// a fresh dispatch (new nonce) re-arms the picker; nothing runs until a file is chosen
			if (v.nonce !== last_nonce_seen) {
				last_nonce_seen = v.nonce;
				if (file_input && !v.result) file_input.value = "";
			}
			return;
		}
		if (v.kind === "flowgraph") return; // handled by the flowgraph island effect below (explicit ignore, S8)
		if (v.result || v.error || !v.bundle || !v.fn) return;
		const key = JSON.stringify([v.bundle, v.fn, v.args, v.nonce ?? 0]);
		if (key === last_key) return;
		last_key = key;
		void run($state.snapshot(v) as WasmPayload);
	});

	// v0.2.5 callback path: mount the React/ReactFlow island via a LAZY import() (so react + xyflow
	// land in a SEPARATE chunk, never in the image/scalar users' index.js). No write-back to
	// gradio.props.value on this path (compute stays in the tab; no `change`, no round-trip) -- the
	// island reports only a client marker, and python_calls is derived server-side.
	$effect(() => {
		const v = gradio.props.value as any;
		if (!v || typeof v !== "object" || v.kind !== "flowgraph" || !flow_el) return;
		const el = flow_el;
		const snap = $state.snapshot(v);
		let cancelled = false;
		import("./flow_island").then((mod) => {
			if (cancelled) return;
			flow_island = mod;
			mod.mountFlowIsland(el, snap as any, {
				onMarker: (m) => {
					flow_marker = { path: m.path, ready: m.ready };
					(window as any).__pythscribeFlow = m;
				}
			});
		});
		return () => {
			cancelled = true;
			if (flow_island) flow_island.unmountFlowIsland(el);
		};
	});
</script>

<Block
	visible={gradio.shared.visible}
	elem_id={gradio.shared.elem_id}
	elem_classes={gradio.shared.elem_classes}
	container={true}
	scale={gradio.shared.scale}
	min_width={gradio.shared.min_width}
>
	{#if gradio.shared.loading_status}
		<StatusTracker
			autoscroll={gradio.shared.autoscroll}
			i18n={gradio.i18n}
			{...gradio.shared.loading_status}
			on_clear_status={() => gradio.dispatch("clear_status", gradio.shared.loading_status)}
		/>
	{/if}
	<BlockTitle show_label={gradio.shared.show_label} info={undefined}>{gradio.shared.label}</BlockTitle>
	{#if gradio.props.value && gradio.props.value.kind === "image"}
		<input
			type="file"
			accept="image/*"
			class="wasm-file"
			data-testid="wasm-file"
			disabled={busy}
			bind:this={file_input}
			onchange={on_file}
		/>
	{/if}
	{#if gradio.props.value && gradio.props.value.kind === "flowgraph"}
		<div
			class="wasm-flow"
			data-testid="wasm-flow"
			data-flow-path={flow_marker ? flow_marker.path : ""}
			data-flow-ready={flow_marker && flow_marker.ready ? "1" : "0"}
			bind:this={flow_el}
		></div>
	{/if}
	<div class="wasm-status" data-testid="wasm-status">{status}</div>
	<pre class="wasm-payload" data-testid="wasm-payload">{JSON.stringify(gradio.props.value, null, 2)}</pre>
</Block>

<style>
	.wasm-file {
		display: block;
		margin-bottom: var(--spacing-sm);
	}
	.wasm-status {
		font-size: var(--text-sm);
		color: var(--body-text-color-subdued);
		margin-bottom: var(--spacing-sm);
	}
	.wasm-payload {
		font-family: var(--font-mono);
		font-size: var(--text-xs);
		max-height: 16rem;
		overflow: auto;
		white-space: pre-wrap;
	}
	.wasm-flow {
		width: 100%;
		height: 340px;
		margin-bottom: var(--spacing-sm);
		border: 1px solid var(--border-color-primary);
		border-radius: var(--radius-sm);
	}
	/* the callback island's node chrome (the ReactFlow node POSITIONING comes from
	   @xyflow/react/dist/style.css, folded into this component's single style.css) */
	:global(.pyths-flow-node) {
		padding: 6px 10px;
		background: var(--background-fill-primary, #fff);
		border: 1px solid var(--border-color-primary, #ccc);
		border-radius: 6px;
		font-size: var(--text-sm);
		min-width: 90px;
	}
	:global(.pyths-flow-label) { color: var(--body-text-color-subdued, #666); font-size: var(--text-xs); }
	:global(.pyths-flow-value) { font-family: var(--font-mono); font-weight: 600; }
	:global(.pyths-flow-error) { color: var(--error-text-color, #b00); font-size: var(--text-xs); }
</style>
