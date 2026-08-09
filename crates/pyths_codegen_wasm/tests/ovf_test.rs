//! #358 — overflow-checked WASM integer arithmetic + `__ovf` flag semantics.
//!
//! Before this fix the WASM fast path lowered `int` arithmetic to bare
//! `i64.*` instructions, so any product / sum / shift whose exact result
//! exceeded i64 wrapped SILENTLY — a wrong value on an explicit
//! `--target js+wasm` (e.g. `4294967296 * 4294967297` returned
//! `4294967296` instead of `18446744078004518912`).
//!
//! The fix makes every admitted i64 operation *checked*: it either leaves
//! the exact value on the stack, or — when the exact result is not
//! representable in i64 (or the op is inexact for WASM, e.g. a negative
//! shift count, `-i64::MIN`, `abs(i64::MIN)`) — it sets the exported
//! `__ovf` i32 global to 1. The JS glue reads `__ovf` after every call and
//! transparently re-runs the call on the exact arbitrary-precision JS twin
//! (or throws where no twin exists). These tests exercise the WASM half of
//! that contract directly: for every op, an in-range call yields the exact
//! value with `__ovf == 0`, and an out-of-range call sets `__ovf == 1`.
//!
//! The soundness property under test: **no admitted i64 op ever leaves an
//! inexact value on the stack without also setting `__ovf`.** (The glue's
//! job — acting on the flag — is covered by the JS-side differential
//! tests; here we prove the flag is raised exactly when it must be.)

use pyths_codegen_wasm::codegen_wasm;
use wasmi::{Engine, Linker, Module, Store, Val};

fn compile_wasm(source: &str) -> Vec<u8> {
    let module = pyths_parser::parse(source).expect("parse failed");
    let output = codegen_wasm(&module);
    assert!(
        !output.wasm.is_empty(),
        "expected WASM output, but got none. Rejected: {:?}",
        output.rejected_functions
    );
    assert!(
        output.has_ovf,
        "#358: has_ovf must be set for a non-empty module"
    );
    wasmparser::validate(&output.wasm).expect("WASM validation failed");
    output.wasm
}

/// Instantiate `wasm`, call the `(i64, i64) -> i64` export `name`, and
/// return `(result, ovf_flag)`. A fresh instance per call means `__ovf`
/// starts at 0, so a non-zero flag was raised by *this* call.
fn call2(wasm: &[u8], name: &str, a: i64, b: i64) -> (i64, i32) {
    let engine = Engine::default();
    let module = Module::new(&engine, wasm).expect("load module");
    let mut store = Store::new(&engine, ());
    let mut linker = <Linker<()>>::new(&engine);
    define_math_pow(&mut linker, &mut store);
    let instance = linker
        .instantiate(&mut store, &module)
        .expect("instantiate")
        .start(&mut store)
        .expect("start");
    let func = instance
        .get_typed_func::<(i64, i64), i64>(&store, name)
        .expect("get func");
    let r = func.call(&mut store, (a, b)).expect("call");
    let ovf = read_ovf(&instance, &mut store);
    (r, ovf)
}

/// As `call2`, for a `(i64) -> i64` export.
fn call1(wasm: &[u8], name: &str, a: i64) -> (i64, i32) {
    let engine = Engine::default();
    let module = Module::new(&engine, wasm).expect("load module");
    let mut store = Store::new(&engine, ());
    let mut linker = <Linker<()>>::new(&engine);
    define_math_pow(&mut linker, &mut store);
    let instance = linker
        .instantiate(&mut store, &module)
        .expect("instantiate")
        .start(&mut store)
        .expect("start");
    let func = instance
        .get_typed_func::<i64, i64>(&store, name)
        .expect("get func");
    let r = func.call(&mut store, a).expect("call");
    let ovf = read_ovf(&instance, &mut store);
    (r, ovf)
}

/// The Pow-operator lowering still registers the `math.pow` f64 import even
/// for the int-pow path (it never calls it), so wasmi needs the import
/// satisfied at instantiation. Defining it unconditionally is harmless for
/// modules that don't import it.
fn define_math_pow(linker: &mut Linker<()>, store: &mut Store<()>) {
    let host = wasmi::Func::wrap(&mut *store, move |a: f64, b: f64| -> f64 { a.powf(b) });
    linker.define("math", "pow", host).expect("define math.pow");
}

fn read_ovf(instance: &wasmi::Instance, store: &mut Store<()>) -> i32 {
    let g = instance
        .get_global(&mut *store, "__ovf")
        .expect("#358: module must export __ovf");
    match g.get(&mut *store) {
        Val::I32(v) => v,
        other => panic!("__ovf must be i32, got {:?}", other),
    }
}

// ===========================================================================
// Multiplication — the original #358 repro.
// ===========================================================================

#[test]
fn mul_in_range_exact_no_flag() {
    let wasm = compile_wasm("def f(a: int, b: int) -> int:\n    return a * b\n");
    let (r, ovf) = call2(&wasm, "f", 1_000_000, 2_000_000);
    assert_eq!(r, 2_000_000_000_000);
    assert_eq!(ovf, 0, "in-range product must not flag");
}

#[test]
fn mul_overflow_flags_the_exact_issue_values() {
    let wasm = compile_wasm("def f(a: int, b: int) -> int:\n    return a * b\n");
    // 4294967296 * 4294967297 = 18446744078004518912 > i64::MAX → flag.
    let (_r, ovf) = call2(&wasm, "f", 4_294_967_296, 4_294_967_297);
    assert_eq!(ovf, 1, "product exceeding i64 must set __ovf");
    // 3037000500 * 3037000501 = 9223372040037250500 > i64::MAX → flag.
    let (_r2, ovf2) = call2(&wasm, "f", 3_037_000_500, 3_037_000_501);
    assert_eq!(ovf2, 1, "just-over-2^63 product must set __ovf");
}

#[test]
fn mul_just_below_boundary_is_exact() {
    let wasm = compile_wasm("def f(a: int, b: int) -> int:\n    return a * b\n");
    // 3037000499^2 = 9223372030926249001 < i64::MAX → exact, no flag.
    let (r, ovf) = call2(&wasm, "f", 3_037_000_499, 3_037_000_499);
    assert_eq!(r, 9_223_372_030_926_249_001);
    assert_eq!(ovf, 0);
}

#[test]
fn mul_negative_min_by_neg_one_flags() {
    let wasm = compile_wasm("def f(a: int, b: int) -> int:\n    return a * b\n");
    // i64::MIN * -1 overflows (the only trap-condition the div-check must
    // special-case).
    let (_r, ovf) = call2(&wasm, "f", i64::MIN, -1);
    assert_eq!(ovf, 1);
    // i64::MIN * 1 is exact.
    let (r, ovf2) = call2(&wasm, "f", i64::MIN, 1);
    assert_eq!(r, i64::MIN);
    assert_eq!(ovf2, 0);
    // anything * 0 is exact.
    let (r0, ovf3) = call2(&wasm, "f", i64::MIN, 0);
    assert_eq!(r0, 0);
    assert_eq!(ovf3, 0);
}

// ===========================================================================
// Addition / subtraction — sign-bit overflow check.
// ===========================================================================

#[test]
fn add_overflow_flags() {
    let wasm = compile_wasm("def f(a: int, b: int) -> int:\n    return a + b\n");
    let (r, ovf) = call2(&wasm, "f", 100, 200);
    assert_eq!(r, 300);
    assert_eq!(ovf, 0);
    let (_r, ovf2) = call2(&wasm, "f", i64::MAX, 1);
    assert_eq!(ovf2, 1, "i64::MAX + 1 must flag");
    let (r3, ovf3) = call2(&wasm, "f", i64::MAX, 0);
    assert_eq!(r3, i64::MAX);
    assert_eq!(ovf3, 0);
}

#[test]
fn sub_overflow_flags() {
    let wasm = compile_wasm("def f(a: int, b: int) -> int:\n    return a - b\n");
    let (r, ovf) = call2(&wasm, "f", 5, 8);
    assert_eq!(r, -3);
    assert_eq!(ovf, 0);
    let (_r, ovf2) = call2(&wasm, "f", i64::MIN, 1);
    assert_eq!(ovf2, 1, "i64::MIN - 1 must flag");
}

// ===========================================================================
// Shift-left — grows without bound in Python; masks mod 64 in WASM.
// ===========================================================================

#[test]
fn shl_in_range_no_flag() {
    let wasm = compile_wasm("def f(a: int, b: int) -> int:\n    return a << b\n");
    let (r, ovf) = call2(&wasm, "f", 1, 62);
    assert_eq!(r, 1i64 << 62);
    assert_eq!(ovf, 0);
}

#[test]
fn shl_past_i64_flags() {
    let wasm = compile_wasm("def f(a: int, b: int) -> int:\n    return a << b\n");
    // 1 << 63 = 2^63 > i64::MAX → flag (bits shifted out of the value).
    let (_r, ovf) = call2(&wasm, "f", 1, 63);
    assert_eq!(ovf, 1, "1 << 63 must flag");
    // 1 << 70 (count >= 64, operand != 0) → flag.
    let (_r2, ovf2) = call2(&wasm, "f", 1, 70);
    assert_eq!(ovf2, 1, "1 << 70 must flag");
    // 0 << 70 → 0, no flag (nothing to lose).
    let (r3, ovf3) = call2(&wasm, "f", 0, 70);
    assert_eq!(r3, 0);
    assert_eq!(ovf3, 0);
}

#[test]
fn shl_negative_count_flags() {
    let wasm = compile_wasm("def f(a: int, b: int) -> int:\n    return a << b\n");
    // Python raises ValueError on a negative shift; the exact JS twin
    // reproduces that, so the WASM path flags.
    let (_r, ovf) = call2(&wasm, "f", 1, -1);
    assert_eq!(ovf, 1, "negative shift count must flag");
}

// ===========================================================================
// Shift-right — Python saturates (>=64 → 0 / -1); WASM masks mod 64.
// ===========================================================================

#[test]
fn shr_normal_and_saturating_are_exact() {
    let wasm = compile_wasm("def f(a: int, b: int) -> int:\n    return a >> b\n");
    let (r, ovf) = call2(&wasm, "f", -8, 1);
    assert_eq!(r, -4);
    assert_eq!(ovf, 0);
    // Count clamped to 63: an arithmetic shift saturates to the sign bit,
    // matching Python (no wrap from the mod-64 masking), no flag.
    let (r2, ovf2) = call2(&wasm, "f", -8, 100);
    assert_eq!(r2, -1);
    assert_eq!(ovf2, 0);
    let (r3, ovf3) = call2(&wasm, "f", 8, 100);
    assert_eq!(r3, 0);
    assert_eq!(ovf3, 0);
}

#[test]
fn shr_negative_count_flags() {
    let wasm = compile_wasm("def f(a: int, b: int) -> int:\n    return a >> b\n");
    let (_r, ovf) = call2(&wasm, "f", 8, -1);
    assert_eq!(ovf, 1, "negative shift count must flag");
}

// ===========================================================================
// Floor division & modulo — Python floor/sign semantics, exact past 2^53.
// ===========================================================================

#[test]
fn floordiv_python_semantics_exact() {
    let wasm = compile_wasm("def f(a: int, b: int) -> int:\n    return a // b\n");
    // floor toward -inf: -7 // 2 == -4 (not -3).
    let (r, ovf) = call2(&wasm, "f", -7, 2);
    assert_eq!(r, -4);
    assert_eq!(ovf, 0);
    // Exact past 2^53 — the old f64 round-trip lost precision here.
    let (r2, ovf2) = call2(&wasm, "f", i64::MAX, 1);
    assert_eq!(r2, i64::MAX);
    assert_eq!(ovf2, 0);
    let (r3, ovf3) = call2(&wasm, "f", 9_223_372_036_854_775_807, 3);
    assert_eq!(r3, 9_223_372_036_854_775_807 / 3);
    assert_eq!(ovf3, 0);
}

#[test]
fn floordiv_min_by_neg_one_flags() {
    let wasm = compile_wasm("def f(a: int, b: int) -> int:\n    return a // b\n");
    // i64::MIN // -1 == 2^63 → not representable → flag (mirrors mul).
    let (_r, ovf) = call2(&wasm, "f", i64::MIN, -1);
    assert_eq!(ovf, 1);
}

#[test]
fn mod_python_semantics_exact() {
    let wasm = compile_wasm("def f(a: int, b: int) -> int:\n    return a % b\n");
    // Result takes the divisor's sign in Python.
    let (r, ovf) = call2(&wasm, "f", -7, 3);
    assert_eq!(r, 2);
    assert_eq!(ovf, 0);
    let (r2, ovf2) = call2(&wasm, "f", 7, -3);
    assert_eq!(r2, -2);
    assert_eq!(ovf2, 0);
    let (r3, ovf3) = call2(&wasm, "f", 10, 3);
    assert_eq!(r3, 1);
    assert_eq!(ovf3, 0);
}

// ===========================================================================
// Exact integer pow — square-and-multiply over checked muls.
// ===========================================================================

#[test]
fn pow_in_range_exact_no_flag() {
    let wasm = compile_wasm("def f(a: int, b: int) -> int:\n    return a ** b\n");
    let (r, ovf) = call2(&wasm, "f", 2, 10);
    assert_eq!(r, 1024);
    assert_eq!(ovf, 0);
    // 10^18 < i64::MAX → exact.
    let (r2, ovf2) = call2(&wasm, "f", 10, 18);
    assert_eq!(r2, 1_000_000_000_000_000_000);
    assert_eq!(ovf2, 0);
    // x^0 == 1, 0^0 == 1.
    let (r3, ovf3) = call2(&wasm, "f", 123456789, 0);
    assert_eq!(r3, 1);
    assert_eq!(ovf3, 0);
}

#[test]
fn pow_overflow_flags() {
    let wasm = compile_wasm("def f(a: int, b: int) -> int:\n    return a ** b\n");
    // 10^19 > i64::MAX → an intermediate checked-mul flags.
    let (_r, ovf) = call2(&wasm, "f", 10, 19);
    assert_eq!(ovf, 1, "10 ** 19 must flag");
    // 2^63 == i64::MIN+... actually 2^63 > i64::MAX → flag.
    let (_r2, ovf2) = call2(&wasm, "f", 2, 63);
    assert_eq!(ovf2, 1, "2 ** 63 must flag");
}

#[test]
fn pow_negative_exponent_flags() {
    let wasm = compile_wasm("def f(a: int, b: int) -> int:\n    return a ** b\n");
    // Negative exponent → the exact result is a float; the JS twin handles
    // it, so the WASM path flags.
    let (_r, ovf) = call2(&wasm, "f", 2, -1);
    assert_eq!(ovf, 1, "negative exponent must flag");
}

// ===========================================================================
// Unary neg / abs — i64::MIN edge cases.
// ===========================================================================

#[test]
fn neg_min_flags() {
    let wasm = compile_wasm("def f(a: int) -> int:\n    return -a\n");
    let (r, ovf) = call1(&wasm, "f", 5);
    assert_eq!(r, -5);
    assert_eq!(ovf, 0);
    // -i64::MIN == 2^63 → not representable → flag.
    let (_r, ovf2) = call1(&wasm, "f", i64::MIN);
    assert_eq!(ovf2, 1, "-i64::MIN must flag");
}

#[test]
fn abs_min_flags() {
    let wasm = compile_wasm("def f(a: int) -> int:\n    return abs(a)\n");
    let (r, ovf) = call1(&wasm, "f", -5);
    assert_eq!(r, 5);
    assert_eq!(ovf, 0);
    let (r2, ovf2) = call1(&wasm, "f", 5);
    assert_eq!(r2, 5);
    assert_eq!(ovf2, 0);
    // abs(i64::MIN) == 2^63 → not representable → flag.
    let (_r, ovf3) = call1(&wasm, "f", i64::MIN);
    assert_eq!(ovf3, 1, "abs(i64::MIN) must flag");
}

// ===========================================================================
// Sticky-flag / intermediate-overflow semantics.
// ===========================================================================

#[test]
fn intermediate_overflow_flags_even_when_final_result_fits() {
    // a*b overflows, but a*b - a*b == 0 fits i64. The checked mul raises
    // __ovf on the intermediate product; the flag is sticky, so the glue
    // still falls back to the exact twin (which computes 0 too). The point:
    // a genuinely-overflowing intermediate is NEVER silently accepted.
    let wasm = compile_wasm("def f(a: int, b: int) -> int:\n    return a * b - a * b\n");
    let (_r, ovf) = call2(&wasm, "f", 4_294_967_296, 4_294_967_297);
    assert_eq!(ovf, 1, "intermediate overflow must set the sticky flag");
    // With in-range operands the same function must not flag.
    let (r2, ovf2) = call2(&wasm, "f", 3, 4);
    assert_eq!(r2, 0);
    assert_eq!(ovf2, 0);
}

#[test]
fn flag_is_settable_and_resettable_on_one_instance() {
    // Directly model the glue's reset discipline: after a flagged call the
    // glue writes __ovf = 0, and the next in-range call must observe a
    // clean flag. We reuse a single instance and reset the global by hand.
    let wasm = compile_wasm("def f(a: int, b: int) -> int:\n    return a * b\n");
    let engine = Engine::default();
    let module = Module::new(&engine, &wasm).expect("load");
    let mut store = Store::new(&engine, ());
    let linker = <Linker<()>>::new(&engine);
    let instance = linker
        .instantiate(&mut store, &module)
        .expect("instantiate")
        .start(&mut store)
        .expect("start");
    let func = instance
        .get_typed_func::<(i64, i64), i64>(&store, "f")
        .expect("get func");

    // Clean start.
    assert_eq!(read_ovf(&instance, &mut store), 0);
    // In-range call: exact, no flag.
    let r = func.call(&mut store, (100, 200)).expect("call");
    assert_eq!(r, 20_000);
    assert_eq!(read_ovf(&instance, &mut store), 0);
    // Overflowing call: flag raised.
    let _ = func
        .call(&mut store, (4_294_967_296, 4_294_967_297))
        .expect("call");
    assert_eq!(read_ovf(&instance, &mut store), 1);
    // Glue resets the flag.
    let g = instance.get_global(&mut store, "__ovf").unwrap();
    g.set(&mut store, Val::I32(0)).expect("reset");
    // A subsequent in-range call leaves it clean (no leak from the prior).
    let r2 = func.call(&mut store, (7, 8)).expect("call");
    assert_eq!(r2, 56);
    assert_eq!(read_ovf(&instance, &mut store), 0);
}

// ===========================================================================
// Admission: int literals beyond i64 keep the function on the JS path.
// ===========================================================================

#[test]
fn oversize_int_literal_is_not_admitted_to_wasm() {
    // 10^20 > i64::MAX as a literal → the function must NOT be compiled to
    // WASM (it stays on the arbitrary-precision JS path). i64::MIN, written
    // as -9223372036854775808, must still be admitted (constant-folded).
    let src = "def big() -> int:\n    return 100000000000000000000\n\
               def edge() -> int:\n    return -9223372036854775808\n";
    let module = pyths_parser::parse(src).expect("parse");
    let output = codegen_wasm(&module);
    assert!(
        output
            .rejected_functions
            .iter()
            .any(|(name, _)| name.contains("big")),
        "oversize int literal must reject `big`: {:?}",
        output.rejected_functions
    );
    assert!(
        !output
            .rejected_functions
            .iter()
            .any(|(name, _)| name.contains("edge")),
        "i64::MIN literal must be admitted: {:?}",
        output.rejected_functions
    );
}
