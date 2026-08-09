// PythScribe web module: fetch
// Provides Pythonic wrappers around the Fetch API

export async function get(url, { headers, params, timeout } = {}) {
    if (params) {
        const qs = new URLSearchParams(params).toString();
        url = `${url}?${qs}`;
    }
    const opts = { method: "GET", headers };
    return _fetch(url, opts, timeout);
}

export async function post(url, { data, json, body, headers, timeout } = {}) {
    headers = { ...headers };
    let payload;
    if (json !== undefined) {
        payload = JSON.stringify(json);
        headers["Content-Type"] = headers["Content-Type"] || "application/json";
    } else if (data !== undefined) {
        payload = data;
    } else if (body !== undefined) {
        // raw body (JS `fetch` style); used verbatim, no Content-Type added.
        payload = body;
    }
    return _fetch(url, { method: "POST", headers, body: payload }, timeout);
}

export async function put(url, { data, json, body, headers, timeout } = {}) {
    headers = { ...headers };
    let payload;
    if (json !== undefined) {
        payload = JSON.stringify(json);
        headers["Content-Type"] = headers["Content-Type"] || "application/json";
    } else if (data !== undefined) {
        payload = data;
    } else if (body !== undefined) {
        // raw body (JS `fetch` style); used verbatim, no Content-Type added.
        payload = body;
    }
    return _fetch(url, { method: "PUT", headers, body: payload }, timeout);
}

export async function patch(url, { data, json, body, headers, timeout } = {}) {
    headers = { ...headers };
    let payload;
    if (json !== undefined) {
        payload = JSON.stringify(json);
        headers["Content-Type"] = headers["Content-Type"] || "application/json";
    } else if (data !== undefined) {
        payload = data;
    } else if (body !== undefined) {
        // raw body (JS `fetch` style); used verbatim, no Content-Type added.
        payload = body;
    }
    return _fetch(url, { method: "PATCH", headers, body: payload }, timeout);
}

export async function delete_(url, { headers, timeout } = {}) {
    return _fetch(url, { method: "DELETE", headers }, timeout);
}

export async function head(url, { headers, timeout } = {}) {
    return _fetch(url, { method: "HEAD", headers }, timeout);
}

class Response {
    constructor(response, _body) {
        this.status = response.status;
        this.status_code = response.status;
        this.ok = response.ok;
        this.headers = Object.fromEntries(response.headers.entries());
        this.url = response.url;
        this._response = response;
        this._body = _body;
    }

    async text() {
        if (this._body !== undefined) return this._body;
        this._body = await this._response.text();
        return this._body;
    }

    async json() {
        const text = await this.text();
        return JSON.parse(text);
    }

    async blob() {
        return this._response.blob();
    }

    async array_buffer() {
        return this._response.arrayBuffer();
    }

    raise_for_status() {
        if (!this.ok) {
            throw new Error(`HTTP Error ${this.status}: ${this.url}`);
        }
    }
}

async function _fetch(url, opts, timeout) {
    let controller;
    let timeoutId;
    if (timeout) {
        controller = new AbortController();
        opts.signal = controller.signal;
        timeoutId = setTimeout(() => controller.abort(), timeout * 1000);
    }
    try {
        const res = await fetch(url, opts);
        return new Response(res);
    } finally {
        if (timeoutId) clearTimeout(timeoutId);
    }
}
