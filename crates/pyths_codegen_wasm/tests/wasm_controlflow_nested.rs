//! B1 (0.2.2 tag-blocker): `break`/`continue`/early-`return` under an `if`
//! (or any nested structured block) on the DEFAULT auto-routed WASM target.
//!
//! The old emitter hardcoded `Br(1)` for break and `Br(0)` for continue,
//! assuming the statement sat DIRECTLY in the loop body. Any intervening
//! structured label (`if`, `try`, a nested loop) shifted the relative WASM
//! label depths, so the branch targeted the wrong block:
//!   - `while x < 5: if n > 0: break`  → infinite loop (br targeted the if)
//!   - `while ...: if x == 2: continue` → no-op continue (15 where CPython
//!     gives 13)
//! Additionally, `continue` in a `for` loop branched to the loop HEADER,
//! skipping the increment — an infinite loop even in the FLAT case.
//!
//! The fix is a label-depth stack: every statement-level structured-block
//! emitter tracks its labels in `FuncContext::block_depth`, loops push their
//! (break, continue) targets as ABSOLUTE label indices, and break/continue
//! compute the `br` relative depth at the branch site — correct at any
//! nesting level by construction. `for` bodies gain a `block $continue`
//! wrapper so continue still runs the increment. Early `return` is a WASM
//! `return` instruction (depth-independent) and must stay correct (#440).
//!
//! Every expected value below is the CPython result for the same source.

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

fn validate_wasm(wasm: &[u8]) {
    wasmparser::validate(wasm).expect("WASM validation failed");
}

/// Call an `(i64) -> i64` export.
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

/// Call a `() -> i64` export.
fn call_i64(wasm: &[u8], func_name: &str) -> i64 {
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
        .get_typed_func::<(), i64>(&store, func_name)
        .expect("Failed to get function");
    func.call(&mut store, ()).expect("Call failed")
}

// ============================
// The two B1 repro cases
// ============================

/// Repro 1: `break` under an `if` in a `while`. Before the fix this was an
/// INFINITE LOOP (the br targeted the if's end, a no-op, so the loop never
/// exited when the break should have fired).
#[test]
fn test_break_under_if_in_while() {
    let src = "\
def f(n: int) -> int:
    x = 0
    while x < 5:
        if n > 0:
            break
        x = x + 1
    return x
";
    let wasm = compile_wasm(src);
    validate_wasm(&wasm);
    // n > 0: break immediately on the first iteration -> x stays 0.
    assert_eq!(call_i64_i64(&wasm, "f", 1), 0);
    // n <= 0: break never fires; loop runs to completion -> x == 5.
    assert_eq!(call_i64_i64(&wasm, "f", 0), 5);
}

/// Repro 2: `continue` under an `if` in a `while`. Before the fix the
/// continue was a no-op (br to the if's end), yielding 15; CPython gives 13.
#[test]
fn test_continue_under_if_in_while() {
    let src = "\
def g(n: int) -> int:
    total = 0
    x = 0
    while x < 5:
        x = x + 1
        if x == n:
            continue
        total = total + x
    return total
";
    let wasm = compile_wasm(src);
    validate_wasm(&wasm);
    // Skip x == 2: 1 + 3 + 4 + 5 == 13 (the buggy emitter returned 15).
    assert_eq!(call_i64_i64(&wasm, "g", 2), 13);
    // No skip: 1 + 2 + 3 + 4 + 5 == 15.
    assert_eq!(call_i64_i64(&wasm, "g", 99), 15);
}

// ============================
// Deeper nesting: if / if-else
// ============================

/// `break` two `if` levels deep, with an `else` arm on the outer if.
#[test]
fn test_break_under_nested_if_else_in_while() {
    let src = "\
def h(a: int) -> int:
    x = 0
    while x < 100:
        if a > 0:
            if x > 3:
                break
            else:
                x = x + 2
        else:
            x = x + 1
    return x
";
    let wasm = compile_wasm(src);
    validate_wasm(&wasm);
    // a > 0: x = 0 -> 2 -> 4, then 4 > 3 breaks -> 4.
    assert_eq!(call_i64_i64(&wasm, "h", 1), 4);
    // a <= 0: no break path; loop exits at the condition -> 100.
    assert_eq!(call_i64_i64(&wasm, "h", 0), 100);
}

/// `continue` under an `elif` arm (the elif lowers to a NESTED if inside the
/// else, so the label depth differs from the plain-if case).
#[test]
fn test_continue_under_elif_in_while() {
    let src = "\
def e(n: int) -> int:
    total = 0
    x = 0
    while x < 6:
        x = x + 1
        if x == 1:
            total = total + 10
        elif x == n:
            continue
        total = total + x
    return total
";
    let wasm = compile_wasm(src);
    validate_wasm(&wasm);
    // x=1: +10 +1; x=2..6 add x except x==3 skipped by continue:
    // 10 + 1 + 2 + 4 + 5 + 6 == 28.
    assert_eq!(call_i64_i64(&wasm, "e", 3), 28);
}

// ============================
// for loops: nesting + the flat-continue increment bug
// ============================

/// `continue` under an `if` in a `for range` loop. Exercises BOTH halves of
/// B1 for `for`: the depth under the if AND that continue must still run the
/// increment (a br to the loop header would loop forever on i == 2).
#[test]
fn test_continue_under_if_in_for_range() {
    let src = "\
def m() -> int:
    total = 0
    for i in range(6):
        if i == 2:
            continue
        total = total + i
    return total
";
    let wasm = compile_wasm(src);
    validate_wasm(&wasm);
    // 0 + 1 + 3 + 4 + 5 == 13.
    assert_eq!(call_i64(&wasm, "m"), 13);
}

/// FLAT `continue` as the last statement of a `for` body. The old emitter's
/// Br(0) targeted the loop header, SKIPPING the increment — an infinite loop
/// even with no nesting at all. Must terminate and match CPython.
#[test]
fn test_flat_continue_in_for_range_terminates() {
    let src = "\
def t() -> int:
    total = 0
    for i in range(4):
        total = total + i
        continue
    return total
";
    let wasm = compile_wasm(src);
    validate_wasm(&wasm);
    // 0 + 1 + 2 + 3 == 6.
    assert_eq!(call_i64(&wasm, "t"), 6);
}

/// `break` in an inner `for` nested in a `while`: the break must target the
/// INNER loop only; the outer while keeps iterating.
#[test]
fn test_break_in_for_inside_while() {
    let src = "\
def k(n: int) -> int:
    total = 0
    w = 0
    while w < 3:
        w = w + 1
        for i in range(10):
            if i == n:
                break
            total = total + 1
    return total
";
    let wasm = compile_wasm(src);
    validate_wasm(&wasm);
    // Each of 3 outer iterations counts i = 0..3 then breaks at i == 4 -> 12.
    assert_eq!(call_i64_i64(&wasm, "k", 4), 12);
    // n outside range: inner loop runs fully, 10 per outer iteration -> 30.
    assert_eq!(call_i64_i64(&wasm, "k", 42), 30);
}

/// `break` in an inner `for` nested in an outer `for` (loop-in-loop label
/// stacking): inner break leaves the outer loop running.
#[test]
fn test_break_in_nested_for_for() {
    let src = "\
def w() -> int:
    total = 0
    for i in range(3):
        for j in range(10):
            if j > i:
                break
            total = total + 1
    return total
";
    let wasm = compile_wasm(src);
    validate_wasm(&wasm);
    // i=0: 1; i=1: 2; i=2: 3 -> 6.
    assert_eq!(call_i64(&wasm, "w"), 6);
}

// ============================
// try/except inside a loop
// ============================

/// `break` and `continue` under `if`s inside a `try` inside a `for`: the
/// branches must clear the try's handler blocks AND the $end block on the
/// way to the loop labels.
#[test]
fn test_break_continue_under_try_in_for() {
    let src = "\
def u() -> int:
    total = 0
    for i in range(6):
        try:
            if i == 2:
                continue
            if i == 4:
                break
            total = total + i
        except ValueError:
            total = total + 100
    return total
";
    let wasm = compile_wasm(src);
    validate_wasm(&wasm);
    // i=0 (+0), i=1 (+1), i=2 continue, i=3 (+3), i=4 break -> 4.
    assert_eq!(call_i64(&wasm, "u"), 4);
}

/// Same composition inside a `while` (continue must re-test the condition,
/// not skip it), plus a handler that actually catches.
#[test]
fn test_break_under_try_in_while_with_raise() {
    let src = "\
def v(n: int) -> int:
    total = 0
    x = 0
    while x < 8:
        x = x + 1
        try:
            if x == n:
                raise ValueError(\"hit\")
            if x == 6:
                break
            total = total + x
        except ValueError:
            total = total + 100
    return total
";
    let wasm = compile_wasm(src);
    validate_wasm(&wasm);
    // x=1,2 add; x=3 raises -> +100; x=4,5 add; x=6 breaks:
    // 1 + 2 + 100 + 4 + 5 == 112.
    assert_eq!(call_i64_i64(&wasm, "v", 3), 112);
}

/// `break`/`continue` inside an EXCEPT HANDLER body (not the try body): the
/// handler runs one label shallower than the try body (its block was closed
/// by the dispatch), so its depth accounting is a distinct code path.
#[test]
fn test_break_continue_in_except_handler() {
    let src = "\
def y() -> int:
    total = 0
    for i in range(6):
        try:
            if i == 1:
                raise ValueError(\"x\")
            total = total + i
        except ValueError:
            if total > 100:
                break
            continue
    return total

def z(n: int) -> int:
    total = 0
    x = 0
    while x < 10:
        x = x + 1
        try:
            if x == n:
                raise ValueError(\"x\")
            total = total + x
        except ValueError:
            break
    return total
";
    let wasm = compile_wasm(src);
    validate_wasm(&wasm);
    // y: i=0 (+0), i=1 raises -> handler continues, i=2..5 add -> 14.
    assert_eq!(call_i64(&wasm, "y"), 14);
    // z: x=1 (+1), x=2 (+2), x=3 raises -> handler breaks -> 3.
    assert_eq!(call_i64_i64(&wasm, "z", 3), 3);
    // z with no raise: 1 + 2 + ... + 10 == 55.
    assert_eq!(call_i64_i64(&wasm, "z", 99), 55);
}

// ============================
// Flat (directly-in-loop) regression cases — the pre-B1 behavior
// ============================

/// Flat `break` directly in a `while` body (no nesting) — the case the old
/// hardcoded Br(1) handled; must still pass.
#[test]
fn test_flat_break_in_while() {
    let src = "\
def q() -> int:
    x = 0
    while x < 10:
        x = x + 1
        break
    return x
";
    let wasm = compile_wasm(src);
    validate_wasm(&wasm);
    assert_eq!(call_i64(&wasm, "q"), 1);
}

/// Flat `continue` as the last statement of a `while` body — the case the
/// old hardcoded Br(0) handled; must still pass.
#[test]
fn test_flat_continue_in_while() {
    let src = "\
def s() -> int:
    x = 0
    while x < 4:
        x = x + 1
        continue
    return x
";
    let wasm = compile_wasm(src);
    validate_wasm(&wasm);
    assert_eq!(call_i64(&wasm, "s"), 4);
}

// ============================
// Early return under nesting (#440 must not regress)
// ============================

/// Early `return` under an `if` inside a `while` (#440's fix): `return` is a
/// depth-independent WASM instruction and must compose with the label stack.
#[test]
fn test_early_return_under_if_in_while() {
    let src = "\
def r(n: int) -> int:
    x = 0
    while x < 10:
        if x == n:
            return x * 100
        x = x + 1
    return 0 - 1
";
    let wasm = compile_wasm(src);
    validate_wasm(&wasm);
    assert_eq!(call_i64_i64(&wasm, "r", 3), 300);
    assert_eq!(call_i64_i64(&wasm, "r", 50), -1);
}

/// Early `return` from inside a `for` nested in a `while`, under an `if` —
/// return + break label bookkeeping composing at three levels.
#[test]
fn test_early_return_in_nested_loops() {
    let src = "\
def d(n: int) -> int:
    w = 0
    while w < 5:
        w = w + 1
        for i in range(4):
            if w * 10 + i == n:
                return w * 100 + i
    return 0 - 1
";
    let wasm = compile_wasm(src);
    validate_wasm(&wasm);
    // w=3, i=2 -> 32 == n -> return 302.
    assert_eq!(call_i64_i64(&wasm, "d", 32), 302);
    assert_eq!(call_i64_i64(&wasm, "d", 999), -1);
}
