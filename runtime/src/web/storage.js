// PythScribe web module: storage
// Provides Pythonic wrappers around localStorage and sessionStorage

class Storage {
    constructor(backend) {
        this._backend = backend;
    }

    get(key, default_value = null) {
        const value = this._backend.getItem(key);
        if (value === null) return default_value;
        try {
            return JSON.parse(value);
        } catch {
            return value;
        }
    }

    set(key, value) {
        this._backend.setItem(key, JSON.stringify(value));
    }

    delete(key) {
        this._backend.removeItem(key);
    }

    has(key) {
        return this._backend.getItem(key) !== null;
    }

    clear() {
        this._backend.clear();
    }

    keys() {
        const keys = [];
        for (let i = 0; i < this._backend.length; i++) {
            keys.push(this._backend.key(i));
        }
        return keys;
    }

    values() {
        return this.keys().map(k => this.get(k));
    }

    items() {
        return this.keys().map(k => [k, this.get(k)]);
    }

    get length() {
        return this._backend.length;
    }

    *[Symbol.iterator]() {
        for (const key of this.keys()) {
            yield [key, this.get(key)];
        }
    }
}

export const local = typeof localStorage !== "undefined" ? new Storage(localStorage) : null;
export const session = typeof sessionStorage !== "undefined" ? new Storage(sessionStorage) : null;

// ── Cookie grammar validation (SEC-8, CWE-116) ─────────────────────────────
//
// `document.cookie = ...` is a SERIALIZED grammar, not a structured API: ";"
// separates the cookie from its attributes and "=" separates each name from
// its value. Encoding only the VALUE (as this helper used to) leaves `name`,
// `path`, and `same_site` as raw injection points — an application that
// forwards remote input into any of them lets an attacker append attributes
// (`Domain=`, `Max-Age=`, `Secure`), retarget the cookie, or emit CR/LF that
// splits headers in any server-side reuse of the same serializer.
//
// So validate each structured field against its actual grammar and REFUSE
// (fail closed) rather than silently sanitizing — a caller that hands us a
// delimiter has a bug or is under attack, and both deserve to be loud.
// See `experiments/codex-security-scan/poc/D-8.md` for the reproducer.

// Local ValueError shape: the codegen matches `except` clauses on
// `e.name === "ValueError" || e instanceof ValueError`, so name-tagging makes
// this catchable from PythScribe without pulling the whole core runtime into
// the lean `pyths-runtime/web` subpath.
function _valueError(msg) {
    const e = new Error(msg);
    e.name = "ValueError";
    return e;
}

// RFC 6265 cookie-name = RFC 2616 token: any CHAR except CTLs and separators
//   ( ) < > @ , ; : \ " / [ ] ? = { } SP HT
const _COOKIE_TOKEN = /^[A-Za-z0-9!#$%&'*+\-.^_`|~]+$/;

// RFC 6265 path-value = any CHAR except CTLs and ";"
// (CTLs = U+0000-U+001F and U+007F — CR/LF header splitting rides in
// on these, so they are rejected as firmly as the ";" delimiter.)
const _PATH_FORBIDDEN = /[;\u0000-\u001F\u007F]/;

const _SAME_SITE = ["Strict", "Lax", "None"];

function _cookieName(name) {
    if (typeof name !== "string" || !_COOKIE_TOKEN.test(name)) {
        throw _valueError(
            `invalid cookie name ${JSON.stringify(String(name))}: `
            + "must be a non-empty RFC 6265 token (no separators, spaces, or control characters)",
        );
    }
    return name;
}

function _cookiePath(path) {
    if (typeof path !== "string" || _PATH_FORBIDDEN.test(path)) {
        throw _valueError(
            `invalid cookie path ${JSON.stringify(String(path))}: `
            + "must be a string containing no ';' or control characters",
        );
    }
    return path;
}

function _cookieSameSite(value) {
    const s = String(value);
    const match = _SAME_SITE.find((v) => v.toLowerCase() === s.toLowerCase());
    if (!match) {
        throw _valueError(
            `invalid cookie same_site ${JSON.stringify(s)}: expected one of ${_SAME_SITE.join(", ")}`,
        );
    }
    return match; // normalized casing
}

function _cookieDays(days) {
    const n = Number(days);
    if (!Number.isFinite(n)) {
        throw _valueError(`invalid cookie days ${JSON.stringify(String(days))}: expected a finite number`);
    }
    return n;
}

// Cookie helpers
export const cookies = {
    // Read-only: the name is regex-escaped, so there is no injection sink
    // here and an ill-formed name simply cannot match — stay permissive so
    // `get`/`has` keep returning the default instead of raising.
    get(name, default_value = null) {
        const match = document.cookie.match(new RegExp(`(?:^|; )${String(name).replace(/[.*+?^${}()|[\]\\]/g, "\\$&")}=([^;]*)`));
        return match ? decodeURIComponent(match[1]) : default_value;
    },

    set(name, value, { days, path = "/", secure, same_site = "Lax" } = {}) {
        const parts = [
            `${_cookieName(name)}=${encodeURIComponent(value)}`,
            `path=${_cookiePath(path)}`,
            `SameSite=${_cookieSameSite(same_site)}`,
        ];
        if (days) {
            parts.push(`expires=${new Date(Date.now() + _cookieDays(days) * 864e5).toUTCString()}`);
        }
        if (secure) parts.push("Secure");
        document.cookie = parts.join("; ");
    },

    delete(name, { path = "/" } = {}) {
        document.cookie = [
            `${_cookieName(name)}=`,
            `path=${_cookiePath(path)}`,
            "expires=Thu, 01 Jan 1970 00:00:00 GMT",
        ].join("; ");
    },

    has(name) {
        return this.get(name) !== null;
    }
};

//# sourceMappingURL=storage.js.map
