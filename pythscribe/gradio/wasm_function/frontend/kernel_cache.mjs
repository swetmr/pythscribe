// v0.2.5 callback path -- the instantiate-once, de-duplicated kernel cache, factored OUT of
// flow_island.tsx so it carries NO react/react-dom/xyflow/CSS imports and can be unit-tested
// directly under Node (the SF-3 control). `list_buffer.mjs` is pure and Node-safe, so this whole
// module imports in Node without a DOM.
//
// ONE authority for kernel instantiation across the island (dedup per (source_sha256, served url)).
// React StrictMode / a Svelte re-mount fires the mount effect twice; this cache keeps it ONE
// fetch+compile per distinct kernel.
import * as ffi from "./list_buffer.mjs";

/** @typedef {{ fn: string, wasm: string|null, param_types: string[], return_type: string, shape: string, source_sha256: string }} CallbackSpecPayload */

/** module-level cache: key -> Promise<KernelHandle>. Exported for the SF-3 control only. */
export const _kernelCache = new Map();

/** dedup key: a kernel is the same iff BOTH its source sha AND its served url match. */
export function kernelKey(spec) {
	return `${spec.source_sha256}@@${spec.wasm ?? ""}`;
}

/** Instantiate a kernel ONCE, returning the cached promise on a repeat mount.
 *
 * SF-3: a REJECTED load MUST NOT poison the cache forever. Before this, a single transient `.wasm`
 * fetch failure (server restarting / a 5xx) cached a rejected promise, so every later remount
 * returned that same rejection -> "error forever until a page reload". We evict the entry the moment
 * its promise rejects (guarding against clobbering a newer entry under the same key), so the NEXT
 * mount retries the fetch. Successful loads stay cached (the dedup the fast path relies on). */
export function instantiateOnce(spec) {
	const key = kernelKey(spec);
	let p = _kernelCache.get(key);
	if (!p) {
		if (!spec.wasm) {
			p = Promise.reject(new Error(`kernel ${spec.fn}: no usable artifact (.wasm)`));
		} else {
			p = ffi.instantiate(ffi.resolveUrl(spec.wasm));
		}
		// evict on rejection so a remount RETRIES (never a poisoned cache). The identity guard means
		// a fresh entry stored under the same key by a later mount is not deleted by this stale one.
		p.catch(() => {
			if (_kernelCache.get(key) === p) _kernelCache.delete(key);
		});
		_kernelCache.set(key, p);
	}
	return p;
}

/** Test-only: clear the cache between control cases. */
export function _resetKernelCacheForTest() {
	_kernelCache.clear();
}
