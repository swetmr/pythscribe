//! WB-16 — `obj.constructor.name` must return the bare class name, not
//! "bound <Class>". `.constructor` is a CLASS reference, not a method; routing
//! it through the value-position method-binder (pyBoundMethod) `.bind()`-wrapped
//! it, and a bound function's `.name` is "bound <Class>". Root fix: pyBoundMethod
//! returns `.constructor` RAW.

use std::process::Command;

fn compile_inline(source: &str) -> String {
    let module = pyths_parser::parse(source).expect("parse failed");
    pyths_codegen_js::codegen_inline(&module)
}

fn node_run(js: &str, name: &str) -> Option<String> {
    let dir = std::env::temp_dir().join(format!("pyths_wb16_{}_{}", name, std::process::id()));
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

#[test]
fn constructor_name_is_bare_class_name_under_node() {
    let js = compile_inline(
        "class Animal:\n    def __init__(self, n):\n        self.n = n\n\
         class Dog(Animal):\n    pass\n\
         d = Dog(\"rex\")\n\
         print(d.constructor.name)\n\
         print(type(d).__name__)\n\
         print(d.__class__.__name__)\n",
    );
    let Some(got) = node_run(&js, "ctor") else {
        return; // node unavailable → skip
    };
    let lines: Vec<&str> = got.trim().lines().collect();
    assert_eq!(
        lines[0], "Dog",
        "d.constructor.name must be the bare class name, not \"bound Dog\"\n--- JS ---\n{js}"
    );
    // The Python-idiom class-name reads must keep working (regression guard).
    assert_eq!(
        lines[1], "Dog",
        "type(d).__name__ regressed\n--- JS ---\n{js}"
    );
    assert_eq!(
        lines[2], "Dog",
        "d.__class__.__name__ regressed\n--- JS ---\n{js}"
    );
}

#[test]
fn getattr_constructor_name_is_bare_class_name_under_node() {
    // WB-16 RESIDUAL: the direct `.constructor` read was patched in
    // pyBoundMethod, but the getattr/hasattr path (__attrLookup) was NOT — so
    // `getattr(o, "constructor").name` bound the class and returned "bound A".
    let js = compile_inline(
        "class A:\n    def __init__(self):\n        self.x = 1\n\
         o = A()\n\
         print(getattr(o, \"constructor\").name)\n",
    );
    let Some(got) = node_run(&js, "getattr_ctor") else {
        return;
    };
    assert_eq!(
        got.trim(),
        "A",
        "getattr(o, \"constructor\").name must be the bare class name, not \"bound A\"\n--- JS ---\n{js}"
    );
}

#[test]
fn getattr_on_real_method_still_binds_under_node() {
    // Regression guard for the __attrLookup change: only `constructor` is
    // returned raw — a getattr on a REAL method must still bind its receiver.
    let js = compile_inline(
        "class C:\n    def __init__(self):\n        self.x = 42\n\
         \n    def get(self):\n        return self.x\n\
         c = C()\n\
         g = getattr(c, \"get\")\n\
         print(g())\n",
    );
    let Some(got) = node_run(&js, "getattr_method") else {
        return;
    };
    assert_eq!(
        got.trim(),
        "42",
        "getattr on a real method must keep binding its receiver\n--- JS ---\n{js}"
    );
}

#[test]
fn def_named_constructor_is_compile_error() {
    // WB-16 (naming soundness): a Python method literally named `constructor`
    // would silently become the JS class constructor → un-instantiable class.
    // It must be a hard compile diagnostic, not a silent miscompile.
    let module =
        pyths_parser::parse("class A:\n    def constructor(self):\n        return 1\n").unwrap();
    let mut gen = pyths_codegen_js::JsCodegen::new();
    gen.emit_module(&module);
    let errors = gen.take_errors();
    assert!(
        errors
            .iter()
            .any(|e| e.contains("constructor") && e.contains("un-instantiable")),
        "a method named `constructor` must be diagnosed: {errors:?}"
    );
}

#[test]
fn ordinary_bound_method_access_unchanged_under_node() {
    // A real bound method still binds `self` when read in value position.
    let js = compile_inline(
        "class C:\n    def __init__(self):\n        self.x = 9\n    def get(self):\n        return self.x\n\
         c = C()\n\
         g = c.get\n\
         print(g())\n",
    );
    let Some(got) = node_run(&js, "bound") else {
        return;
    };
    assert_eq!(
        got.trim(),
        "9",
        "value-position bound method must keep its receiver\n--- JS ---\n{js}"
    );
}
