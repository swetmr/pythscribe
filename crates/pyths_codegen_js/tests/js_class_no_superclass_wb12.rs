//! WB-12 — `@js_class` opt-in emits a plain JS class with NO `extends
//! PyObject`, so foreign libraries that reject any class with a superclass
//! (MobX `makeAutoObservable`, which throws "can only be used for classes that
//! don't have a superclass") work. The DEFAULT class emission is unchanged
//! (`extends PyObject`, Python object semantics).
//!
//! Structural tests assert the `extends` slot; the behavioral test runs the
//! compiled class under node and applies MobX's real superclass guard
//! (`Object.getPrototypeOf(C.prototype) !== Object.prototype`), which is
//! exactly the check that rejected every `extends PyObject` class.

use std::process::Command;

fn compile_inline(source: &str) -> String {
    let module = pyths_parser::parse(source).expect("parse failed");
    pyths_codegen_js::codegen_inline(&module)
}

/// Run `js` under node from a scratch dir; None when node is unavailable.
fn node_run(js: &str, name: &str) -> Option<String> {
    let dir = std::env::temp_dir().join(format!("pyths_wb12_{}_{}", name, std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    pyths_runtime::materialize_runtime_package(&dir).expect("materialize runtime");
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

// ── structural ──────────────────────────────────────────────────────────────

#[test]
fn js_class_emits_plain_class_no_pyobject() {
    let js = compile_inline(
        "from pyths import js_class\n\
         @js_class\n\
         class Store:\n\
         \x20   def __init__(self):\n\
         \x20       self.n = 0\n",
    );
    assert!(
        js.contains("class Store {"),
        "@js_class must emit a plain `class Store {{`:\n{js}"
    );
    assert!(
        !js.contains("class Store extends"),
        "@js_class must NOT extend anything:\n{js}"
    );
    // The whole PyObject cooperative-MRO install is skipped for a @js_class.
    assert!(
        !js.contains("__pyClass(Store"),
        "@js_class must skip the cooperative-MRO __pyClass install:\n{js}"
    );
}

#[test]
fn js_class_init_becomes_constructor() {
    let js = compile_inline(
        "from pyths import js_class\n\
         @js_class\n\
         class Store:\n\
         \x20   def __init__(self):\n\
         \x20       self.n = 0\n\
         \x20   def inc(self):\n\
         \x20       self.n = self.n + 1\n",
    );
    // Scope the checks to the emitted Store class (the inlined runtime prelude
    // contains other classes with their own `super()` calls).
    let store = &js[js.find("class Store {").expect("Store class emitted")..];
    assert!(
        store.contains("constructor(") && store.contains("this.n = 0"),
        "@js_class __init__ must lower to a plain JS constructor:\n{js}"
    );
    assert!(
        !store.contains("super("),
        "a no-base @js_class constructor must NOT synthesize super():\n{js}"
    );
}

#[test]
fn normal_class_still_extends_pyobject() {
    // Regression guard: the default (no decorator) keeps `extends PyObject`.
    let js = compile_inline(
        "class Store:\n\
         \x20   def __init__(self):\n\
         \x20       self.n = 0\n",
    );
    assert!(
        js.contains("class Store extends PyObject"),
        "a normal class must still extend PyObject:\n{js}"
    );
}

// ── behavioral: MobX's superclass guard + methods work ───────────────────────

#[test]
fn js_class_passes_mobx_superclass_guard_and_runs() {
    let mut js = compile_inline(
        "from pyths import js_class\n\
         @js_class\n\
         class Store:\n\
         \x20   def __init__(self):\n\
         \x20       self.n = 0\n\
         \x20   def inc(self):\n\
         \x20       self.n = self.n + 1\n\
         def run():\n\
         \x20   s = Store()\n\
         \x20   s.inc()\n\
         \x20   s.inc()\n\
         \x20   return s.n\n\
         print(run())\n",
    );
    // MobX's actual guard: makeAutoObservable throws unless the class has NO
    // superclass. Reproduce it verbatim against the compiled class.
    js.push_str(
        "\nif (Object.getPrototypeOf(Store.prototype) !== Object.prototype) {\n\
         \x20 throw new Error(\"[MobX] makeAutoObservable can only be used for classes that don't have a superclass\");\n\
         }\nconsole.log(\"mobx-guard-ok\");\n",
    );
    let Some(got) = node_run(&js, "guard") else {
        return; // node unavailable → skip
    };
    let lines: Vec<&str> = got.trim().lines().collect();
    assert_eq!(lines[0], "2", "@js_class methods must work:\n{got}");
    assert_eq!(
        lines[1], "mobx-guard-ok",
        "@js_class must pass MobX's no-superclass guard:\n{got}"
    );
}
