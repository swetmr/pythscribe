// PythScribe Runtime — DOM wrappers
// Provides Pythonic (snake_case) DOM access for browser environments.

/**
 * Query a single DOM element.
 * Equivalent to document.querySelector().
 */
export function query(selector) {
    return document.querySelector(selector);
}

/**
 * Query all matching DOM elements.
 * Returns an array (not NodeList).
 */
export function query_all(selector) {
    return [...document.querySelectorAll(selector)];
}

/**
 * Get a DOM element by its ID.
 * Equivalent to document.getElementById().
 */
export function get_element_by_id(id) {
    return document.getElementById(id);
}

/**
 * Create a new DOM element.
 */
export function create_element(tag, attrs = {}, children = []) {
    const el = document.createElement(tag);
    for (const [key, value] of Object.entries(attrs)) {
        if (key === "class_name" || key === "className") {
            el.className = value;
        } else if (key === "style" && typeof value === "object") {
            Object.assign(el.style, value);
        } else if (key.startsWith("on_") && typeof value === "function") {
            el.addEventListener(key.slice(3).replace(/_/g, ""), value);
        } else {
            el.setAttribute(key, value);
        }
    }
    for (const child of children) {
        if (typeof child === "string") {
            el.appendChild(document.createTextNode(child));
        } else {
            el.appendChild(child);
        }
    }
    return el;
}

/**
 * Add an event listener. Returns a cleanup function.
 */
export function add_event_listener(element, event, handler) {
    element.addEventListener(event, handler);
    return () => element.removeEventListener(event, handler);
}

/**
 * Remove an event listener.
 */
export function remove_event_listener(element, event, handler) {
    element.removeEventListener(event, handler);
}

/**
 * Set text content of an element.
 */
export function set_text(element, text) {
    element.textContent = text;
}

/**
 * Get text content of an element.
 */
export function get_text(element) {
    return element.textContent;
}

/**
 * Set inner HTML of an element.
 *
 * DANGER (A18): this is an `innerHTML` sink. Any HTML passed here is parsed
 * and inserted verbatim — if `html` contains attacker-controlled data, this
 * is an XSS vector. Prefer `set_text` for untrusted strings; when you must
 * insert markup, sanitize first (e.g. DOMPurify). See `docs/known-limitations.md`
 * ("HTML sinks") and `docs/security.md` §T2.
 *
 * `dangerously_set_html` (below) is a clearer-named alias for the same sink;
 * both are kept working.
 */
export function set_html(element, html) {
    element.innerHTML = html;
}

/**
 * Explicitly-named alias for {@link set_html} — an unescaped `innerHTML` sink.
 *
 * DANGER (A18): identical behavior to `set_html`; the name is chosen to make
 * the XSS risk unmissable at the call site (mirrors React's
 * `dangerouslySetInnerHTML`). Sanitize any untrusted input before calling.
 */
export function dangerously_set_html(element, html) {
    element.innerHTML = html;
}

/**
 * Get inner HTML of an element.
 */
export function get_html(element) {
    return element.innerHTML;
}

/**
 * Set an attribute on an element.
 */
export function set_attribute(element, name, value) {
    element.setAttribute(name, value);
}

/**
 * Get an attribute from an element.
 */
export function get_attribute(element, name) {
    return element.getAttribute(name);
}

/**
 * Remove an attribute from an element.
 */
export function remove_attribute(element, name) {
    element.removeAttribute(name);
}

/**
 * Add a CSS class to an element.
 */
export function add_class(element, ...classes) {
    element.classList.add(...classes);
}

/**
 * Remove a CSS class from an element.
 */
export function remove_class(element, ...classes) {
    element.classList.remove(...classes);
}

/**
 * Toggle a CSS class on an element.
 */
export function toggle_class(element, cls) {
    return element.classList.toggle(cls);
}

/**
 * Check if an element has a CSS class.
 */
export function has_class(element, cls) {
    return element.classList.contains(cls);
}
