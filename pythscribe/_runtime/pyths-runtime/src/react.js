// PythScribe React Runtime Helpers
// Provides Python-friendly wrappers around React APIs.

/**
 * `@component` and `@psx` are codegen-meta decorators — the compiler
 * strips them from function definitions and uses them only to flip
 * codegen flags (PSX-mode emission, named export, props destructuring).
 * They're exported here as identity passthroughs so that
 * `from pyths.react import component, psx` resolves at runtime even
 * though the names are never *invoked* in emitted JS.
 */
export function component(fn) { return fn; }
export function psx(fn) { return fn; }

/**
 * Create a React element with Pythonic prop name transforms.
 * Handles: class_name → className, on_click → onClick, etc.
 *
 * Usage in .ps files:
 *   from pyths.react import create_element
 *   create_element("div", {"class_name": "container"}, "Hello")
 *
 * @param {Function} createElement - React.createElement reference
 * @returns {Function} Wrapped createElement that transforms prop names
 */
export function createPropTransformer(createElement) {
    const PROP_MAP = {
        class_name: "className",
        html_for: "htmlFor",
        tab_index: "tabIndex",
        auto_complete: "autoComplete",
        auto_focus: "autoFocus",
        auto_play: "autoPlay",
        col_span: "colSpan",
        row_span: "rowSpan",
        read_only: "readOnly",
        max_length: "maxLength",
        min_length: "minLength",
        src_doc: "srcDoc",
        src_set: "srcSet",
        input_mode: "inputMode",
        cross_origin: "crossOrigin",
        date_time: "dateTime",
        enc_type: "encType",
        form_action: "formAction",
        form_method: "formMethod",
        form_target: "formTarget",
        frame_border: "frameBorder",
        cell_padding: "cellPadding",
        cell_spacing: "cellSpacing",
        use_map: "useMap",
        dangerously_set_inner_html: "dangerouslySetInnerHTML",
        suppress_hydration_warning: "suppressHydrationWarning",
    };

    const EVENT_PREFIX = "on_";

    function transformProps(props) {
        if (props == null || typeof props !== "object") return props;

        const result = {};
        for (const [key, value] of Object.entries(props)) {
            if (PROP_MAP[key]) {
                result[PROP_MAP[key]] = value;
            } else if (key.startsWith(EVENT_PREFIX)) {
                // on_click → onClick, on_key_down → onKeyDown
                const eventName = key
                    .slice(3)
                    .split("_")
                    .map((part, i) => (i === 0 ? part : part.charAt(0).toUpperCase() + part.slice(1)))
                    .join("");
                result["on" + eventName.charAt(0).toUpperCase() + eventName.slice(1)] = value;
            } else if (key.startsWith("aria_")) {
                // aria_label → aria-label
                result[key.replace(/_/g, "-")] = value;
            } else if (key.startsWith("data_")) {
                // data_testid → data-testid
                result[key.replace(/_/g, "-")] = value;
            } else {
                result[key] = value;
            }
        }
        return result;
    }

    return function pyCreateElement(type, props, ...children) {
        return createElement(type, transformProps(props), ...children);
    };
}

/**
 * A hook-style helper that provides Pythonic tuple destructuring for useState.
 * In Python: count, set_count = use_state(0)
 * Returns [value, setter] which Python unpacks as a tuple.
 *
 * This is a pass-through — React's useState already returns [value, setter].
 * Included for documentation / potential future wrapping.
 */
export function wrapUseState(useState) {
    return useState;
}

/**
 * Style object helper: converts snake_case CSS property names to camelCase.
 *   style({"background_color": "red", "font_size": "14px"})
 *   → {backgroundColor: "red", fontSize: "14px"}
 */
export function style(obj) {
    if (obj == null) return null;
    const result = {};
    for (const [key, value] of Object.entries(obj)) {
        if (key.includes("_")) {
            const camelKey = key.replace(/_([a-z])/g, (_, c) => c.toUpperCase());
            result[camelKey] = value;
        } else {
            result[key] = value;
        }
    }
    return result;
}

/**
 * CSS class helper: joins class names, filtering out falsy values.
 *   classes("btn", active and "btn-active", None, "btn-primary")
 *   → "btn btn-active btn-primary"
 */
export function classes(...args) {
    return args.filter(Boolean).join(" ");
}

//# sourceMappingURL=react.js.map
