// PythScribe Runtime — Web (Worker-safe) subpath
// ================================================
// Exposes `handler` + `Response` for the W* tier preset.
//
// W* expands to:  from pyths.web import handler, Response
// which compiles: import { handler, Response } from "pyths-runtime/web"
//
// Contract:
//   Response  — the standard Web API Response constructor (available in
//               Cloudflare Workers, Deno, and Node ≥ 18). Re-exported as-is.
//               Usage: Response("body", { status: 200, headers: {...} })
//
//   handler(fn) — wraps a fetch function into the Cloudflare Worker default-export
//                 entry shape { fetch: fn }.  Usable as:
//                   __default__ = handler(my_fetch)     # compiles fine today
//                   @handler                            # decorator form
//                   def my_fetch(request): ...          #   → my_fetch = handler(my_fetch) = { fetch: ... }
//                 Note: the decorator form produces the Worker entry object as
//                 the name's value; if codegen later adds @handler→__default__
//                 auto-wiring, this runtime implementation needs no changes.
//
// DOM-free — Workers have no DOM; zero references to document/window here.
//
// Fix: B-029 — pyths-runtime/web subpath was missing handler + Response.
//
// Design follow-up (NOT in this fix): a @handler decorator that auto-wires
// the module's default export via codegen (so users write @handler on a
// fetch fn and pyths emits `export default { fetch: my_fetch }` without
// needing __default__ = handler(...)). Tracked as a codegen enhancement.

// ── Response ──────────────────────────────────────────────────────────────
// Re-export the standard Web Response.  Cloudflare Workers, Deno, and
// Node ≥ 18 all provide it on `globalThis`.  The guard surfaces a clear
// error in older / stripped environments rather than a cryptic
// "undefined is not a constructor" at call time.

if (typeof globalThis.Response === "undefined") {
    throw new Error(
        "pyths-runtime/web: globalThis.Response is not available. " +
        "Upgrade to Node ≥ 18, Deno, or a Cloudflare Worker environment."
    );
}

export const Response = globalThis.Response;

// ── handler ───────────────────────────────────────────────────────────────
// Returns the Cloudflare Worker default-export entry shape { fetch: fn }.
// Works both as a plain call and as a decorator:
//
//   Plain:      __default__ = handler(my_fetch)
//   Decorator:  @handler
//               def my_fetch(request): ...
//               # equivalent to: my_fetch = handler(my_fetch)
//               # value of my_fetch is now { fetch: <original fn> }

export function handler(fn) {
    return { fetch: fn };
}

//# sourceMappingURL=web.js.map
