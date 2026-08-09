//! #364 — WASM soundness under explicit `--target js+wasm`: the fallback
//! ladder + data-informed eligibility calibration.
//!
//! These exercise the codegen/analysis half of the #364 wave directly (the
//! JS-glue half — the runtime try/catch + `__pyparams__` kwarg binding — is
//! covered by the real-code forced-WASM differential in the reference-app repo,
//! which this wave drove from 29 miscompiles to 0). Each test pins one of the
//! four root causes the differential surfaced:
//!
//!   1. Augmented assignment to a SUBSCRIPT target (`t[i] += v`) silently
//!      no-op'd — the emitter only handled `Name` targets. Now desugared to
//!      `t[i] = t[i] <op> v` (checkArray, sample_106/109).
//!   2. Reverse `range(n-1, -1, -1)` (negative step) iterated zero times — the
//!      loop test was a hardcoded `i < stop`. Now sign-specialized
//!      (minOperations, sample_421).
//!   3. A statically-negative subscript index (`a[-1]`) silently addressed the
//!      wrong slot — now EXCLUDED from admission (stays correct JS; cluster02).
//!   4. A function whose lowering emits INVALID WASM (`ans += x < y`, an i32
//!      bool stored into an i64 slot) is caught at compile time and demoted to
//!      JS, so the module always instantiates (cluster03).

use pyths_codegen_wasm::codegen_wasm;
use wasmi::{Engine, Linker, Module, Store};

fn compile(source: &str) -> pyths_codegen_wasm::WasmCodegenOutput {
    let module = pyths_parser::parse(source).expect("parse failed");
    codegen_wasm(&module)
}

fn compile_valid_wasm(source: &str) -> Vec<u8> {
    let out = compile(source);
    assert!(
        !out.wasm.is_empty(),
        "expected WASM output; rejected: {:?}",
        out.rejected_functions
    );
    wasmparser::validate(&out.wasm).expect("emitted WASM must validate");
    out.wasm
}

/// Call an `(i64) -> i64` export and return its result.
fn call1(wasm: &[u8], name: &str, a: i64) -> i64 {
    let engine = Engine::default();
    let module = Module::new(&engine, wasm).expect("load module");
    let mut store = Store::new(&engine, ());
    let mut linker = <Linker<()>>::new(&engine);
    let host = wasmi::Func::wrap(&mut store, |a: f64, b: f64| -> f64 { a.powf(b) });
    linker.define("math", "pow", host).expect("define math.pow");
    let instance = linker
        .instantiate(&mut store, &module)
        .expect("instantiate")
        .start(&mut store)
        .expect("start");
    let func = instance
        .get_typed_func::<i64, i64>(&store, name)
        .expect("get func");
    func.call(&mut store, a).expect("call")
}

// 1. Augmented subscript store — previously a silent no-op.
#[test]
fn aug_subscript_store_is_not_a_noop() {
    let src =
        "def f(j: int) -> int:\n    t = [0] * 5\n    t[j] += 9\n    t[j] -= 2\n    return t[j]\n";
    let wasm = compile_valid_wasm(src);
    assert_eq!(call1(&wasm, "f", 2), 7, "t[2] += 9; t[2] -= 2 must yield 7");
}

// 2. Reverse range (negative step) — previously zero iterations.
#[test]
fn reverse_range_iterates() {
    let src = "def f(n: int) -> int:\n    s = 0\n    for i in range(n - 1, -1, -1):\n        s += i\n    return s\n";
    let wasm = compile_valid_wasm(src);
    assert_eq!(call1(&wasm, "f", 5), 10, "sum(4,3,2,1,0) = 10");
}

#[test]
fn reverse_range_step_neg2_iterates() {
    let src = "def f(n: int) -> int:\n    s = 0\n    for i in range(n, 0, -2):\n        s += i\n    return s\n";
    let wasm = compile_valid_wasm(src);
    assert_eq!(call1(&wasm, "f", 6), 12, "sum(6,4,2) = 12");
}

#[test]
fn ascending_range_still_iterates() {
    let src =
        "def f(n: int) -> int:\n    s = 0\n    for i in range(n):\n        s += i\n    return s\n";
    let wasm = compile_valid_wasm(src);
    assert_eq!(
        call1(&wasm, "f", 5),
        10,
        "ascending path must be unaffected"
    );
}

// 3. Statically-negative subscript index — excluded from admission.
#[test]
fn negative_literal_index_is_excluded() {
    let src = "def f(n: int) -> int:\n    a = [0] * n\n    a[-1] = 7\n    return a[n - 1]\n";
    let out = compile(src);
    assert!(
        !out.compiled_functions.iter().any(|n| n == "f"),
        "a[-1] must not be admitted to WASM (stays correct JS)"
    );
    assert!(
        out.rejected_functions.iter().any(|(n, _)| n == "f"),
        "f must appear in rejected_functions"
    );
}

// 4. A function whose lowering emits invalid WASM is demoted to JS at compile
// time; the module still validates, and co-admitted good functions survive.
#[test]
fn invalid_wasm_function_is_demoted_but_module_validates() {
    let src = "def countPairs(nums: list[int], target: int) -> int:\n    ans = 0\n    for i in range(len(nums)):\n        for j in range(i + 1, len(nums)):\n            ans += nums[i] + nums[j] < target\n    return ans\ndef good(x: int) -> int:\n    return x * 2\n";
    let out = compile(src);
    // The un-lowerable function is demoted...
    assert!(
        !out.compiled_functions.iter().any(|n| n == "countPairs"),
        "countPairs emits invalid WASM and must be demoted to JS"
    );
    // ...the good one stays admitted...
    assert!(
        out.compiled_functions.iter().any(|n| n == "good"),
        "the valid co-admitted function must remain on the WASM path"
    );
    // ...and whatever WASM ships must instantiate.
    assert!(!out.wasm.is_empty());
    wasmparser::validate(&out.wasm).expect("post-fallback module must validate");
    assert_eq!(call1(&out.wasm, "good", 21), 42);
}
