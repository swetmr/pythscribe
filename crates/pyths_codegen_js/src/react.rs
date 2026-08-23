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

/// Which module actually exports a given `from pyths.react import X` helper.
///
/// `pyths.react` is a HYBRID surface: the user writes one import, but the
/// symbols live in three genuinely different upstream places plus the pyths
/// runtime. Routing every name through a single audited table (this enum + the
/// `react_helper_source` match) is the root fix for the WB-22/WB-23 family, in
/// which individual symbols were mapped to a module that does not export them:
///
/// * `ReactCore` (`"react"`) — hooks and React-core APIs (`createElement`,
///   `cloneElement`, `isValidElement`, `Fragment`, `memo`, `forwardRef`, …).
/// * `ReactDom` (`"react-dom"`) — DOM-only APIs (`createPortal`, `flushSync`,
///   `findDOMNode`). These are NOT in the `react` package (WB-23:
///   `createPortal` was wrongly mapped to `"react"`).
/// * `ReactDomClient` (`"react-dom/client"`) — the React 18+ client roots
///   (`createRoot`, `hydrateRoot`).
/// * `PythsRuntime` (`"pyths-runtime/react"`) — the codegen-meta helpers the
///   runtime ACTUALLY exports (`component`, `psx`, `style`, `classes`, plus the
///   internal `createPropTransformer`/`wrapUseState`). Nothing else may route
///   here: the runtime deliberately does not re-export React (that would force
///   React as a hard dep and break the non-React CPython differential tests),
///   so a React symbol mapped here is a load-time "no such export" crash
///   (WB-22: `cloneElement` fell through to this module and blanked the app).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReactHelperSource {
    ReactCore,
    ReactDom,
    ReactDomClient,
    PythsRuntime,
}

impl ReactHelperSource {
    /// The literal JS module specifier this source imports from.
    pub fn module(self) -> &'static str {
        match self {
            ReactHelperSource::ReactCore => "react",
            ReactHelperSource::ReactDom => "react-dom",
            ReactHelperSource::ReactDomClient => "react-dom/client",
            ReactHelperSource::PythsRuntime => "pyths-runtime/react",
        }
    }
}

/// THE single audited routing table: PYTHON name (snake_case for
/// functions/hooks, Capitalized for class/element re-exports, exactly as
/// written at the import/member site, before `snake_to_camel`) → the module
/// that really exports the symbol.
///
/// This is a `pub` DATA table (not match arms) so consumers — the emit paths,
/// `route_namespace_member`, and the real-package boundary test
/// (`tests/react_boundary_test.rs`) — iterate the SAME table. The boundary
/// test imports every non-runtime entry from its declared module against the
/// actually-installed React 19 packages, so the table is verified exhaustively
/// BY CONSTRUCTION: an entry added here is automatically checked; there is no
/// second manually-synced list to forget (the old test's `ROUTABLE` array was
/// exactly that escape hatch).
///
/// Entries whose symbol was REMOVED in React 19 (see `REACT_19_REMOVED`) are
/// still listed with their would-be module: every import/member path checks
/// `react_19_removed` FIRST, so the route is never emitted — it exists so the
/// boundary test can assert the symbol is genuinely ABSENT from that module.
pub const REACT_HELPER_TABLE: &[(&str, ReactHelperSource)] = &[
    // ---- pyths-runtime/react — the runtime's real codegen-meta exports.
    ("component", ReactHelperSource::PythsRuntime),
    ("psx", ReactHelperSource::PythsRuntime),
    ("style", ReactHelperSource::PythsRuntime),
    ("classes", ReactHelperSource::PythsRuntime),
    ("create_prop_transformer", ReactHelperSource::PythsRuntime),
    ("wrap_use_state", ReactHelperSource::PythsRuntime),
    // ---- react-dom — DOM-only APIs (NOT in the `react` package).
    ("create_portal", ReactHelperSource::ReactDom),
    ("flush_sync", ReactHelperSource::ReactDom),
    ("use_form_status", ReactHelperSource::ReactDom),
    ("use_form_state", ReactHelperSource::ReactDom),
    // react-dom symbols REMOVED in 19 — would-be routes only (see above).
    ("find_dom_node", ReactHelperSource::ReactDom),
    ("render", ReactHelperSource::ReactDom),
    ("hydrate", ReactHelperSource::ReactDom),
    ("unmount_component_at_node", ReactHelperSource::ReactDom),
    // ---- react-dom/client — React 18+ client roots.
    ("create_root", ReactHelperSource::ReactDomClient),
    ("hydrate_root", ReactHelperSource::ReactDomClient),
    // ---- react (core) — hooks + core APIs + capitalized re-exports.
    // Hooks
    ("use_state", ReactHelperSource::ReactCore),
    ("use_reducer", ReactHelperSource::ReactCore),
    ("use_ref", ReactHelperSource::ReactCore),
    ("use_effect", ReactHelperSource::ReactCore),
    ("use_layout_effect", ReactHelperSource::ReactCore),
    ("use_insertion_effect", ReactHelperSource::ReactCore),
    ("use_context", ReactHelperSource::ReactCore),
    ("use_memo", ReactHelperSource::ReactCore),
    ("use_callback", ReactHelperSource::ReactCore),
    ("use_transition", ReactHelperSource::ReactCore),
    ("use_deferred_value", ReactHelperSource::ReactCore),
    ("use_id", ReactHelperSource::ReactCore),
    ("use_sync_external_store", ReactHelperSource::ReactCore),
    ("use_imperative_handle", ReactHelperSource::ReactCore),
    ("use_debug_value", ReactHelperSource::ReactCore),
    // React 19 additions
    ("use", ReactHelperSource::ReactCore),
    ("use_optimistic", ReactHelperSource::ReactCore),
    ("use_action_state", ReactHelperSource::ReactCore),
    ("use_effect_event", ReactHelperSource::ReactCore),
    ("act", ReactHelperSource::ReactCore),
    ("cache", ReactHelperSource::ReactCore),
    // Core APIs
    ("create_context", ReactHelperSource::ReactCore),
    ("create_element", ReactHelperSource::ReactCore),
    ("clone_element", ReactHelperSource::ReactCore),
    ("create_ref", ReactHelperSource::ReactCore),
    ("is_valid_element", ReactHelperSource::ReactCore),
    ("forward_ref", ReactHelperSource::ReactCore),
    ("start_transition", ReactHelperSource::ReactCore),
    ("lazy", ReactHelperSource::ReactCore),
    ("memo", ReactHelperSource::ReactCore),
    // react-core symbol REMOVED in 19 — would-be route only.
    ("create_factory", ReactHelperSource::ReactCore),
    // Capitalized re-exports (arrive un-snaked)
    ("Suspense", ReactHelperSource::ReactCore),
    ("Fragment", ReactHelperSource::ReactCore),
    ("StrictMode", ReactHelperSource::ReactCore),
    ("Profiler", ReactHelperSource::ReactCore),
    ("Component", ReactHelperSource::ReactCore),
    ("PureComponent", ReactHelperSource::ReactCore),
    ("Children", ReactHelperSource::ReactCore),
];

/// Resolve the upstream module for a `from pyths.react import <name>` symbol.
///
/// A lookup over `REACT_HELPER_TABLE` (the single audited surface). The
/// default for unknown names is `ReactCore`: `pyths.react` fronts React, only
/// the runtime meta-helpers live in `pyths-runtime/react` and they are all
/// listed explicitly, so an unknown symbol is far likelier to be a genuine
/// React export than a runtime shim — and defaulting unknowns to the runtime
/// is precisely what produced the WB-22 "no such export" crash.
pub fn react_helper_source(name: &str) -> ReactHelperSource {
    REACT_HELPER_TABLE
        .iter()
        .find(|(n, _)| *n == name)
        .map(|(_, s)| *s)
        .unwrap_or(ReactHelperSource::ReactCore)
}

/// Back-compat predicate: whether a `pyths.react` symbol resolves to the
/// `react` core package (as opposed to react-dom or the pyths runtime).
pub fn is_react_core_export(name: &str) -> bool {
    react_helper_source(name) == ReactHelperSource::ReactCore
}

/// One React-19-removed export: the PYTHON spelling, the camelCase JS
/// spelling, and the one-line remediation for the compile diagnostic.
pub struct React19Removed {
    pub py_name: &'static str,
    pub js_name: &'static str,
    pub message: &'static str,
}

/// B5 — the COMPLETE set of symbols the routing table maps (or would map)
/// to a module that no longer exports them in React 19 / react-dom 19,
/// verified against the actually-installed `react@19` / `react-dom@19`
/// packages (a production React app, 19.2.6). Emitting any of these is a hard
/// ESM "no such export" load error → blank app. The WB-22/23 table tests only
/// asserted the table against ITSELF, never against the real packages, so a
/// dead route passed — and the original fix listed only `findDOMNode`,
/// validating its own shape and missing the adjacent removals (`render`,
/// `hydrate`, `unmountComponentAtNode`, `createFactory`). This is a `pub`
/// DATA table so the boundary test iterates it exhaustively: every entry is
/// asserted genuinely ABSENT from its would-be module against the real
/// packages, and every entry must also appear in `REACT_HELPER_TABLE` (its
/// would-be route), so the set cannot silently drift back into a live route.
///
/// Lookups match the PYTHON name AND the camelCase JS spelling, so the
/// diagnostic fires regardless of how the user wrote it.
pub const REACT_19_REMOVED: &[React19Removed] = &[
    React19Removed {
        py_name: "find_dom_node",
        js_name: "findDOMNode",
        message: "`findDOMNode` was removed in React 19 (react-dom no longer exports it) — \
             importing it is a load-time \"no such export\" error. Use a ref (`useRef`/\
             `createRef` on the element) instead of `findDOMNode`.",
    },
    React19Removed {
        py_name: "render",
        js_name: "render",
        message: "`render` was removed from react-dom in React 19 — importing it is a \
             load-time \"no such export\" error. Use `create_root` (createRoot from \
             react-dom/client): `root = create_root(el)` then `root.render(app)`.",
    },
    React19Removed {
        py_name: "hydrate",
        js_name: "hydrate",
        message: "`hydrate` was removed from react-dom in React 19 — importing it is a \
             load-time \"no such export\" error. Use `hydrate_root` (hydrateRoot from \
             react-dom/client) instead.",
    },
    React19Removed {
        py_name: "unmount_component_at_node",
        js_name: "unmountComponentAtNode",
        message: "`unmountComponentAtNode` was removed from react-dom in React 19 — \
             importing it is a load-time \"no such export\" error. Call `.unmount()` \
             on the root returned by `create_root` instead.",
    },
    React19Removed {
        py_name: "create_factory",
        js_name: "createFactory",
        message: "`createFactory` was removed from react in React 19 — importing it is a \
             load-time \"no such export\" error. Call `create_element(type, ...)` \
             directly instead of a factory.",
    },
];

/// Look up a React-19-removed symbol by either spelling (python snake or
/// camelCase JS). Returns the remediation message for the compile diagnostic.
pub fn react_19_removed(name: &str) -> Option<&'static str> {
    REACT_19_REMOVED
        .iter()
        .find(|e| e.py_name == name || e.js_name == name)
        .map(|e| e.message)
}

/// The CORE React packages the routing table covers, keyed on the RAW Python
/// module spelling at an `import` site (kebab forms kept defensively for
/// callers that pass the mapped name). This is the gate for two class rules:
///
/// * a namespace alias of one of these modules gets full MEMBER routing
///   (`route_namespace_member`) — camel-casing + module check + removed check;
/// * the `react_19_removed` import diagnostic is SCOPED to these modules (plus
///   the `pyths.react` hybrid). Other react-ecosystem packages legitimately
///   export names like `render` (`@testing-library/react`), so a global
///   symbol-keyed check would misfire on them.
pub fn core_react_module(module: &str) -> Option<ReactHelperSource> {
    match module {
        "react" => Some(ReactHelperSource::ReactCore),
        "react_dom" | "react-dom" => Some(ReactHelperSource::ReactDom),
        "react_dom.client" | "react-dom/client" => Some(ReactHelperSource::ReactDomClient),
        _ => None,
    }
}

/// The routing decision for a member access `<ns>.<member>` where `<ns>` is a
/// namespace alias of a core React module (`import react [as R]`,
/// `import react_dom [as D]`, `import react_dom.client as C`).
pub enum MemberRoute {
    /// The member is exported by the base module — emit `<ns>.<jsName>`.
    Routed(String),
    /// The member was removed in React 19 — compile diagnostic (message).
    Removed(&'static str),
    /// The member is a KNOWN symbol exported by a DIFFERENT module — emitting
    /// `<ns>.<jsName>` would be a silent `undefined`. Compile diagnostic.
    WrongModule {
        js_name: String,
        exports_from: ReactHelperSource,
    },
}

/// THE uniform member-routing rule (0.2.2 member-call class fix). For a member
/// access `<base>.<member>(...)` — or a bound reference `f = <base>.<member>`
/// — where `<base>` is a core-React namespace alias, the member is routed
/// EXACTLY like the from-import path: `react_19_removed` checked first, then
/// camel-cased via `snake_to_camel`, then module-checked via the audited
/// `REACT_HELPER_TABLE`. The original fix special-cased only
/// `create_element`-with-≥2-args in call position — every other member
/// (`react.use_state`, `react.clone_element`, `react_dom.create_portal`,
/// single-arg `create_element`, value-position references) compiled to a
/// silent-dead snake identifier. This one rule closes the whole class:
///
/// * removed in React 19 (either spelling) → `Removed` (compile diagnostic);
/// * known symbol, matching module → `Routed(jsName)`;
/// * known symbol, DIFFERENT module (`react.create_portal`,
///   `react_dom.use_state`, `react.component`) → `WrongModule` (diagnostic —
///   the namespace object genuinely has no such export);
/// * unknown symbol → `Routed(snake_to_camel(member))`, byte-identical to
///   what `from <module> import <member>` emits (the from-import parity
///   contract; unknown names cannot be module-checked without a full export
///   inventory, and the boundary test owns table completeness).
///
/// Known-symbol lookups match the python spelling OR its camelCase JS form
/// (`react.createPortal` is caught, not just `react.create_portal`).
pub fn route_namespace_member(base: ReactHelperSource, member: &str) -> MemberRoute {
    if let Some(msg) = react_19_removed(member) {
        return MemberRoute::Removed(msg);
    }
    let known = REACT_HELPER_TABLE
        .iter()
        .find(|(n, _)| *n == member || snake_to_camel(n) == member);
    match known {
        Some((name, src)) => {
            let js_name = snake_to_camel(name);
            if *src == base {
                MemberRoute::Routed(js_name)
            } else {
                MemberRoute::WrongModule {
                    js_name,
                    exports_from: *src,
                }
            }
        }
        None => MemberRoute::Routed(snake_to_camel(member)),
    }
}

/// The React hooks whose FIRST-argument callback's RETURN is stored as a
/// cleanup ("destroy") and invoked by React. React accepts only `undefined`
/// or a function as that return; a Python effect ending in `return None`
/// emits `return null`, and since `null !== undefined` React calls it →
/// "destroy is not a function" (WF-1). Callbacks to these hooks are wrapped
/// in `__pyEffect`, which coerces any non-function return to `undefined`.
/// `use_sync_external_store` belongs here too: its `subscribe` callback's
/// return is the unsubscribe cleanup, invoked the same way (WF-1 round 2).
/// The value-returning hooks (`use_memo`/`use_callback`) and all other hooks
/// are NOT wrapped — their return is data, not a cleanup.
pub fn is_cleanup_effect_hook(name: &str) -> bool {
    matches!(
        name,
        "use_effect" | "use_layout_effect" | "use_insertion_effect" | "use_sync_external_store"
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
        "clone_element" => Some("cloneElement"),
        "is_valid_element" => Some("isValidElement"),
        // react-dom / react-dom/client APIs (the generic snake→camel rule
        // mis-cases `find_dom_node` as `findDomNode`; the acronym must stay
        // upper — pin the whole react-dom family explicitly).
        "flush_sync" => Some("flushSync"),
        "find_dom_node" => Some("findDOMNode"),
        "create_root" => Some("createRoot"),
        "hydrate_root" => Some("hydrateRoot"),
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

/// WB-17 — the ES primitive-WRAPPER globals. Called WITHOUT `new` they are
/// TYPE-CONVERSION functions returning primitives (`Boolean("")` → `false`,
/// `Number("3")` → `3`, `String(5)` → `"5"`); called WITH `new` they return
/// boxed wrapper OBJECTS (`new Boolean(false)` is TRUTHY, `typeof
/// new Number(3)` is `"object"`), and `Symbol`/`BigInt` THROW when `new`-ed.
/// The capitalized-name → `new` heuristic must therefore emit a BARE call for
/// these — never `new`.
pub fn is_js_primitive_wrapper(name: &str) -> bool {
    matches!(name, "Boolean" | "Number" | "String" | "Symbol" | "BigInt")
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

    // -------- WB-22/WB-23: the audited pyths.react → module routing table --------

    #[test]
    fn test_react_helper_source_routing() {
        use ReactHelperSource::*;
        // react-core hooks + APIs.
        for n in [
            "use_state",
            "use_effect",
            "use_memo",
            "use_ref",
            "create_element",
            "create_context",
            "create_ref",
            "forward_ref",
            "start_transition",
            "lazy",
            "memo",
            "use",
            "Suspense",
            "Fragment",
            "StrictMode",
            "Children",
            // WB-22: cloneElement/isValidElement are REACT CORE — the old
            // table listed them only in camelCase, so the snake python names
            // fell through to pyths-runtime/react (a non-existent export).
            "clone_element",
            "is_valid_element",
        ] {
            assert_eq!(
                react_helper_source(n),
                ReactCore,
                "{} must route to react core",
                n
            );
        }
        // react-dom — WB-23: createPortal lives here, NOT in react.
        for n in ["create_portal", "flush_sync", "find_dom_node"] {
            assert_eq!(
                react_helper_source(n),
                ReactDom,
                "{} must route to react-dom",
                n
            );
        }
        // react-dom/client — the 18+ roots.
        for n in ["create_root", "hydrate_root"] {
            assert_eq!(
                react_helper_source(n),
                ReactDomClient,
                "{} must route to react-dom/client",
                n
            );
        }
        // pyths-runtime/react — ONLY the runtime's real codegen-meta exports.
        for n in ["component", "psx", "style", "classes"] {
            assert_eq!(
                react_helper_source(n),
                PythsRuntime,
                "{} must route to pyths-runtime/react",
                n
            );
        }
    }

    #[test]
    fn test_react_helper_source_module_strings() {
        assert_eq!(ReactHelperSource::ReactCore.module(), "react");
        assert_eq!(ReactHelperSource::ReactDom.module(), "react-dom");
        assert_eq!(
            ReactHelperSource::ReactDomClient.module(),
            "react-dom/client"
        );
        assert_eq!(
            ReactHelperSource::PythsRuntime.module(),
            "pyths-runtime/react"
        );
    }

    #[test]
    fn test_wb22_wb23_symbols_convert_correctly() {
        // The affected symbols must snake→camel to the exact React identifier.
        assert_eq!(snake_to_camel("clone_element"), "cloneElement");
        assert_eq!(snake_to_camel("create_portal"), "createPortal");
        assert_eq!(snake_to_camel("is_valid_element"), "isValidElement");
        assert_eq!(snake_to_camel("flush_sync"), "flushSync");
        // Acronym must stay upper — generic snake→camel would give findDomNode.
        assert_eq!(snake_to_camel("find_dom_node"), "findDOMNode");
        assert_eq!(snake_to_camel("create_root"), "createRoot");
        assert_eq!(snake_to_camel("hydrate_root"), "hydrateRoot");
    }

    #[test]
    fn test_react_helper_audit_no_runtime_leak() {
        use ReactHelperSource::*;
        // AUDIT: the ONLY names that may route to pyths-runtime/react are the
        // four codegen-meta helpers the runtime actually exports (plus its two
        // internal helpers). Any React symbol routed there is a load-time crash
        // (WB-22). Guard the whole audited surface against regression.
        let runtime_ok = [
            "component",
            "psx",
            "style",
            "classes",
            "create_prop_transformer",
            "wrap_use_state",
        ];
        for n in [
            "use_state",
            "use_effect",
            "use_reducer",
            "use_context",
            "use_memo",
            "use_callback",
            "use_ref",
            "use_id",
            "use_transition",
            "use_deferred_value",
            "use_sync_external_store",
            "use_imperative_handle",
            "use_debug_value",
            "use_layout_effect",
            "use_insertion_effect",
            "use",
            "create_element",
            "clone_element",
            "is_valid_element",
            "create_context",
            "create_ref",
            "forward_ref",
            "start_transition",
            "lazy",
            "memo",
            "Suspense",
            "Fragment",
            "StrictMode",
            "Profiler",
            "Component",
            "PureComponent",
            "Children",
            "create_portal",
            "flush_sync",
            "find_dom_node",
            "create_root",
            "hydrate_root",
        ] {
            assert_ne!(
                react_helper_source(n),
                PythsRuntime,
                "{} must NOT route to pyths-runtime/react (it does not export it)",
                n
            );
        }
        for n in runtime_ok {
            assert_eq!(react_helper_source(n), PythsRuntime, "{}", n);
        }
    }

    #[test]
    fn test_react_19_removed_symbols_flagged() {
        // B5 (class rule): EVERY React-19 removal must be flagged under BOTH
        // spellings, with a message that names the JS symbol and dates the
        // removal. The full set, verified against the real react@19 /
        // react-dom@19 packages: findDOMNode, render, hydrate,
        // unmountComponentAtNode (react-dom) + createFactory (react).
        let expected: &[(&str, &str)] = &[
            ("find_dom_node", "findDOMNode"),
            ("render", "render"),
            ("hydrate", "hydrate"),
            ("unmount_component_at_node", "unmountComponentAtNode"),
            ("create_factory", "createFactory"),
        ];
        assert_eq!(
            REACT_19_REMOVED.len(),
            expected.len(),
            "REACT_19_REMOVED drifted from the audited set — update this test \
             AND re-verify against the real packages"
        );
        for (py, js) in expected {
            for n in [py, js] {
                let msg =
                    react_19_removed(n).unwrap_or_else(|| panic!("{n} must be flagged as removed"));
                assert!(msg.contains(js), "message names the symbol {js}: {msg}");
                assert!(msg.contains("React 19"), "message dates the removal: {msg}");
            }
        }
        // Every removed symbol has its would-be route in the table (so the
        // boundary test can assert genuine absence from that module).
        for e in REACT_19_REMOVED {
            assert!(
                REACT_HELPER_TABLE.iter().any(|(n, _)| *n == e.py_name),
                "{} must have a would-be route in REACT_HELPER_TABLE",
                e.py_name
            );
        }
        // Symbols that DO still exist in React 19 must NOT be flagged.
        for n in [
            "create_element",
            "clone_element",
            "is_valid_element",
            "create_portal",
            "flush_sync",
            "create_root",
            "hydrate_root",
            "use_state",
            "use_form_status",
            "use_optimistic",
            "act",
            "cache",
        ] {
            assert!(
                react_19_removed(n).is_none(),
                "{n} is a live React-19 export and must NOT be flagged as removed"
            );
        }
    }

    #[test]
    fn test_react_helper_table_has_no_duplicates() {
        // The table is the single audited surface — a duplicate key would make
        // the routing depend on entry order.
        for (i, (n, _)) in REACT_HELPER_TABLE.iter().enumerate() {
            assert!(
                !REACT_HELPER_TABLE[i + 1..].iter().any(|(m, _)| m == n),
                "duplicate REACT_HELPER_TABLE entry: {n}"
            );
        }
    }

    #[test]
    fn test_core_react_module_gate() {
        use ReactHelperSource::*;
        assert_eq!(core_react_module("react"), Some(ReactCore));
        assert_eq!(core_react_module("react_dom"), Some(ReactDom));
        assert_eq!(core_react_module("react-dom"), Some(ReactDom));
        assert_eq!(core_react_module("react_dom.client"), Some(ReactDomClient));
        // Ecosystem packages are NOT core — the removed-import diagnostic and
        // member module-check must not fire for them (`render` is a real
        // @testing-library/react export).
        for m in [
            "at_testing_library.react",
            "react_markdown",
            "react_router_dom",
            "zustand",
            "react_dom.server",
            "pyths.react",
        ] {
            assert!(core_react_module(m).is_none(), "{m} must not be core");
        }
    }

    #[test]
    fn test_route_namespace_member_matrix() {
        use ReactHelperSource::*;
        // Routed: known symbol on its home module — both spellings.
        for (base, member, js) in [
            (ReactCore, "use_state", "useState"),
            (ReactCore, "useState", "useState"),
            (ReactCore, "clone_element", "cloneElement"),
            (ReactCore, "create_element", "createElement"),
            (ReactCore, "createElement", "createElement"),
            (ReactCore, "Fragment", "Fragment"),
            (ReactDom, "create_portal", "createPortal"),
            (ReactDom, "createPortal", "createPortal"),
            (ReactDom, "flush_sync", "flushSync"),
            (ReactDom, "use_form_status", "useFormStatus"),
            (ReactDomClient, "create_root", "createRoot"),
            (ReactDomClient, "hydrate_root", "hydrateRoot"),
        ] {
            match route_namespace_member(base, member) {
                MemberRoute::Routed(got) => assert_eq!(got, js, "{member}"),
                _ => panic!("{member} on {base:?} must route"),
            }
        }
        // Removed: React-19 removals diagnose on ANY base, both spellings.
        for member in [
            "find_dom_node",
            "findDOMNode",
            "render",
            "hydrate",
            "unmount_component_at_node",
            "unmountComponentAtNode",
            "create_factory",
            "createFactory",
        ] {
            for base in [ReactCore, ReactDom, ReactDomClient] {
                assert!(
                    matches!(
                        route_namespace_member(base, member),
                        MemberRoute::Removed(_)
                    ),
                    "{member} on {base:?} must be Removed"
                );
            }
        }
        // WrongModule: known symbol reached through the wrong namespace —
        // the export genuinely does not exist there (silent undefined).
        for (base, member, home) in [
            (ReactCore, "create_portal", ReactDom),
            (ReactCore, "createPortal", ReactDom),
            (ReactCore, "create_root", ReactDomClient),
            (ReactDom, "use_state", ReactCore),
            (ReactDom, "create_element", ReactCore),
            (ReactDomClient, "create_portal", ReactDom),
            // the pyths meta-helpers are not React exports at all
            (ReactCore, "component", PythsRuntime),
            (ReactCore, "psx", PythsRuntime),
        ] {
            match route_namespace_member(base, member) {
                MemberRoute::WrongModule { exports_from, .. } => {
                    assert_eq!(exports_from, home, "{member}")
                }
                _ => panic!("{member} on {base:?} must be WrongModule"),
            }
        }
        // Unknown symbols: from-import parity — camel-cased, routed on any
        // base (no export inventory to check against).
        for (member, js) in [
            ("use_whatever_custom", "useWhateverCustom"),
            ("preload", "preload"),
            ("version", "version"),
        ] {
            for base in [ReactCore, ReactDom, ReactDomClient] {
                match route_namespace_member(base, member) {
                    MemberRoute::Routed(got) => assert_eq!(got, js),
                    _ => panic!("unknown {member} must route (from-import parity)"),
                }
            }
        }
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
