// Tests for pyths-runtime/web — the W* tier runtime.
// Run with: node --test runtime/src/web.test.mjs
//
// Guards:
//  1. handler(fn) returns { fetch: fn } — a valid Cloudflare Worker entry.
//  2. Response is the real Web Response constructor (not undefined).
//  3. DOM-free — web.js contains zero references to document/window.
//
// Fix: B-029 — pyths-runtime/web subpath was missing handler + Response.

import { test } from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { dirname, join } from "node:path";

import { handler, Response } from "./web.js";

// ── 1. handler ─────────────────────────────────────────────────────────────

test("handler(fn) returns { fetch: fn } — valid Worker entry shape", () => {
    async function my_fetch(request) {
        return new Response("hello");
    }
    const entry = handler(my_fetch);
    assert.ok(typeof entry === "object", "entry is an object");
    assert.strictEqual(entry.fetch, my_fetch, "entry.fetch === original fn");
});

test("handler: decorator form — handler(handler(fn)) nests (documents decorator semantics)", () => {
    // @handler on a fetch function assigns my_fetch = handler(my_fetch) = { fetch: fn }.
    // The resulting value is NOT a function, so a double-decoration would wrap the object,
    // not the inner function. This test documents that the supported form is
    // __default__ = handler(fn), not double-decoration.
    function fn(req) { return req; }
    const entry = handler(fn);
    assert.strictEqual(typeof entry, "object");
    assert.strictEqual(entry.fetch, fn);
    // Double-decoration wraps the entry object (not a useful pattern; this is intentional doc).
    const double = handler(entry);
    assert.strictEqual(double.fetch, entry);
});

test("handler: the entry's fetch fn is callable", async () => {
    async function my_fetch(_request) {
        return new Response("ok", { status: 200 });
    }
    const entry = handler(my_fetch);
    const res = await entry.fetch(null);
    assert.strictEqual(res.status, 200);
    assert.strictEqual(await res.text(), "ok");
});

// ── 2. Response ────────────────────────────────────────────────────────────

test("Response is the real Web Response constructor (not undefined)", () => {
    assert.ok(Response !== undefined, "Response is exported");
    assert.strictEqual(typeof Response, "function", "Response is a constructor");
});

test("Response is globalThis.Response (same reference)", () => {
    assert.strictEqual(Response, globalThis.Response,
        "pyths-runtime/web re-exports globalThis.Response unchanged");
});

test("Response(body, init) constructs a real response", async () => {
    const r = new Response("hello world", {
        status: 201,
        headers: { "content-type": "text/plain" },
    });
    assert.strictEqual(r.status, 201);
    assert.strictEqual(await r.text(), "hello world");
});

test("Response() with no body constructs with status 200", async () => {
    const r = new Response();
    assert.strictEqual(r.status, 200);
});

// ── 3. DOM-free static check ───────────────────────────────────────────────
// Verifies that web.js contains zero references to document/window globals.

test("web.js: no document/window property accesses (DOM-free)", () => {
    const __dirname = dirname(fileURLToPath(import.meta.url));
    const src = readFileSync(join(__dirname, "web.js"), "utf8");

    const forbidden = [
        { pattern: /\bdocument\.\w/, label: "document global property access" },
        { pattern: /\bwindow\.\w/, label: "window global property access" },
    ];

    for (const { pattern, label } of forbidden) {
        assert.ok(
            !pattern.test(src),
            `B-029: web.js must not contain '${label}' (Worker-safe)`
        );
    }
});
