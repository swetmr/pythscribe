// v0.2.5 callback path (M0) -- the React + @xyflow/react ISLAND mounted inside the Svelte-5 Gradio
// component. This module is loaded via a LAZY `import("./flow_island")` from Index.svelte, so Vite
// emits it (react + react-dom + @xyflow/react) as a SEPARATE chunk: the non-callback (image/scalar)
// WasmFunction users never pay for it (the `index.js` <= 5 KB delta budget, B1/S-r2-7).
//
// The "wrinkle" this proves (req §4a-a): a React island renders inside a Svelte bundle AND a
// Python `@wasm` kernel, instantiated ONCE (async), is called SYNCHRONOUSLY from a ReactFlow custom
// node's compute path. The async-instantiate-vs-sync-call seam is handled explicitly: nodes show a
// "loading" state until every kernel is ready; the kernel is NEVER called before its module exists
// (never a crash/NaN), and a non-finite result is an ERROR STATE, never a rendered value (S-r2-1).
//
// NB: no JSX -- `h = createElement` -- so the build does not depend on a JSX-runtime transform
// (an M0 failure mode, S4). NO `window`-touching at module top level (kept inside functions, B1).
import "./_process_shim"; // MUST be first: defines globalThis.process for react-dom/xyflow prod checks
import { createElement as h, useEffect, useMemo, useState } from "react";
import { createRoot, type Root } from "react-dom/client";
import { ReactFlow, Background, Handle, Position } from "@xyflow/react";
import type { Node, Edge, NodeProps } from "@xyflow/react";
// folded into the single templates/component/style.css (lib mode cssCodeSplit=false); without it a
// `.react-flow__node` mounts but `position` computes `static` -> the graph is a blank box (B1-iv).
import "@xyflow/react/dist/style.css";
import * as ffi from "./list_buffer.mjs";
// B2: `spec.wasm` is the HOST-ATTACHED transport (client_callback ships wasm=null + a neutral
// wasm_path; the Gradio FlowGraph.postprocess resolves it to a served /gradio_api/file=... URL
// before the payload reaches us). The island reads only `spec.wasm`, transport-agnostic.
// SF-3: the instantiate-once cache is factored into ./kernel_cache.mjs (react-free, Node-testable)
// and evicts a REJECTED load so a remount retries instead of a poisoned cache.
import { instantiateOnce, kernelKey } from "./kernel_cache.mjs";
import type { CallbackSpecPayload, FlowGraphPayload, FlowNodePayload } from "./types";

type Scalar = number | bigint;
interface KernelHandle {
	instance: unknown;
	exports: Record<string, unknown>;
	exportNames: string[];
	how: string;
}

// --- the synchronous compute: a once-instantiated kernel called like any JS function -------------
// A non-finite result (NaN/+-inf) is an error state, never a value (S-r2-1); a legitimate large
// int crosses as a BigInt (renderable). Throws on a kernel error (B3 __err_code / __ovf) too.
function computeSync(kernel: KernelHandle, spec: CallbackSpecPayload, args: Scalar[]): Scalar {
	const r = ffi.call(kernel, spec.fn, spec.param_types, args, { returnType: spec.return_type }).value as Scalar;
	if (!ffi.isRenderableScalar(r)) throw new Error(`${spec.fn}: non-finite result (${String(r)}) — value out of domain`);
	return r;
}

const POISON = Symbol("poison");
type NodeVal = Scalar | typeof POISON;

/** Evaluate the whole graph synchronously in topological order once every kernel is ready. A node
 * whose kernel throws OR returns non-finite is POISON; every downstream node is POISON too (never
 * recomputed from a stale value -- B5a). Cycles -> every node on the cycle is POISON (never a hang). */
function evalGraph(
	payload: FlowGraphPayload,
	kernels: Map<string, KernelHandle>
): { values: Map<string, NodeVal>; errors: Map<string, string> } {
	const byId = new Map<string, FlowNodePayload>();
	for (const n of payload.nodes) byId.set(n.id, n);
	const inputs = new Map<string, string[]>(); // target -> [source ids] in edge order
	for (const [from, to] of payload.edges) {
		if (!inputs.has(to)) inputs.set(to, []);
		inputs.get(to)!.push(from);
	}
	const values = new Map<string, NodeVal>();
	const errors = new Map<string, string>();
	const state = new Map<string, 0 | 1 | 2>(); // 0 unseen, 1 in-progress (cycle guard), 2 done

	function evalNode(id: string): NodeVal {
		if (state.get(id) === 2) return values.get(id)!;
		if (state.get(id) === 1) { errors.set(id, "cycle detected"); values.set(id, POISON); state.set(id, 2); return POISON; }
		state.set(id, 1);
		const node = byId.get(id);
		let out: NodeVal;
		if (!node) { out = POISON; errors.set(id, "unknown node"); }
		else if (node.type === "source") {
			out = typeof node.value === "number" ? node.value : POISON;
			if (out === POISON) errors.set(id, "source has no value");
		} else {
			const spec = node.kernel;
			const ins = (inputs.get(id) ?? []).map(evalNode);
			if (!spec) { out = POISON; errors.set(id, "compute node has no kernel"); }
			else if (ins.some((v) => v === POISON)) { out = POISON; errors.set(id, "upstream error"); }
			else {
				const k = kernels.get(kernelKey(spec));
				if (!k) { out = POISON; errors.set(id, "kernel not ready"); }
				else {
					try { out = computeSync(k, spec, ins as Scalar[]); }
					catch (e) { out = POISON; errors.set(id, e instanceof Error ? e.message : String(e)); }
				}
			}
		}
		values.set(id, out);
		state.set(id, 2);
		return out;
	}
	for (const n of payload.nodes) evalNode(n.id);
	return { values, errors };
}

function fmtVal(v: NodeVal | undefined): string {
	if (v === undefined) return "loading";
	if (v === POISON) return "error";
	return typeof v === "bigint" ? v.toString() : String(v);
}

// --- the ReactFlow custom nodes -----------------------------------------------------------------
interface NData extends Record<string, unknown> { label: string; display: string; kind: "source" | "compute"; error?: string }

function ComputeNode(props: NodeProps): unknown {
	const d = props.data as unknown as NData;
	return h("div", { className: "pyths-flow-node pyths-compute", "data-testid": "flow-compute-node", "data-value": d.display },
		h(Handle, { type: "target", position: Position.Left }),
		h("div", { className: "pyths-flow-label" }, d.label),
		h("div", { className: "pyths-flow-value", "data-testid": "flow-node-value" }, d.display),
		d.error ? h("div", { className: "pyths-flow-error", "data-testid": "flow-node-error" }, d.error) : null,
		h(Handle, { type: "source", position: Position.Right })
	);
}
function SourceNode(props: NodeProps): unknown {
	const d = props.data as unknown as NData;
	return h("div", { className: "pyths-flow-node pyths-source", "data-testid": "flow-source-node" },
		h("div", { className: "pyths-flow-label" }, d.label),
		h("div", { className: "pyths-flow-value" }, d.display),
		h(Handle, { type: "source", position: Position.Right })
	);
}
const NODE_TYPES = { compute: ComputeNode, source: SourceNode };

// SF-6: the marker carries an explicit `error` PATH (+ message) on a failed load, so a blocked/failed
// `.wasm` fetch is distinguishable from "still loading" (it otherwise stayed path:"loading" forever).
export interface MarkerCb { (m: { path: string; ready: boolean; values: Record<string, string>; error?: string }): void }

function FlowApp(props: { payload: FlowGraphPayload; onMarker?: MarkerCb }): unknown {
	const { payload, onMarker } = props;
	const [kernels, setKernels] = useState<Map<string, KernelHandle> | null>(null);
	const [loadError, setLoadError] = useState<string | null>(null);

	// distinct kernels across the graph (dedup per key)
	const specs = useMemo(() => {
		const m = new Map<string, CallbackSpecPayload>();
		for (const n of payload.nodes) if (n.type === "compute" && n.kernel) m.set(kernelKey(n.kernel), n.kernel);
		return m;
	}, [payload]);

	// graph-level readiness (S1 i/ii): instantiate ALL distinct kernels, THEN a full evaluation.
	useEffect(() => {
		let alive = true;
		Promise.all([...specs.values()].map((s) => instantiateOnce(s).then((k) => [kernelKey(s), k] as const)))
			// SF-C: a later SUCCESSFUL load clears a previous transient error within the SAME React root
			// (the island is correct on its own, not by courtesy of the Svelte host's remount).
			.then((pairs) => { if (alive) { setKernels(new Map(pairs)); setLoadError(null); } })
			.catch((e) => { if (alive) setLoadError(e instanceof Error ? e.message : String(e)); });
		return () => { alive = false; };
	}, [specs]);

	const evald = useMemo(() => (kernels ? evalGraph(payload, kernels) : null), [payload, kernels]);

	// publish the marker (path=browser-wasm once anything computed in-tab; server derives
	// python_calls). SF-6: a load failure publishes path="error" + the message (never a stuck
	// "loading"), so the E2E and the Svelte status can tell "failed" from "still loading".
	useEffect(() => {
		if (!onMarker) return;
		const values: Record<string, string> = {};
		if (evald) for (const [id, v] of evald.values) values[id] = fmtVal(v);
		const path = loadError ? "error" : kernels ? "browser-wasm" : "loading";
		onMarker({ path, ready: !!kernels, values, error: loadError ?? undefined });
	}, [evald, kernels, loadError, onMarker]);

	const rf = useMemo(() => {
		const cols = new Map<string, number>();
		const nodes: Node[] = payload.nodes.map((n, i) => {
			// simple deterministic layout: sources on the left, computes to the right
			const col = n.type === "source" ? 0 : 1;
			const row = cols.get(String(col)) ?? 0;
			cols.set(String(col), row + 1);
			const v = evald ? evald.values.get(n.id) : undefined;
			const err = evald ? evald.errors.get(n.id) : undefined;
			return {
				id: n.id,
				type: n.type,
				position: { x: col * 220 + 20, y: row * 90 + 20 },
				data: { label: n.label ?? n.id, display: loadError ? "error" : fmtVal(v), kind: n.type, error: loadError ?? err } as NData,
			} as Node;
		});
		const edges: Edge[] = payload.edges.map(([s, t], i) => ({ id: `e${i}-${s}-${t}`, source: s, target: t }));
		return { nodes, edges };
	}, [payload, evald, loadError]);

	return h("div", { className: "pyths-flow-root", "data-testid": "flow-root", "data-ready": kernels ? "1" : "0", style: { width: "100%", height: "320px" } },
		h(ReactFlow, { nodes: rf.nodes, edges: rf.edges, nodeTypes: NODE_TYPES, fitView: true, proOptions: { hideAttribution: true } },
			h(Background, {})
		)
	);
}

// --- the mount/unmount API the Svelte component calls -------------------------------------------
const _roots = new WeakMap<Element, Root>();

export function mountFlowIsland(el: Element, payload: FlowGraphPayload, opts?: { onMarker?: MarkerCb }): void {
	let root = _roots.get(el);
	if (!root) { root = createRoot(el); _roots.set(el, root); }
	root.render(h(FlowApp, { payload, onMarker: opts?.onMarker }));
}

export function unmountFlowIsland(el: Element): void {
	const root = _roots.get(el);
	if (root) { root.unmount(); _roots.delete(el); }
}
