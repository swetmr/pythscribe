//! React-specific codegen helpers.

/// Convert a Python snake_case identifier to JavaScript camelCase
/// for known React identifiers. Lookup order:
///   1. Closed list of React hook names (use_state → useState, …).
///   2. Closed list of React prop names (class_name → className, …).
///   3. `aria_*` → `aria-*` (kebab-case; React's only kebab convention
///      besides `data-*`).
///   4. `data_*` → `data-*` (HTML data attribute passthrough).
///   5. Generic snake → camelCase fallback.
pub fn snake_to_camel(name: &str) -> String {
    // 1. Closed list of hooks
    if let Some(mapped) = react_hook_mapping(name) {
        return mapped.to_string();
    }
    // 2. Closed list of props
    if let Some(mapped) = react_prop_mapping(name) {
        return mapped.to_string();
    }
    // 3. ARIA: every aria_<x> → aria-<x>. The whole ARIA namespace uses
    //    kebab-case in HTML/JSX, not camelCase. The closed list above
    //    only covers a few common ones; this rule covers the rest
    //    (aria-controls, aria-describedby, aria-modal, aria-flowto, …).
    if let Some(rest) = name.strip_prefix("aria_") {
        if !rest.is_empty() && rest.chars().all(|c| c.is_ascii_lowercase() || c == '_') {
            return format!("aria-{}", rest.replace('_', "-"));
        }
    }
    // 4. data_*: arbitrary user data attributes → kebab.
    if let Some(rest) = name.strip_prefix("data_") {
        if !rest.is_empty() {
            return format!("data-{}", rest.replace('_', "-"));
        }
    }
    // 5. Generic snake → camelCase
    generic_snake_to_camel(name)
}

/// Whether a snake_case name (as found in `from pyths.react import X`) refers
/// to an export that lives in the `react` npm package itself — NOT in
/// pyths-runtime/react. These must be split out into a separate
/// `import { X } from "react"` statement during codegen.
///
/// Why split: the runtime can't re-export React APIs without forcing React as
/// a hard dependency, which breaks non-React consumers of pyths-runtime (the
/// CPython semantic differential tests run in Node without React installed).
/// pyths-runtime/react keeps codegen-meta helpers like `component`, `psx`,
/// `style`, `classes`; React APIs come from React itself.
pub fn is_react_core_export(name: &str) -> bool {
    matches!(
        name,
        // State & refs
        "use_state" | "use_reducer" | "use_ref"
        // Effects
        | "use_effect" | "use_layout_effect" | "use_insertion_effect"
        // Context
        | "use_context"
        // Memoization
        | "use_memo" | "use_callback"
        // Transitions
        | "use_transition" | "use_deferred_value"
        // Other hooks
        | "use_id" | "use_sync_external_store"
        | "use_imperative_handle" | "use_debug_value"
        // React API
        | "create_context" | "create_element" | "create_ref" | "create_portal"
        | "forward_ref" | "start_transition"
        // React 19
        | "use"
        // Common capitalized re-exports
        | "Suspense" | "Fragment" | "StrictMode" | "Profiler"
        | "Component" | "PureComponent" | "Children" | "cloneElement"
        | "isValidElement" | "lazy" | "memo"
    )
}

/// Map snake_case React hook names to their camelCase JS equivalents.
pub fn react_hook_mapping(name: &str) -> Option<&'static str> {
    match name {
        // State & refs
        "use_state" => Some("useState"),
        "use_reducer" => Some("useReducer"),
        "use_ref" => Some("useRef"),
        // Effects
        "use_effect" => Some("useEffect"),
        "use_layout_effect" => Some("useLayoutEffect"),
        "use_insertion_effect" => Some("useInsertionEffect"),
        // Context
        "use_context" => Some("useContext"),
        // Memoization
        "use_memo" => Some("useMemo"),
        "use_callback" => Some("useCallback"),
        // Transitions
        "use_transition" => Some("useTransition"),
        "use_deferred_value" => Some("useDeferredValue"),
        // Other hooks
        "use_id" => Some("useId"),
        "use_sync_external_store" => Some("useSyncExternalStore"),
        "use_imperative_handle" => Some("useImperativeHandle"),
        "use_debug_value" => Some("useDebugValue"),
        // React API
        "create_context" => Some("createContext"),
        "create_element" => Some("createElement"),
        "create_ref" => Some("createRef"),
        "create_portal" => Some("createPortal"),
        "forward_ref" => Some("forwardRef"),
        "start_transition" => Some("startTransition"),
        // Next.js hooks
        "use_router" => Some("useRouter"),
        "use_pathname" => Some("usePathname"),
        "use_search_params" => Some("useSearchParams"),
        "use_params" => Some("useParams"),
        "use_selected_layout_segment" => Some("useSelectedLayoutSegment"),
        "use_selected_layout_segments" => Some("useSelectedLayoutSegments"),
        // React-Redux hooks
        "use_selector" => Some("useSelector"),
        "use_dispatch" => Some("useDispatch"),
        "use_store" => Some("useStore"),
        // @reduxjs/toolkit
        "create_slice" => Some("createSlice"),
        "configure_store" => Some("configureStore"),
        "create_async_thunk" => Some("createAsyncThunk"),
        "create_reducer" => Some("createReducer"),
        "create_action" => Some("createAction"),
        "create_entity_adapter" => Some("createEntityAdapter"),
        "create_selector" => Some("createSelector"),
        "create_listing_matcher" => Some("createListenerMiddleware"),
        _ => None,
    }
}

/// Map snake_case React prop/DOM attribute names to camelCase.
pub fn react_prop_mapping(name: &str) -> Option<&'static str> {
    match name {
        // CSS class
        "class_name" => Some("className"),
        "html_for" => Some("htmlFor"),
        // Events
        "on_click" => Some("onClick"),
        "on_change" => Some("onChange"),
        "on_submit" => Some("onSubmit"),
        "on_input" => Some("onInput"),
        "on_focus" => Some("onFocus"),
        "on_blur" => Some("onBlur"),
        "on_key_down" => Some("onKeyDown"),
        "on_key_up" => Some("onKeyUp"),
        "on_key_press" => Some("onKeyPress"),
        "on_mouse_down" => Some("onMouseDown"),
        "on_mouse_up" => Some("onMouseUp"),
        "on_mouse_enter" => Some("onMouseEnter"),
        "on_mouse_leave" => Some("onMouseLeave"),
        "on_mouse_move" => Some("onMouseMove"),
        "on_mouse_over" => Some("onMouseOver"),
        "on_mouse_out" => Some("onMouseOut"),
        "on_double_click" => Some("onDoubleClick"),
        "on_context_menu" => Some("onContextMenu"),
        "on_drag" => Some("onDrag"),
        "on_drag_end" => Some("onDragEnd"),
        "on_drag_enter" => Some("onDragEnter"),
        "on_drag_leave" => Some("onDragLeave"),
        "on_drag_over" => Some("onDragOver"),
        "on_drag_start" => Some("onDragStart"),
        "on_drop" => Some("onDrop"),
        "on_scroll" => Some("onScroll"),
        "on_wheel" => Some("onWheel"),
        "on_touch_start" => Some("onTouchStart"),
        "on_touch_move" => Some("onTouchMove"),
        "on_touch_end" => Some("onTouchEnd"),
        "on_touch_cancel" => Some("onTouchCancel"),
        "on_pointer_down" => Some("onPointerDown"),
        "on_pointer_up" => Some("onPointerUp"),
        "on_pointer_move" => Some("onPointerMove"),
        "on_pointer_enter" => Some("onPointerEnter"),
        "on_pointer_leave" => Some("onPointerLeave"),
        "on_load" => Some("onLoad"),
        "on_error" => Some("onError"),
        "on_abort" => Some("onAbort"),
        // Form events
        "on_reset" => Some("onReset"),
        "on_select" => Some("onSelect"),
        "on_invalid" => Some("onInvalid"),
        // Media events
        "on_play" => Some("onPlay"),
        "on_pause" => Some("onPause"),
        "on_ended" => Some("onEnded"),
        // Clipboard
        "on_copy" => Some("onCopy"),
        "on_cut" => Some("onCut"),
        "on_paste" => Some("onPaste"),
        // Composition
        "on_composition_start" => Some("onCompositionStart"),
        "on_composition_update" => Some("onCompositionUpdate"),
        "on_composition_end" => Some("onCompositionEnd"),
        // Animation / transition
        "on_animation_start" => Some("onAnimationStart"),
        "on_animation_end" => Some("onAnimationEnd"),
        "on_animation_iteration" => Some("onAnimationIteration"),
        "on_transition_end" => Some("onTransitionEnd"),
        // Attributes
        "tab_index" => Some("tabIndex"),
        "auto_complete" => Some("autoComplete"),
        "auto_focus" => Some("autoFocus"),
        "auto_play" => Some("autoPlay"),
        "col_span" => Some("colSpan"),
        "row_span" => Some("rowSpan"),
        "cell_padding" => Some("cellPadding"),
        "cell_spacing" => Some("cellSpacing"),
        "cross_origin" => Some("crossOrigin"),
        "date_time" => Some("dateTime"),
        "enc_type" => Some("encType"),
        "form_action" => Some("formAction"),
        "form_method" => Some("formMethod"),
        "form_target" => Some("formTarget"),
        "frame_border" => Some("frameBorder"),
        "input_mode" => Some("inputMode"),
        "max_length" => Some("maxLength"),
        "min_length" => Some("minLength"),
        "read_only" => Some("readOnly"),
        "src_doc" => Some("srcDoc"),
        "src_set" => Some("srcSet"),
        "use_map" => Some("useMap"),
        // Form / input attributes
        "default_value" => Some("defaultValue"),
        "default_checked" => Some("defaultChecked"),
        "default_selected" => Some("defaultSelected"),
        "spell_check" => Some("spellCheck"),
        "content_editable" => Some("contentEditable"),
        "no_validate" => Some("noValidate"),
        "auto_save" => Some("autoSave"),
        "list_id" => Some("listId"), // input.list as listId rare; skip
        // Resource attributes
        "referrer_policy" => Some("referrerPolicy"),
        "fetch_priority" => Some("fetchPriority"),
        "image_size_set" => Some("imageSrcSet"),
        "image_sizes" => Some("imageSizes"),
        "as_type" => Some("as"), // <link as="...">
        // Image / video
        "plays_inline" => Some("playsInline"),
        "controls_list" => Some("controlsList"),
        "disable_picture_in_picture" => Some("disablePictureInPicture"),
        "disable_remote_playback" => Some("disableRemotePlayback"),
        "media_group" => Some("mediaGroup"),
        // Iframe / embed
        "allow_full_screen" => Some("allowFullScreen"),
        "allow_transparency" => Some("allowTransparency"),
        "marginal_height" => Some("marginHeight"),
        "marginal_width" => Some("marginWidth"),
        // Table
        "row_group" => Some("rowGroup"),
        "scope_attr" => Some("scope"),
        // Generic / SVG-shared
        "view_box" => Some("viewBox"),
        "preserve_aspect_ratio" => Some("preserveAspectRatio"),
        "fill_rule" => Some("fillRule"),
        "clip_rule" => Some("clipRule"),
        "stroke_width" => Some("strokeWidth"),
        "stroke_linecap" => Some("strokeLinecap"),
        "stroke_linejoin" => Some("strokeLinejoin"),
        "stroke_miterlimit" => Some("strokeMiterlimit"),
        "stroke_dasharray" => Some("strokeDasharray"),
        "stroke_dashoffset" => Some("strokeDashoffset"),
        "stroke_opacity" => Some("strokeOpacity"),
        "fill_opacity" => Some("fillOpacity"),
        "text_anchor" => Some("textAnchor"),
        "dominant_baseline" => Some("dominantBaseline"),
        "alignment_baseline" => Some("alignmentBaseline"),
        "transform_origin" => Some("transformOrigin"),
        "xmlns_xlink" => Some("xmlnsXlink"),
        "xlink_href" => Some("xlinkHref"),
        // Misc
        "accept_charset" => Some("acceptCharset"),
        "http_equiv" => Some("httpEquiv"),
        "char_set" => Some("charSet"),
        "item_scope" => Some("itemScope"),
        "item_type" => Some("itemType"),
        "item_id" => Some("itemId"),
        "item_prop" => Some("itemProp"),
        "item_ref" => Some("itemRef"),
        "key_type" => Some("keyType"),
        "key_params" => Some("keyParams"),
        "radio_group" => Some("radioGroup"),
        "menu_item_id" => Some("menuItemId"),
        // SSR / hydration
        "is_static" => Some("isStatic"),
        // Aria
        "aria_label" => Some("aria-label"),
        "aria_hidden" => Some("aria-hidden"),
        "aria_disabled" => Some("aria-disabled"),
        "aria_expanded" => Some("aria-expanded"),
        "aria_selected" => Some("aria-selected"),
        "aria_checked" => Some("aria-checked"),
        "aria_live" => Some("aria-live"),
        "aria_role" => Some("role"),
        // Style
        "dangerously_set_inner_html" => Some("dangerouslySetInnerHTML"),
        "suppress_hydration_warning" => Some("suppressHydrationWarning"),
        _ => None,
    }
}

/// Map Python Next.js function names to their JS convention equivalents.
pub fn nextjs_export_mapping(name: &str) -> Option<&'static str> {
    match name {
        "get_static_props" => Some("getStaticProps"),
        "get_server_side_props" => Some("getServerSideProps"),
        "get_static_paths" => Some("getStaticPaths"),
        "generate_static_params" => Some("generateStaticParams"),
        "generate_metadata" => Some("generateMetadata"),
        _ => None,
    }
}

/// Check if a name should be treated as a Next.js exported function.
pub fn is_nextjs_export(name: &str) -> bool {
    nextjs_export_mapping(name).is_some()
}

/// Generic snake_case → camelCase conversion.
fn generic_snake_to_camel(name: &str) -> String {
    if !name.contains('_') {
        return name.to_string();
    }
    let mut result = String::new();
    let mut capitalize_next = false;
    for (i, ch) in name.chars().enumerate() {
        if ch == '_' {
            if i == 0 || i == name.len() - 1 {
                result.push('_'); // preserve leading/trailing underscores
            } else {
                capitalize_next = true;
            }
        } else if capitalize_next {
            result.push(ch.to_ascii_uppercase());
            capitalize_next = false;
        } else {
            result.push(ch);
        }
    }
    result
}

/// Check if a name is a known HTML element (for PSX rendering).
pub fn is_html_element(name: &str) -> bool {
    matches!(
        name,
        // Main root
        "html"
        // Document metadata
        | "head" | "title" | "base" | "link" | "meta" | "style"
        // Sectioning
        | "body" | "article" | "section" | "nav" | "aside"
        | "header" | "footer" | "main" | "address"
        // Heading
        | "h1" | "h2" | "h3" | "h4" | "h5" | "h6" | "hgroup"
        // Block text
        | "div" | "p" | "hr" | "pre" | "blockquote"
        | "ol" | "ul" | "li" | "dl" | "dt" | "dd"
        | "figure" | "figcaption"
        // Inline text
        | "span" | "a" | "em" | "strong" | "small" | "s"
        | "cite" | "q" | "dfn" | "abbr" | "time" | "code"
        | "var" | "samp" | "kbd" | "sub" | "sup"
        | "i" | "b" | "u" | "mark" | "ruby" | "rt" | "rp"
        | "bdi" | "bdo" | "br" | "wbr"
        // Media
        | "img" | "video" | "audio" | "source" | "track"
        | "map" | "area" | "picture" | "canvas" | "svg"
        // Embedded
        | "iframe" | "embed" | "object" | "param" | "portal"
        // Table
        | "table" | "caption" | "colgroup" | "col"
        | "thead" | "tbody" | "tfoot" | "tr" | "td" | "th"
        // Form
        | "form" | "label" | "input" | "button" | "select"
        | "datalist" | "optgroup" | "option" | "textarea"
        | "output" | "progress" | "meter" | "fieldset" | "legend"
        // Interactive
        | "details" | "summary" | "dialog" | "menu"
        // Script
        | "script" | "noscript" | "template" | "slot"
        // SVG common
        | "path" | "circle" | "rect" | "line" | "polyline"
        | "polygon" | "ellipse" | "g" | "text" | "defs"
        | "use" | "clipPath" | "mask" | "pattern"
        | "linearGradient" | "radialGradient" | "stop"
        | "foreignObject"
    )
}

/// Check if a name should be treated as a PSX element:
/// either a known HTML tag or a capitalized component name.
pub fn is_psx_element(name: &str) -> bool {
    is_html_element(name) || name.chars().next().is_some_and(|c| c.is_uppercase())
}

/// Known JS/DOM global *constructors*. Inside a `@component` (PSX mode) a
/// capitalized call is normally a React element, but these built-in globals
/// are the exception — `EventSource(url)` means `new EventSource(url)`, not
/// `createElement(EventSource, ...)`. (User-defined `class` names already
/// route to `new` via `known_classes`; this allowlist covers external/global
/// constructors that never appear in that set.) Regression guard for B-037
/// (pythscribe#53). Conservative on purpose: only names that are real global
/// constructors AND implausible as React component names.
pub fn is_builtin_constructor(name: &str) -> bool {
    matches!(
        name,
        // Streaming / networking / workers
        "EventSource" | "WebSocket" | "Worker" | "SharedWorker" | "ServiceWorker"
        | "XMLHttpRequest" | "BroadcastChannel" | "MessageChannel" | "MessagePort"
        | "AbortController" | "AbortSignal"
        // URLs / fetch primitives
        | "URL" | "URLSearchParams" | "FormData" | "Headers" | "Request" | "Response"
        // Files / binary
        | "Blob" | "File" | "FileReader" | "ArrayBuffer" | "SharedArrayBuffer"
        | "DataView" | "TextEncoder" | "TextDecoder"
        // Observers
        | "IntersectionObserver" | "ResizeObserver" | "MutationObserver"
        | "PerformanceObserver"
        // Events / media
        | "Event" | "CustomEvent" | "Notification" | "Image" | "Audio"
        // Core JS objects that require `new`
        | "Date" | "Map" | "WeakMap" | "Set" | "WeakSet" | "Promise" | "Proxy"
        | "RegExp" | "WeakRef" | "FinalizationRegistry"
        // Typed arrays
        | "Int8Array" | "Uint8Array" | "Uint8ClampedArray" | "Int16Array"
        | "Uint16Array" | "Int32Array" | "Uint32Array" | "Float32Array"
        | "Float64Array" | "BigInt64Array" | "BigUint64Array"
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_hook_mappings() {
        assert_eq!(snake_to_camel("use_state"), "useState");
        assert_eq!(snake_to_camel("use_effect"), "useEffect");
        assert_eq!(snake_to_camel("use_callback"), "useCallback");
        assert_eq!(snake_to_camel("use_memo"), "useMemo");
        assert_eq!(snake_to_camel("use_ref"), "useRef");
        assert_eq!(snake_to_camel("use_context"), "useContext");
        assert_eq!(snake_to_camel("use_reducer"), "useReducer");
    }

    #[test]
    fn test_builtin_constructor_allowlist() {
        // Real global constructors → true (route to `new` in PSX).
        for n in [
            "EventSource",
            "WebSocket",
            "Worker",
            "URL",
            "URLSearchParams",
            "FormData",
            "Date",
            "Map",
            "Set",
            "Promise",
            "AbortController",
            "IntersectionObserver",
            "Uint8Array",
        ] {
            assert!(
                is_builtin_constructor(n),
                "{} should be a builtin constructor",
                n
            );
        }
        // Plausible React component / non-constructor names → false (stay PSX).
        for n in [
            "Counter",
            "PaperList",
            "Studio",
            "Header",
            "App",
            "div",
            "EventStream",
        ] {
            assert!(
                !is_builtin_constructor(n),
                "{} must NOT be treated as a builtin constructor",
                n
            );
        }
    }

    #[test]
    fn test_prop_mappings() {
        assert_eq!(snake_to_camel("class_name"), "className");
        assert_eq!(snake_to_camel("on_click"), "onClick");
        assert_eq!(snake_to_camel("on_change"), "onChange");
        assert_eq!(snake_to_camel("html_for"), "htmlFor");
        assert_eq!(snake_to_camel("tab_index"), "tabIndex");
    }

    #[test]
    fn test_nextjs_exports() {
        assert_eq!(
            nextjs_export_mapping("get_static_props").unwrap(),
            "getStaticProps"
        );
        assert_eq!(
            nextjs_export_mapping("get_server_side_props").unwrap(),
            "getServerSideProps"
        );
        assert_eq!(
            nextjs_export_mapping("generate_static_params").unwrap(),
            "generateStaticParams"
        );
    }

    #[test]
    fn test_redux_hook_mappings() {
        assert_eq!(snake_to_camel("use_selector"), "useSelector");
        assert_eq!(snake_to_camel("use_dispatch"), "useDispatch");
        assert_eq!(snake_to_camel("use_store"), "useStore");
        assert_eq!(snake_to_camel("create_slice"), "createSlice");
        assert_eq!(snake_to_camel("configure_store"), "configureStore");
        assert_eq!(snake_to_camel("create_async_thunk"), "createAsyncThunk");
        assert_eq!(snake_to_camel("create_selector"), "createSelector");
    }

    #[test]
    fn test_generic_camel_case() {
        assert_eq!(generic_snake_to_camel("hello_world"), "helloWorld");
        assert_eq!(generic_snake_to_camel("my_var_name"), "myVarName");
        assert_eq!(generic_snake_to_camel("simple"), "simple");
        assert_eq!(generic_snake_to_camel("_private"), "_private");
    }

    // -------- Batch A.1: community use_* hooks via generic fallthrough --------

    #[test]
    fn test_community_hooks_via_generic_rule() {
        // Names not in the closed list should still snake→camel via the
        // generic fallthrough — covers the community React-hook ecosystem
        // (React Query, React Hook Form, Framer Motion, Zustand, etc.)
        assert_eq!(snake_to_camel("use_query"), "useQuery");
        assert_eq!(snake_to_camel("use_mutation"), "useMutation");
        assert_eq!(snake_to_camel("use_navigate"), "useNavigate");
        assert_eq!(snake_to_camel("use_form"), "useForm");
        assert_eq!(snake_to_camel("use_field"), "useField");
        assert_eq!(snake_to_camel("use_motion_value"), "useMotionValue");
        assert_eq!(snake_to_camel("use_animate"), "useAnimate");
        assert_eq!(snake_to_camel("use_atom"), "useAtom");
        assert_eq!(snake_to_camel("use_swr"), "useSwr");
        assert_eq!(snake_to_camel("use_recoil_state"), "useRecoilState");
        assert_eq!(snake_to_camel("use_machine"), "useMachine");
    }

    // -------- Batch A.5: ARIA + data attribute prefix rules --------

    #[test]
    fn test_aria_attributes_become_kebab() {
        // ARIA props use kebab-case in HTML/JSX, not camelCase.
        assert_eq!(snake_to_camel("aria_label"), "aria-label");
        assert_eq!(snake_to_camel("aria_describedby"), "aria-describedby");
        assert_eq!(snake_to_camel("aria_controls"), "aria-controls");
        assert_eq!(snake_to_camel("aria_flowto"), "aria-flowto");
        assert_eq!(snake_to_camel("aria_modal"), "aria-modal");
        assert_eq!(snake_to_camel("aria_relevant"), "aria-relevant");
        assert_eq!(snake_to_camel("aria_value_text"), "aria-value-text");
    }

    #[test]
    fn test_aria_role_special_case_kept() {
        // aria_role → "role" (not aria-role); special case preserved.
        assert_eq!(snake_to_camel("aria_role"), "role");
    }

    #[test]
    fn test_data_attributes_become_kebab() {
        assert_eq!(snake_to_camel("data_id"), "data-id");
        assert_eq!(snake_to_camel("data_test_id"), "data-test-id");
        assert_eq!(snake_to_camel("data_user_role"), "data-user-role");
    }

    #[test]
    fn test_extended_html_props() {
        assert_eq!(snake_to_camel("default_value"), "defaultValue");
        assert_eq!(snake_to_camel("default_checked"), "defaultChecked");
        assert_eq!(snake_to_camel("read_only"), "readOnly");
        assert_eq!(snake_to_camel("auto_focus"), "autoFocus");
        assert_eq!(snake_to_camel("auto_complete"), "autoComplete");
        assert_eq!(snake_to_camel("spell_check"), "spellCheck");
        assert_eq!(snake_to_camel("content_editable"), "contentEditable");
        assert_eq!(snake_to_camel("referrer_policy"), "referrerPolicy");
        assert_eq!(snake_to_camel("accept_charset"), "acceptCharset");
        assert_eq!(snake_to_camel("http_equiv"), "httpEquiv");
        assert_eq!(snake_to_camel("plays_inline"), "playsInline");
        assert_eq!(snake_to_camel("allow_full_screen"), "allowFullScreen");
    }

    #[test]
    fn test_extended_svg_props() {
        assert_eq!(snake_to_camel("view_box"), "viewBox");
        assert_eq!(
            snake_to_camel("preserve_aspect_ratio"),
            "preserveAspectRatio"
        );
        assert_eq!(snake_to_camel("stroke_width"), "strokeWidth");
        assert_eq!(snake_to_camel("stroke_linecap"), "strokeLinecap");
        assert_eq!(snake_to_camel("text_anchor"), "textAnchor");
        assert_eq!(snake_to_camel("dominant_baseline"), "dominantBaseline");
    }
}
