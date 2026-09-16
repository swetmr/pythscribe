//! #491 name-binding soundness — a user `def` SHADOWS the builtin (and any math
//! import alias) of the same name, matching CPython. These tests pin the WASM
//! backend's call-classification so a user shadow WINS in every path. Each is a
//! paired control: it passes with the fix and FAILS on revert (the builtin / math
//! alias answer differs from the user function's).
//!
//! The differential net (`tests/differential/wasm_net`, `shadow*` families) covers
//! the end-to-end value + demotion behavior; this file pins the two cases the net
//! cannot express cleanly as a green row:
//!   - the ELIGIBLE math-alias-shadow EMIT reorder (a `from math import X as N` alias
//!     must lose to a later user `def N` in the emitted WASM) — the net's glue arm
//!     trips a separate, pre-existing JS-glue double-declaration collision there;
//!   - the direct raw-WASM value of an eligible builtin-name shadow.

use pyths_codegen_wasm::codegen_wasm;

fn compile_wasm(source: &str) -> Vec<u8> {
    let module = pyths_parser::parse(source).expect("Parse failed");
    let output = codegen_wasm(&module);
    assert!(
        !output.wasm.is_empty(),
        "Expected WASM output, but got none. Rejected: {:?}",
        output.rejected_functions
    );
    output.wasm
}

fn call_i64_i64_i64(wasm: &[u8], func_name: &str, a: i64, b: i64) -> i64 {
    let engine = wasmi::Engine::default();
    let module = wasmi::Module::new(&engine, wasm).expect("Failed to load WASM module");
    let mut store = wasmi::Store::new(&engine, ());
    let linker = <wasmi::Linker<()>>::new(&engine);
    let instance = linker
        .instantiate(&mut store, &module)
        .expect("Failed to instantiate")
        .start(&mut store)
        .expect("Failed to start");
    let func = instance
        .get_typed_func::<(i64, i64), i64>(&store, func_name)
        .expect("Failed to get function");
    func.call(&mut store, (a, b)).expect("Call failed")
}

fn call_i64_i64(wasm: &[u8], func_name: &str, arg: i64) -> i64 {
    let engine = wasmi::Engine::default();
    let module = wasmi::Module::new(&engine, wasm).expect("Failed to load WASM module");
    let mut store = wasmi::Store::new(&engine, ());
    let linker = <wasmi::Linker<()>>::new(&engine);
    let instance = linker
        .instantiate(&mut store, &module)
        .expect("Failed to instantiate")
        .start(&mut store)
        .expect("Failed to start");
    let func = instance
        .get_typed_func::<i64, i64>(&store, func_name)
        .expect("Failed to get function");
    func.call(&mut store, arg).expect("Call failed")
}

/// Call an (i64,i64)->i64 export in a module that also carries an (unused) `math.sqrt`
/// import. The sqrt stub PANICS if called: with the fix a user `def` shadowing the
/// math alias must be emitted as the user function and NEVER touch the import, so the
/// stub is dead code; on revert the alias is emitted → sqrt is called → the stub panics
/// (or the answer diverges) → the test fails (the paired negative control).
fn call_i64_i64_i64_with_math(wasm: &[u8], func_name: &str, a: i64, b: i64) -> i64 {
    let engine = wasmi::Engine::default();
    let module = wasmi::Module::new(&engine, wasm).expect("Failed to load WASM module");
    let mut store = wasmi::Store::new(&engine, ());
    let mut linker = <wasmi::Linker<()>>::new(&engine);
    linker
        .func_wrap(
            "math",
            "sqrt",
            |_caller: wasmi::Caller<'_, ()>, _x: f64| -> f64 {
                panic!("math.sqrt was called — a user `def` shadowing the alias must win (#491)")
            },
        )
        .expect("define math.sqrt");
    let instance = linker
        .instantiate(&mut store, &module)
        .expect("Failed to instantiate")
        .start(&mut store)
        .expect("Failed to start");
    let func = instance
        .get_typed_func::<(i64, i64), i64>(&store, func_name)
        .expect("Failed to get function");
    func.call(&mut store, (a, b)).expect("Call failed")
}

/// Blocker 3 (emit reorder): `from math import sqrt as abs` must NOT win over a
/// later user `def abs`. The user function (x*y) is emitted, NOT the math alias
/// `sqrt` (which would drop the 2nd arg and return sqrt(x)). On revert, emit_call
/// consults `math_aliases` before the user-function lookup → emits sqrt → the
/// sqrt import stub panics / the value diverges (RED).
#[test]
fn math_alias_shadowed_by_user_def() {
    let src = "\
from math import sqrt as abs

def abs(x: int, y: int) -> int:
    return x * y

def use_alias_abs(x: int, y: int) -> int:
    return abs(x, y)
";
    let wasm = compile_wasm(src);
    // The user `def abs` wins: 3*4 = 12, NOT sqrt(3) ~= 1.73 (arg dropped).
    assert_eq!(call_i64_i64_i64_with_math(&wasm, "abs", 3, 4), 12);
    assert_eq!(call_i64_i64_i64_with_math(&wasm, "use_alias_abs", 6, 5), 30);
    assert_eq!(call_i64_i64_i64_with_math(&wasm, "abs", 7, 0), 0);
}

/// A user `def abs` shadowing the BUILTIN abs is emitted as the user function
/// (arg2 kept, user semantics), not the builtin abs (which would drop arg2 and
/// return |x|). Direct raw-WASM value. On revert → builtin abs → RED.
#[test]
fn builtin_abs_shadowed_by_user_def() {
    let src = "\
def abs(x: int, y: int) -> int:
    return x * y - 1

def use_abs(x: int, y: int) -> int:
    return abs(x, y)
";
    let wasm = compile_wasm(src);
    // user abs: 3*4-1 = 11, NOT builtin |3| = 3.
    assert_eq!(call_i64_i64_i64(&wasm, "abs", 3, 4), 11);
    assert_eq!(call_i64_i64_i64(&wasm, "use_abs", 5, 5), 24);
}

/// A user `def len` shadowing the builtin `len` returns the user value, not the
/// builtin length. Scalar-arg direct call. On revert → builtin len (0 for a
/// non-pointer i64) → RED.
#[test]
fn builtin_len_shadowed_by_user_def() {
    let src = "\
def len(x: int) -> int:
    return x + 1000

def use_len(x: int) -> int:
    return len(x)
";
    let wasm = compile_wasm(src);
    assert_eq!(call_i64_i64(&wasm, "use_len", 5), 1005);
    assert_eq!(call_i64_i64(&wasm, "len", 5), 1005);
}
