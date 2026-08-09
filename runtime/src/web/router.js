// PythScribe web module: router
// Simple client-side routing for single-page applications

let _routes = [];
let _notFound = null;
let _currentPath = null;
let _onNavigate = null;

export function route(path, handler) {
    _routes.push({ path, handler, pattern: _compilePattern(path) });
}

export function not_found(handler) {
    _notFound = handler;
}

export function on_navigate(callback) {
    _onNavigate = callback;
}

export function navigate(path, { replace = false } = {}) {
    if (replace) {
        history.replaceState(null, "", path);
    } else {
        history.pushState(null, "", path);
    }
    _dispatch(path);
}

export function current_path() {
    return window.location.pathname;
}

export function query_params() {
    return Object.fromEntries(new URLSearchParams(window.location.search));
}

export function start() {
    window.addEventListener("popstate", () => {
        _dispatch(window.location.pathname);
    });

    // Intercept link clicks for SPA navigation
    document.addEventListener("click", (e) => {
        const anchor = e.target.closest("a");
        if (anchor && anchor.href && anchor.origin === window.location.origin && !anchor.hasAttribute("data-external")) {
            e.preventDefault();
            navigate(anchor.pathname + anchor.search + anchor.hash);
        }
    });

    _dispatch(window.location.pathname);
}

function _dispatch(path) {
    _currentPath = path;
    for (const r of _routes) {
        const match = r.pattern.exec(path);
        if (match) {
            const params = match.groups || {};
            if (_onNavigate) _onNavigate(path, params);
            r.handler(params);
            return;
        }
    }
    if (_notFound) {
        if (_onNavigate) _onNavigate(path, {});
        _notFound();
    }
}

function _compilePattern(path) {
    // Convert route patterns like "/users/:id" or "/posts/<slug>" to regex
    const escaped = path
        .replace(/[.*+?^${}()|[\]\\]/g, "\\$&")
        .replace(/:(\w+)/g, "(?<$1>[^/]+)")
        .replace(/<(\w+)>/g, "(?<$1>[^/]+)");
    return new RegExp(`^${escaped}$`);
}

export class Router {
    constructor() {
        this._routes = [];
        this._notFound = null;
    }

    route(path, handler) {
        this._routes.push({ path, handler, pattern: _compilePattern(path) });
        return this;
    }

    not_found(handler) {
        this._notFound = handler;
        return this;
    }

    match(path) {
        for (const r of this._routes) {
            const m = r.pattern.exec(path);
            if (m) return { handler: r.handler, params: m.groups || {} };
        }
        return this._notFound ? { handler: this._notFound, params: {} } : null;
    }
}
