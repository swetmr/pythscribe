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

// Cookie helpers
export const cookies = {
    get(name, default_value = null) {
        const match = document.cookie.match(new RegExp(`(?:^|; )${name.replace(/[.*+?^${}()|[\]\\]/g, "\\$&")}=([^;]*)`));
        return match ? decodeURIComponent(match[1]) : default_value;
    },

    set(name, value, { days, path = "/", secure, same_site = "Lax" } = {}) {
        let cookie = `${name}=${encodeURIComponent(value)}; path=${path}; SameSite=${same_site}`;
        if (days) {
            const expires = new Date(Date.now() + days * 864e5).toUTCString();
            cookie += `; expires=${expires}`;
        }
        if (secure) cookie += "; Secure";
        document.cookie = cookie;
    },

    delete(name, { path = "/" } = {}) {
        document.cookie = `${name}=; path=${path}; expires=Thu, 01 Jan 1970 00:00:00 GMT`;
    },

    has(name) {
        return this.get(name) !== null;
    }
};
