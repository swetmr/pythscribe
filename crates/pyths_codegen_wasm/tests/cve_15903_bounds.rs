//! CVE-2026-15903 regression: the WASM list-subscript bounds-check cluster
//! (F1-F6 from `experiments/cve-15903-probe/RESULTS-2026-08-13.md`).
//!
//! Each test executes the emitted WASM under `wasmi` and asserts the memory-safe
//! outcome the fix guarantees. The canary tests are the strongest: a 16-element
//! `b` sits immediately after the 3-element victim `a`, so an out-of-bounds store
//! that used to land in `b` is caught by reading `b` back unchanged.
//!
//! Pre-fix behaviour (unpatched `pyths 0.2.1`, per the probe): F1 `a[5]=999`
//! wrote into `b[1]`; F2 dropped the read check unless the module had an
//! unrelated raise; F3 tested the `i32.wrap_i64`-truncated index so `a[2**32]`
//! aliased `a[0]`; F5 raised on `a[-1]` (or leaked the list header). Post-fix all
//! of these raise `IndexError` / normalize correctly, and OOB never touches the
//! canary.

use pyths_codegen_wasm::codegen_wasm;

fn compile_wasm(source: &str) -> Vec<u8> {
    let module = pyths_parser::parse(source).expect("parse failed");
    let output = codegen_wasm(&module);
    assert!(
        !output.wasm.is_empty(),
        "expected WASM output; rejected: {:?}",
        output.rejected_functions
    );
    output.wasm
}

fn validate_wasm(wasm: &[u8]) {
    wasmparser::validate(wasm).expect("WASM validation failed");
}

fn make_instance(wasm: &[u8]) -> (wasmi::Store<()>, wasmi::Instance) {
    let engine = wasmi::Engine::default();
    let module = wasmi::Module::new(&engine, wasm).expect("load module");
    let mut store = wasmi::Store::new(&engine, ());
    let linker = <wasmi::Linker<()>>::new(&engine);
    let instance = linker
        .instantiate(&mut store, &module)
        .expect("instantiate")
        .start(&mut store)
        .expect("start");
    (store, instance)
}

fn read_err_code(store: &mut wasmi::Store<()>, instance: &wasmi::Instance) -> i32 {
    let g = instance
        .get_global(&mut *store, "__err_code")
        .expect("no __err_code export");
    match g.get(&mut *store) {
        wasmi::Val::I32(v) => v,
        _ => panic!("__err_code not i32"),
    }
}

const IDX_ERR: i32 = 3;

// ── F1: `a[i] = v` store had NO bounds check — OOB wrote into a neighbour ──

#[test]
fn f1_store_oob_sets_indexerror_and_leaves_canary_intact() {
    // `b` is allocated right after `a`; pre-fix `a[5]=999` landed in `b[1]`.
    // The try/except lets execution continue past the caught IndexError. The
    // `caught` flag makes the RAISE observable (a silent-skip impl that merely
    // did nothing on OOB would leave caught==0 and fail here — closing review
    // K's tautology), and reading `b`'s canary digest back confirms the OOB
    // store was skipped (it would be 100_999_300 if it had corrupted b[1]).
    let src = "\
def victim(n: int) -> int:
    a = [1, 2, 3]
    b = [100, 200, 300]
    caught = 0
    try:
        a[n] = 999
    except IndexError:
        caught = 1
    return caught * 1000000000 + b[0] * 1000000 + b[1] * 1000 + b[2]
";
    let wasm = compile_wasm(src);
    validate_wasm(&wasm);
    let (mut store, instance) = make_instance(&wasm);
    let f = instance.get_typed_func::<i64, i64>(&store, "victim").unwrap();

    // In-bounds store works: no raise (caught==0), canary untouched.
    assert_eq!(f.call(&mut store, 0).unwrap(), 100_200_300);

    // OOB store: IndexError ACTUALLY RAISED (caught==1) AND canary b intact.
    let r = f.call(&mut store, 5).unwrap();
    assert_eq!(
        r, 1_100_200_300,
        "OOB store must RAISE IndexError (caught==1) and not corrupt b"
    );
}

#[test]
fn f1_store_oob_traps_when_no_error_infra() {
    // No raise/assert/try anywhere → needs_errors off. The store check is still
    // emitted (F2) and diverges via a memory-safe trap rather than corrupting
    // memory. wasmi surfaces the trap as a call error.
    let src = "\
def victim(n: int) -> int:
    a = [1, 2, 3]
    b = [100, 200, 300]
    a[n] = 999
    return b[0] * 1000000 + b[1] * 1000 + b[2]
";
    let wasm = compile_wasm(src);
    validate_wasm(&wasm);
    let (mut store, instance) = make_instance(&wasm);
    let f = instance.get_typed_func::<i64, i64>(&store, "victim").unwrap();
    // In-bounds still fine.
    assert_eq!(f.call(&mut store, 0).unwrap(), 100_200_300);
    // OOB traps (no OOB write).
    assert!(
        f.call(&mut store, 5).is_err(),
        "OOB store with no error infra must trap, not corrupt memory"
    );
}

// ── F2: read bounds check was gated on an unrelated raise/assert/try ──

#[test]
fn f2_read_oob_checked_without_any_error_infra() {
    // No raise/assert/try; pre-fix the read check was elided and `lst[10]`
    // returned 0. Post-fix it traps (memory-safe) instead of reading OOB.
    let src = "\
def get(i: int) -> int:
    lst = [10, 20, 30]
    return lst[i]
";
    let wasm = compile_wasm(src);
    validate_wasm(&wasm);
    let (mut store, instance) = make_instance(&wasm);
    let f = instance.get_typed_func::<i64, i64>(&store, "get").unwrap();
    assert_eq!(f.call(&mut store, 1).unwrap(), 20);
    assert!(
        f.call(&mut store, 10).is_err(),
        "OOB read with no error infra must trap, not read OOB"
    );
}

// ── F3: check tested the i32.wrap_i64-TRUNCATED index (a[2**32] -> a[0]) ──

#[test]
fn f3_read_full_width_index_not_truncated() {
    // needs_errors on so OOB is a catchable IndexError. 2**32 truncates to 0
    // under i32.wrap_i64; pre-fix that passed the check and returned lst[0]=10.
    let src = "\
def get(i: int) -> int:
    lst = [10, 20, 30]
    if i < -1000000:
        raise ValueError
    return lst[i]
";
    let wasm = compile_wasm(src);
    validate_wasm(&wasm);
    let (mut store, instance) = make_instance(&wasm);
    let f = instance.get_typed_func::<i64, i64>(&store, "get").unwrap();

    // 2**32 (0x1_0000_0000) truncates to 0; must NOT return lst[0].
    let r = f.call(&mut store, 1i64 << 32).unwrap();
    assert_eq!(read_err_code(&mut store, &instance), IDX_ERR, "2**32 must raise");
    assert_eq!(r, 0, "sentinel, not lst[0]");

    // 2**32 + 1 truncates to 1; must NOT return lst[1].
    let (mut store, instance) = make_instance(&wasm);
    let f = instance.get_typed_func::<i64, i64>(&store, "get").unwrap();
    let r = f.call(&mut store, (1i64 << 32) + 1).unwrap();
    assert_eq!(read_err_code(&mut store, &instance), IDX_ERR, "2**32+1 must raise");
    assert_eq!(r, 0, "sentinel, not lst[1]");
}

#[test]
fn f3_store_full_width_index_not_truncated() {
    // Store analogue: `a[2**32] = 999` truncates to a[0] pre-fix and silently
    // corrupted the victim. Post-fix it raises (caught==1, observable — closes
    // review K) and leaves a untouched.
    let src = "\
def victim(n: int) -> int:
    a = [1, 2, 3]
    caught = 0
    try:
        a[n] = 999
    except IndexError:
        caught = 1
    return caught * 1000 + a[0] * 100 + a[1] * 10 + a[2]
";
    let wasm = compile_wasm(src);
    validate_wasm(&wasm);
    let (mut store, instance) = make_instance(&wasm);
    let f = instance.get_typed_func::<i64, i64>(&store, "victim").unwrap();
    let r = f.call(&mut store, 1i64 << 32).unwrap();
    assert_eq!(
        r, 1123,
        "a[2**32]=999 must RAISE (caught==1) and not alias a[0]"
    );
}

// ── F5: negative indices — full Python from-the-end normalization ──

#[test]
fn f5_negative_index_reads_from_end() {
    // a[-1] == a[len-1], a[-2] == a[len-2]; a[-len-1] is out of range.
    let src = "\
def get(i: int) -> int:
    lst = [11, 22, 33]
    if i < -1000000:
        raise ValueError
    return lst[i]
";
    let wasm = compile_wasm(src);
    validate_wasm(&wasm);
    let (mut store, instance) = make_instance(&wasm);
    let f = instance.get_typed_func::<i64, i64>(&store, "get").unwrap();
    assert_eq!(f.call(&mut store, -1).unwrap(), 33, "a[-1] -> last");
    assert_eq!(read_err_code(&mut store, &instance), 0);
    assert_eq!(f.call(&mut store, -3).unwrap(), 11, "a[-3] -> first");
    assert_eq!(read_err_code(&mut store, &instance), 0);
    // a[-4] on len 3 is genuinely out of range -> IndexError.
    let r = f.call(&mut store, -4).unwrap();
    assert_eq!(read_err_code(&mut store, &instance), IDX_ERR, "a[-4] out of range");
    assert_eq!(r, 0);
}

#[test]
fn f5_negative_store_writes_from_end() {
    // `a[-1] = v` targets the last element (computed negative reaches the store
    // path; pre-fix it landed on the list header, corrupting len).
    let src = "\
def f(n: int) -> int:
    a = [1, 2, 3]
    if n < -1000000:
        raise ValueError
    a[n] = 99
    return a[0] * 100 + a[1] * 10 + a[2]
";
    let wasm = compile_wasm(src);
    validate_wasm(&wasm);
    let (mut store, instance) = make_instance(&wasm);
    let f = instance.get_typed_func::<i64, i64>(&store, "f").unwrap();
    // a[-1]=99 -> [1, 2, 99] -> 1*100 + 2*10 + 99 = 219.
    assert_eq!(f.call(&mut store, -1).unwrap(), 219, "a[-1]=99 -> a[2]");
    assert_eq!(read_err_code(&mut store, &instance), 0);
}

// ── Review D: subscript in a `return` inside try/except stays on JS ──

#[test]
fn d_subscript_return_in_try_is_rejected_from_wasm() {
    // The WASM error model can't dispatch the pending IndexError before the
    // `return` exits the function, so the local handler would be bypassed. Such
    // a function must be rejected from WASM and handled by the (correct) JS
    // backend instead.
    let src = "\
def f(n: int) -> int:
    a = [1, 2, 3]
    try:
        return a[n]
    except IndexError:
        return -1
";
    let module = pyths_parser::parse(src).expect("parse");
    let out = pyths_codegen_wasm::codegen_wasm(&module);
    assert!(
        out.wasm.is_empty(),
        "function with a raising `return` inside try must NOT be WASM-admitted"
    );
    assert!(
        out.rejected_functions
            .iter()
            .any(|(n, r)| n == "f" && r.contains("return") && r.contains("try")),
        "expected a review-D rejection reason, got {:?}",
        out.rejected_functions
    );
}

#[test]
fn d_subscript_assignment_in_try_still_wasm_eligible() {
    // Control: the common `try: x = a[i]` assignment form composes correctly
    // (the post-statement dispatch runs) and must stay WASM-eligible.
    let src = "\
def g(n: int) -> int:
    a = [1, 2, 3]
    x = 0
    try:
        x = a[n]
    except IndexError:
        x = -1
    return x
";
    let module = pyths_parser::parse(src).expect("parse");
    let out = pyths_codegen_wasm::codegen_wasm(&module);
    assert!(
        !out.wasm.is_empty(),
        "try: x = a[i] must stay WASM-eligible; rejected: {:?}",
        out.rejected_functions
    );
}

#[test]
fn d_subscript_return_in_nested_bodies_within_try_rejected() {
    // Review D round 2: the raising `return` may hide inside a loop `else`,
    // a `with`, a `match` arm, an `if`, or a nested try inside the try body —
    // all must be caught and pushed to the JS backend.
    let cases = [
        // loop else
        "def f(n: int) -> int:\n    a = [1, 2, 3]\n    try:\n        while False:\n            pass\n        else:\n            return a[n]\n    except IndexError:\n        return -1\n",
        // for else
        "def f(n: int) -> int:\n    a = [1, 2, 3]\n    try:\n        for _ in a:\n            pass\n        else:\n            return a[n]\n    except IndexError:\n        return -1\n",
        // nested inside an if
        "def f(n: int) -> int:\n    a = [1, 2, 3]\n    try:\n        if n > 0:\n            return a[n]\n    except IndexError:\n        return -1\n    return 0\n",
    ];
    for (i, src) in cases.iter().enumerate() {
        let module = pyths_parser::parse(src).expect("parse");
        let out = pyths_codegen_wasm::codegen_wasm(&module);
        assert!(
            out.wasm.is_empty(),
            "case {i}: raising return in a nested try body must NOT be WASM-admitted"
        );
    }
}

#[test]
fn d_except_kind_gates_rejection() {
    // Review D over-rejection fix: reject a subscript-return-in-try ONLY when a
    // handler could catch IndexError. `except ValueError` cannot, so the
    // IndexError propagates either way and the function stays WASM-eligible.
    let catches = [
        ("except IndexError", true),
        ("except LookupError", true),
        ("except Exception", true),
        ("except", true), // bare
        ("except ValueError", false),
        ("except KeyError", false),
    ];
    for (handler, should_reject) in catches {
        let src = format!(
            "def f(n: int) -> int:\n    a = [1, 2, 3]\n    try:\n        return a[n]\n    {handler}:\n        return -1\n"
        );
        let module = pyths_parser::parse(&src).expect("parse");
        let out = pyths_codegen_wasm::codegen_wasm(&module);
        if should_reject {
            assert!(
                out.wasm.is_empty(),
                "`{handler}` catches IndexError → must be rejected from WASM"
            );
        } else {
            assert!(
                !out.wasm.is_empty(),
                "`{handler}` cannot catch IndexError → must stay WASM-eligible; rejected: {:?}",
                out.rejected_functions
            );
        }
    }
}

#[test]
fn t6_tuple_handler_catches_only_listed_types() {
    // Review finding 6: `except (ValueError, KeyError)` must catch those two
    // types but NOT IndexError (the emitter previously treated any tuple as a
    // catch-all Exception).
    let src = "\
def f(x: int) -> int:
    try:
        if x == 1:
            raise ValueError
        if x == 2:
            raise KeyError
        raise IndexError
    except (ValueError, KeyError):
        return 99
    return 0
";
    let wasm = compile_wasm(src);
    validate_wasm(&wasm);
    let (mut store, instance) = make_instance(&wasm);
    let f = instance.get_typed_func::<i64, i64>(&store, "f").unwrap();
    // ValueError and KeyError are caught → 99, err_code cleared.
    assert_eq!(f.call(&mut store, 1).unwrap(), 99);
    assert_eq!(read_err_code(&mut store, &instance), 0, "ValueError must be caught");
    assert_eq!(f.call(&mut store, 2).unwrap(), 99);
    assert_eq!(read_err_code(&mut store, &instance), 0, "KeyError must be caught");
    // IndexError is NOT listed → not caught → err_code stays set (propagates).
    let _ = f.call(&mut store, 3).unwrap();
    assert_ne!(
        read_err_code(&mut store, &instance),
        0,
        "IndexError must NOT be caught by (ValueError, KeyError)"
    );
}
