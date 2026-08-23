//! WF-1 (0.2.2 batch 2) — cleanup-hook wrapping must survive a SPREAD call
//! and cover `use_sync_external_store`.
//!
//! `use_effect(*args)` emitted `useEffect(__pyEffect(...args))` — the wrap
//! swallowed the deps array into __pyEffect's parameter list, so React saw a
//! single-argument effect and RE-RAN IT EVERY RENDER. `use_sync_external_
//! store` wasn't wrapped at all (its `subscribe` return is the unsubscribe
//! cleanup — a Python `return None` crashes React's cleanup path).
//!
//! CLASS rule: when ANY positional arg of a cleanup hook is a spread, route
//! through the runtime splitter `useEffect(...__pyEffectArgs(...args))`,
//! which wraps ONLY the runtime-resolved FIRST argument and passes the rest
//! (deps / getSnapshot) through. The non-spread form keeps its compile-time
//! wrap; `use_sync_external_store` joins the cleanup-hook set.
//!
//! Behavioral tests stub the react module (records call shapes) — skipped
//! when node is unavailable.

use std::process::Command;

fn compile_inline(source: &str) -> String {
    let module = pyths_parser::parse(source).expect("parse failed");
    pyths_codegen_js::codegen_inline(&module)
}

/// Stub react: `useEffect`/`useLayoutEffect`/`useSyncExternalStore` record
/// how many args arrive and exercise the effect + its cleanup exactly the
/// way React does (call the callback, then call its return if non-nullish).
const REACT_STUB: &str = r#"
export function useEffect(effect, deps) {
    console.log("useEffect argc=" + arguments.length + " deps=" + (deps === undefined ? "MISSING" : JSON.stringify(deps)));
    const cleanup = effect();
    if (cleanup !== undefined && cleanup !== null) cleanup();
    console.log("cleanup=" + (cleanup === undefined ? "undefined" : cleanup === null ? "null" : typeof cleanup));
}
export const useLayoutEffect = useEffect;
export function useSyncExternalStore(subscribe, getSnapshot) {
    const unsub = subscribe(() => {});
    if (unsub !== undefined && unsub !== null) unsub();
    console.log("uSES unsub=" + (unsub === undefined ? "undefined" : unsub === null ? "null" : typeof unsub) + " snap=" + getSnapshot());
    return getSnapshot();
}
"#;

fn node_run(js: &str, name: &str) -> Option<String> {
    let dir = std::env::temp_dir().join(format!("pyths_wf1_{}_{}", name, std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    pyths_runtime::materialize_runtime_package(&dir).expect("materialize runtime");
    let rdir = dir.join("node_modules").join("react");
    std::fs::create_dir_all(&rdir).unwrap();
    std::fs::write(
        rdir.join("package.json"),
        r#"{"name":"react","version":"0.0.0","type":"module","main":"index.js"}"#,
    )
    .unwrap();
    std::fs::write(rdir.join("index.js"), REACT_STUB).unwrap();
    let path = dir.join(format!("{name}.mjs"));
    std::fs::write(&path, js).unwrap();
    let out = Command::new("node").arg(&path).output().ok()?;
    let stdout = String::from_utf8_lossy(&out.stdout).to_string();
    let stderr = String::from_utf8_lossy(&out.stderr).to_string();
    let _ = std::fs::remove_dir_all(&dir);
    assert!(
        out.status.success(),
        "node failed on {name}:\n{stderr}\n--- JS ---\n{js}"
    );
    Some(stdout)
}

fn assert_behaves(src: &str, expected: &str, name: &str) {
    let js = compile_inline(src);
    let Some(got) = node_run(&js, name) else {
        return; // node unavailable → skip
    };
    assert_eq!(got.trim(), expected, "{name}\n--- JS ---\n{js}");
}

// ── the deps array must survive ─────────────────────────────────────────────

#[test]
fn plain_two_arg_form_unchanged() {
    // Compile-time wrap: both args reach useEffect; None return coerced.
    assert_behaves(
        "from react import use_effect\n\
         def comp():\n    use_effect(lambda: None, [1])\n\
         comp()\n",
        "useEffect argc=2 deps=[1]\ncleanup=undefined",
        "plain",
    );
}

#[test]
fn spread_form_preserves_deps() {
    // THE reported bug: the deps array must arrive as useEffect's 2nd arg.
    assert_behaves(
        "from react import use_effect\n\
         def comp():\n\
         \x20   args = [lambda: None, [1, 2]]\n\
         \x20   use_effect(*args)\n\
         comp()\n",
        "useEffect argc=2 deps=[1,2]\ncleanup=undefined",
        "spread",
    );
}

#[test]
fn spread_form_single_element() {
    // A one-element spread: no deps — argc must be 1, wrap still applies.
    assert_behaves(
        "from react import use_effect\n\
         def comp():\n\
         \x20   args = [lambda: None]\n\
         \x20   use_effect(*args)\n\
         comp()\n",
        "useEffect argc=1 deps=MISSING\ncleanup=undefined",
        "spread_one",
    );
}

#[test]
fn mixed_fn_then_spread_rest() {
    // First arg explicit, rest spread — still routed through the splitter
    // (wrap applies to the resolved first arg), deps survive.
    assert_behaves(
        "from react import use_effect\n\
         def comp():\n\
         \x20   rest = [[3]]\n\
         \x20   use_effect(lambda: None, *rest)\n\
         comp()\n",
        "useEffect argc=2 deps=[3]\ncleanup=undefined",
        "mixed",
    );
}

#[test]
fn real_cleanup_function_still_runs() {
    // A genuine cleanup must pass through the wrap untouched (both forms).
    assert_behaves(
        "from react import use_effect\n\
         def comp():\n\
         \x20   def eff():\n\
         \x20       return lambda: print(\"cleaned\")\n\
         \x20   args = [eff, []]\n\
         \x20   use_effect(*args)\n\
         comp()\n",
        "useEffect argc=2 deps=[]\ncleaned\ncleanup=function",
        "cleanup_fn",
    );
}

// ── use_sync_external_store joins the cleanup-hook set ──────────────────────

#[test]
fn sync_external_store_subscribe_none_coerced() {
    // Python subscribe returning None → undefined (not null): React's
    // cleanup path must not see null.
    assert_behaves(
        "from react import use_sync_external_store\n\
         def comp():\n\
         \x20   return use_sync_external_store(lambda cb: None, lambda: 42)\n\
         print(comp())\n",
        "uSES unsub=undefined snap=42\n42",
        "uses_none",
    );
}

#[test]
fn sync_external_store_spread_form() {
    assert_behaves(
        "from react import use_sync_external_store\n\
         def comp():\n\
         \x20   args = [lambda cb: None, lambda: 7]\n\
         \x20   return use_sync_external_store(*args)\n\
         print(comp())\n",
        "uSES unsub=undefined snap=7\n7",
        "uses_spread",
    );
}

// ── structural guards ───────────────────────────────────────────────────────

#[test]
fn spread_routes_through_splitter() {
    let js = compile_inline(
        "from react import use_effect\ndef comp():\n    args = [lambda: None, []]\n    use_effect(*args)\n",
    );
    assert!(
        js.contains("useEffect(...__pyEffectArgs(...args))"),
        "spread form must route through the runtime splitter:\n{js}"
    );
}

#[test]
fn non_cleanup_hooks_never_wrapped() {
    // use_memo's return is DATA — no wrap, no splitter.
    let js = compile_inline(
        "from react import use_memo\ndef comp():\n    return use_memo(lambda: 1, [])\n",
    );
    assert!(
        !js.contains("__pyEffect"),
        "value-returning hooks must not be wrapped:\n{js}"
    );
}
