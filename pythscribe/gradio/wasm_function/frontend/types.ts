import type { ILoadingStatus as LoadingStatus } from "@gradio/statustracker";

/** The result block the component writes back (browser path) or Python writes (fallback). */
export interface WasmResult {
	/** convenience only; null for non-finite results. The AUTHORITATIVE field is `bits`. */
	value: number | null;
	/** little-endian IEEE-754 hex of the result -- what the Python side reconstructs the float from */
	bits: string | null;
	path: "browser-wasm" | "python-fallback";
	/** server-side counter delta; the client-written 0 is a placeholder the server overwrites */
	python_calls: number | null;
	/** resource-timing entries ending in .wasm observed in this tab (browser path only) */
	wasm_fetched?: string[];
	wasm_supported?: boolean;
	user_agent?: string;
	error?: string | null;
}

/** A Gradio file reference as `client.upload` returns it (server-side path + meta tag). */
export interface GradioFileData {
	path: string;
	url?: string;
	orig_name?: string;
	size?: number;
	mime_type?: string;
	meta: { _type: "gradio.FileData" };
}

/** M1: what the component writes back after an image was picked in image mode. */
export interface ImageResult {
	/** browser-wasm: the small JPEG encoded from the WASM `out` buffer was uploaded;
	 *  upload-original: the untouched file was uploaded (no artifact, or the WASM path failed:
	 *  see browser_error) and the server runs the Python fallback. */
	path: "browser-wasm" | "upload-original";
	file: GradioFileData;
	orig_name: string;
	orig_bytes: number;
	orig_type: string;
	/** client-side size of the uploaded blob; the server measures the wire bytes itself */
	upload_bytes_client: number;
	in_w?: number;
	in_h?: number;
	scale?: number;
	out_w?: number;
	out_h?: number;
	/** CRC-32 over the row-major RGB bytes of the WASM output (before the lossy encode) */
	checksum?: number;
	decode_ms?: number;
	scale_ms?: number;
	pack_ms?: number;
	instantiate_ms?: number;
	wasm_call_ms?: number;
	unpack_ms?: number;
	encode_ms?: number;
	upload_ms?: number;
	total_ms?: number;
	heap_bytes?: number;
	/** resolution markers (client-observed; the server adds its own python_calls) */
	wasm_exports?: string[];
	wasm_how?: string;
	wasm_fetched?: string[];
	/** the shim's declared list-layout version the bundle marshalled with */
	layout_version?: string;
	wasm_supported?: boolean;
	user_agent?: string;
	python_calls: number | null;
	browser_error?: string | null;
}

/** v0.2.5 callback path (kind: "flowgraph"): a Python @wasm kernel serialized as a synchronous
 * client-side callback. Plain JSON (the vendored backend never imports pythscribe -- S-r3-2); the
 * pythscribe side builds this via `CallbackSpec.to_payload()`. */
export interface CallbackSpecPayload {
	fn: string;
	/** the HOST-ATTACHED transport: a root-absolute served .wasm url (resolved against
	 * gradio_config.root via ffi.resolveUrl). B2: `client_callback` ships this as `null` (with a
	 * neutral server-side `wasm_path`); the Gradio host resolves it at postprocess before this
	 * payload reaches the client, so the island only ever sees the concrete url (or null = no
	 * usable artifact). The Streamlit host will attach a base64 blob transport instead. */
	wasm: string | null;
	param_types: string[];
	return_type: "float" | "int";
	shape: "node" | "comparator" | "cell";
	source_sha256: string;
}

export interface FlowNodePayload {
	id: string;
	type: "source" | "compute";
	/** source only: the initial scalar value + its declared dtype (for edge typing) */
	value?: number;
	dtype?: "int" | "float";
	/** compute only: the Python @wasm kernel used as this node's synchronous compute callback */
	kernel?: CallbackSpecPayload;
	label?: string;
}

/** kind: "flowgraph" payload -- a live dataflow graph whose compute nodes run in the tab. */
export interface FlowGraphPayload {
	kind: "flowgraph";
	nodes: FlowNodePayload[];
	edges: [string, string][];
	nonce: number;
	result: unknown | null;
	error?: string | null;
}

/** The value the Python side dispatches; the component fills `result` when it ran the bundle. */
export interface WasmPayload {
	/** absent/undefined = M0 scalar mode; "image" = image-preprocessing mode; "flowgraph" = the
	 * v0.2.5 callback path (a React/ReactFlow island; handled by the flowgraph branch, cast to
	 * FlowGraphPayload) */
	kind?: "image" | "flowgraph";
	fn: string;
	args?: unknown[];
	bundle?: string | null;
	source_sha256?: string | null;
	artifact_status?: string;
	nonce: number;
	result: WasmResult | ImageResult | null;
	error?: string | null;
	// image mode
	wasm?: string | null;
	param_types?: string[];
	return_type?: string;
	scale_fn?: string;
	scale_wasm?: string | null;
	scale_param_types?: string[];
	max_dim?: number;
	quality?: number;
}

export interface WasmFunctionProps {
	value: WasmPayload | null;
}

export interface WasmFunctionEvents {
	change: never;
	clear_status: LoadingStatus;
}
