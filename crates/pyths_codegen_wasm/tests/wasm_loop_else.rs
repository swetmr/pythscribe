//! Loop-`else` on the DEFAULT auto-routed WASM target (0.2.2 hold blocker 1,
//! B1 family).
//!
//! `wasm_analysis` ADMITTED `while/for ... else` (it recursively validated the
//! else body), but the emitter loop arms DISCARDED `else_body` via `..` —
//! admitted but never emitted, a SILENT wrong result on the default target
//! (`pyths run` uses the JS path and masked it). Repro p62: `fe(9)` returned 0
//! where CPython gives 200.
//!
//! The fix emits the else body using B1's label-depth-stack infrastructure:
//!
//!   block $break              ; `break` targets this — skips the else
//!     block $normal           ; normal loop exit targets this
//!       loop $continue ... end
//!     end
//!     <else body>             ; runs on NORMAL exit only
//!   end
//!
//! and closes the CLASS: every recursive walker in the WASM emitter
//! (typed-local/lambda/string collectors, pow/strings/dicts/errors usage
//! scans, math-import collection) now traverses loop else bodies too.
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

fn call_i64_i64(wasm: &[u8], func_name: &str, arg: i64) -> i64 {
    wasmparser::validate(wasm).expect("WASM validation failed");
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

// ============================
// The p62 repro pair
// ============================

/// `for ... else` — else runs on normal exhaustion, is skipped by `break`.
/// Pre-fix: the else body was silently dropped → fe(9) returned 0 (CPython
/// gives 200) on the default WASM target.
#[test]
fn test_for_else_break_vs_exhausted() {
    let src = r#"
def fe(n: int) -> int:
    r = 0
    for i in range(4):
        if i == n:
            r = 100
            break
    else:
        r = 200
    return r
"#;
    let wasm = compile_wasm(src);
    assert_eq!(call_i64_i64(&wasm, "fe", 2), 100); // break taken → else SKIPPED
    assert_eq!(call_i64_i64(&wasm, "fe", 9), 200); // exhausted → else RUNS
    assert_eq!(call_i64_i64(&wasm, "fe", 0), 100); // break on first iteration
    assert_eq!(call_i64_i64(&wasm, "fe", 3), 100); // break on last iteration
}

/// `while ... else` with a `return` INSIDE the else body (p62's `we`).
#[test]
fn test_while_else_with_return_in_else() {
    let src = r#"
def we(n: int) -> int:
    x = 0
    while x < 3:
        if x == n:
            break
        x = x + 1
    else:
        return 50
    return x
"#;
    let wasm = compile_wasm(src);
    assert_eq!(call_i64_i64(&wasm, "we", 1), 1); // break taken → else skipped
    assert_eq!(call_i64_i64(&wasm, "we", 9), 50); // test went false → else runs
    assert_eq!(call_i64_i64(&wasm, "we", 0), 0); // break before any increment
}

// ============================
// Class-boundary probes
// ============================

/// A zero-iteration loop still runs the else (CPython: the else fires on any
/// non-break exit, including "never entered").
#[test]
fn test_for_else_zero_iterations() {
    let src = r#"
def zi(n: int) -> int:
    r = 0
    for i in range(n):
        if i > 100:
            break
    else:
        r = 7
    return r
"#;
    let wasm = compile_wasm(src);
    assert_eq!(call_i64_i64(&wasm, "zi", 0), 7); // range(0): never entered → else runs
    assert_eq!(call_i64_i64(&wasm, "zi", 3), 7); // exhausted → else runs
}

/// Loop-else with a NESTED loop that breaks: the inner `break` must not skip
/// the OUTER loop's else (break binds to the nearest loop only).
#[test]
fn test_nested_break_does_not_skip_outer_else() {
    let src = r#"
def nb(n: int) -> int:
    r = 0
    for i in range(3):
        for j in range(3):
            if j == 1:
                break
        r = r + 1
    else:
        r = r + 100
    return r
"#;
    let wasm = compile_wasm(src);
    // inner break fires every outer iteration; outer loop exhausts → else runs
    assert_eq!(call_i64_i64(&wasm, "nb", 0), 103);
}

/// `break` under an `if` inside a `try` still skips the else (B1's label
/// depths compose with the extra structured blocks between break and loop).
#[test]
fn test_for_else_break_under_try() {
    let src = r#"
def bt(n: int) -> int:
    r = 0
    for i in range(5):
        try:
            if i == n:
                break
        except ValueError:
            r = -1
    else:
        r = 300
    return r
"#;
    let wasm = compile_wasm(src);
    assert_eq!(call_i64_i64(&wasm, "bt", 2), 0); // break → else skipped
    assert_eq!(call_i64_i64(&wasm, "bt", 9), 300); // exhausted → else runs
}

/// while-else where the else is skipped by a break at ANY depth and continue
/// still re-tests correctly.
#[test]
fn test_while_else_with_continue() {
    let src = r#"
def wc(n: int) -> int:
    x = 0
    total = 0
    while x < 6:
        x = x + 1
        if x % 2 == 0:
            continue
        if x == n:
            break
        total = total + x
    else:
        total = total + 1000
    return total
"#;
    let wasm = compile_wasm(src);
    // n=5: adds 1,3 then breaks at 5 → 4 (else skipped)
    assert_eq!(call_i64_i64(&wasm, "wc", 5), 4);
    // n=99: adds 1,3,5, loop exhausts → 9 + 1000
    assert_eq!(call_i64_i64(&wasm, "wc", 99), 1009);
}

/// for-else over a DESCENDING range (#364's negative-step path) — the
/// normal-exit branch is the specialized gt_s condition, not lt_s.
#[test]
fn test_for_else_negative_step() {
    let src = r#"
def ds(n: int) -> int:
    r = 0
    for i in range(5, 0, -1):
        if i == n:
            break
        r = r + i
    else:
        r = r + 100
    return r
"#;
    let wasm = compile_wasm(src);
    assert_eq!(call_i64_i64(&wasm, "ds", 3), 9); // 5+4 then break → else skipped
    assert_eq!(call_i64_i64(&wasm, "ds", 0), 115); // 5+4+3+2+1 + 100 (0 never hit)
}

/// A local assigned ONLY in the loop-else body (the walker class: typed-local
/// collection must traverse else bodies or the local is missing).
#[test]
fn test_local_only_assigned_in_else() {
    let src = r#"
def oe(n: int) -> int:
    for i in range(3):
        if i == n:
            break
    else:
        y = 41
        return y + 1
    return -1
"#;
    let wasm = compile_wasm(src);
    assert_eq!(call_i64_i64(&wasm, "oe", 1), -1); // break → else skipped
    assert_eq!(call_i64_i64(&wasm, "oe", 9), 42); // else runs, local works
}

/// for-else over a LIST (the emit_for_list arm, not emit_for_range) — the
/// same $normal-block layout must hold there.
#[test]
fn test_for_else_over_list() {
    let src = r#"
def fl(n: int) -> int:
    xs = [10, 20, 30]
    r = 0
    for x in xs:
        if x == n:
            r = x
            break
    else:
        r = 999
    return r
"#;
    let wasm = compile_wasm(src);
    assert_eq!(call_i64_i64(&wasm, "fl", 20), 20); // break → else skipped
    assert_eq!(call_i64_i64(&wasm, "fl", 5), 999); // exhausted → else runs
}
