//! Option B (minimal int/float fidelity, #451 replacement) — the three
//! interop ACCEPTANCE LANES, executed for real through `pyths run` + node,
//! plus the REAL-WASM battery (#465/#468): lanes that compile
//! `--target js+wasm` and execute the emitted `.wasm` + glue under node,
//! so the int/float boundary is exercised through the actual JS↔WASM
//! marshalling surface — not the pure-JS twin `pyths run` executes.
//!
//! Value model under test:
//!   * `int` — UNTOUCHED pre-#451 hybrid (small int = native JS Number,
//!     large int = BigInt). Lane 2 proves it stayed that way: native sinks
//!     (`.slice`, `Date`, object literals, `Array(n)`, array indexing)
//!     receive plain Numbers — no BigInt leak, no throw.
//!   * `float` — ONLY integer-valued floats (8.0) are boxed in a PyFloat
//!     carrier (`valueOf()` keeps native arithmetic/ToNumber working; the
//!     `__pyfloat__` brand drives repr). A non-integer float (3.14) stays a
//!     native Number. Lane 1 proves container/dynamic repr fidelity; lane 3
//!     proves the boxed value crosses native JS sinks correctly, including
//!     the typeof-dispatching ones (`Array(3.0)`).
//!
//! These lanes are the regression fence for the Option A revert: A (int =
//! BigInt everywhere) fixed lane 1 but broke lane 2 catastrophically —
//! bigints at native sinks. B must hold all three at once.
//!
//! Lane 5 (`lane5_ts_dts_typecheck_and_run`) is the third battery leg the
//! docs promise — JS-native / WASM-host / **TS-.d.ts-run**: a real
//! TypeScript consumer is type-checked against the emitted `.d.ts` AND
//! executed under node, so a declaration that lies about the runtime value
//! model can no longer pass silently.

use std::path::Path;
use std::process::Command;

fn pyths_bin() -> Command {
    Command::new(env!("CARGO_BIN_EXE_pyths"))
}

/// Compile-and-run `src` through `pyths run`, returning stdout.
fn run_ps(name: &str, src: &str) -> String {
    let dir = std::env::temp_dir().join(format!("pyths_optb_{}", name));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let ps = dir.join(format!("{}.ps", name));
    std::fs::write(&ps, src).unwrap();
    let out = pyths_bin()
        .args(["run", ps.to_str().unwrap()])
        .output()
        .expect("pyths run failed to spawn");
    let stdout = String::from_utf8_lossy(&out.stdout).replace("\r\n", "\n");
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        out.status.success(),
        "pyths run {name} should exit 0; stderr={stderr}\nstdout={stdout}"
    );
    let _ = std::fs::remove_dir_all(&dir);
    stdout
}

/// Real-WASM harness (#465 pattern, generalized for #468): compile `src`
/// with `--target js+wasm`, prove the artifacts are genuine WASM, execute
/// main + glue + `.wasm` under node, and return stdout.
///
/// Anti-vacuity gates (a lane built on this helper cannot silently test the
/// JS twin the way the old `run_ps` "wasm lane" did — `pyths run` never
/// emits WASM):
///   1. a non-empty `<stem>.wasm` module is emitted;
///   2. the glue routes every function in `wasm_fns` through a literal
///      `__wasm.<name>(` call (the instantiated module's export — the twin
///      ladder only runs on `__isWasmFault`, never first);
///   3. every fragment in `glue_markers` (per-lane marshaller authorities:
///      `__f64Box(`, `__i64ToJs(`, `__f64Arg(`, `__list_to_wasm(`, ...) is
///      present in the glue;
///   4. after the successful run, the `.wasm` is DELETED and node is run
///      again — the program must FAIL, proving execution genuinely loads
///      the module (a twin-only or JS-only build would keep passing).
fn compile_wasm_run(name: &str, src: &str, wasm_fns: &[&str], glue_markers: &[&str]) -> String {
    let dir = std::env::temp_dir().join(format!("pyths_optb_{}", name));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let ps = dir.join(format!("{}.ps", name));
    std::fs::write(&ps, src).unwrap();
    let main_js = dir.join("main.mjs");
    let out = pyths_bin()
        .args([
            "compile",
            ps.to_str().unwrap(),
            "--target",
            "js+wasm",
            "-o",
            main_js.to_str().unwrap(),
        ])
        .output()
        .expect("pyths compile failed to spawn");
    assert!(
        out.status.success(),
        "compile --target js+wasm should succeed; stderr={}",
        String::from_utf8_lossy(&out.stderr)
    );
    // Gate 1 — a real, non-empty WASM module was emitted.
    let wasm_path = dir.join("main.wasm");
    let wasm_len = std::fs::metadata(&wasm_path)
        .expect("a .wasm module must be emitted beside the output")
        .len();
    assert!(wasm_len > 0, "emitted .wasm must be non-empty");
    // Gates 2 + 3 — the glue calls the instantiated module's exports and
    // carries the expected marshalling authorities.
    let glue = std::fs::read_to_string(dir.join("main.glue.js")).unwrap();
    for f in wasm_fns {
        assert!(
            glue.contains(&format!("__wasm.{}(", f)),
            "glue must route `{f}` through the real WASM export __wasm.{f}(...):\n{glue}"
        );
    }
    for m in glue_markers {
        assert!(glue.contains(m), "glue must contain `{m}`:\n{glue}");
    }
    // ESM package root + the runtime the compiled module imports,
    // materialized exactly like `pyths run` does it.
    std::fs::write(dir.join("package.json"), "{\"type\":\"module\"}\n").unwrap();
    pyths_runtime::materialize_runtime_package(&dir).unwrap();
    let node = Command::new("node")
        .arg(&main_js)
        .current_dir(&dir)
        .output()
        .expect("node failed to spawn");
    let stdout = String::from_utf8_lossy(&node.stdout).replace("\r\n", "\n");
    assert!(
        node.status.success(),
        "node should exit 0; stderr={}\nstdout={stdout}",
        String::from_utf8_lossy(&node.stderr)
    );
    // Gate 4 — the run DEPENDS on the .wasm: remove it and the program must
    // fail (the glue's fs loader reads it at startup; nothing can silently
    // fall back to the twin for a missing module).
    std::fs::remove_file(&wasm_path).unwrap();
    let node_no_wasm = Command::new("node")
        .arg(&main_js)
        .current_dir(&dir)
        .output()
        .expect("node failed to spawn");
    assert!(
        !node_no_wasm.status.success(),
        "with the .wasm deleted the program must FAIL — if it still passes, \
         the lane never depended on the WASM module and is vacuous; stdout={}",
        String::from_utf8_lossy(&node_no_wasm.stdout)
    );
    let _ = std::fs::remove_dir_all(&dir);
    stdout
}

#[test]
fn lane1_container_float_fidelity() {
    // The bug Option B exists to fix: an integer-valued float in a container
    // or dynamic context must repr as a float, exactly like CPython.
    let out = run_ps(
        "lane1",
        r#"print([8.0, 9.0])
print([0.0, 1.0])
print([1, 2.0, 3])
print([float(5)])
print(8.0)
print(2**53 + 1)
print((3.9, 4.0))
"#,
    );
    assert_eq!(
        out,
        "[8.0, 9.0]\n[0.0, 1.0]\n[1, 2.0, 3]\n[5.0]\n8.0\n9007199254740993\n(3.9, 4.0)\n"
    );
}

#[test]
fn lane2_int_interop_stays_native() {
    // int is UNTOUCHED: native Numbers at every JS sink; exact BigInt only
    // past 2**53. This is the lane the reverted Option A broke.
    let out = run_ps(
        "lane2",
        r#"xs = [10, 20, 30, 40]
n = 2
print(xs.slice(0, n))
d = Date(0)
print(d.getTime())
opts = {"maximumAge": 0, "timeout": 5000}
print(JSON.stringify(opts))
arr = Array(n)
print(arr.length)
i = 1
print(xs[i], xs[i + 1])
big = 2**53 + 1
print(big, big - 1)
"#,
    );
    assert_eq!(
        out,
        "[10, 20]\n0\n{\"maximumAge\":0,\"timeout\":5000}\n2\n20 30\n9007199254740993 9007199254740992\n"
    );
}

#[test]
fn lane3_float_interop_crosses_native_sinks() {
    // Boxed floats must behave as native numbers at JS sinks. valueOf()
    // covers ToNumber coercion (Math.*, toFixed via the [[NumberData]]
    // slot, JSON.stringify); Array(3.0) is the typeof-DISPATCH case the
    // codegen unboxes explicitly (a box would build [box] of length 1).
    let out = run_ps(
        "lane3",
        r#"import math
print(math.sin(3.14))
print(math.sqrt(16))
print(Math.max(1.0, 2.0))
x = 3.14159
print(x.toFixed(2))
y = 8.0
print(y.toFixed(2))
arr = Array(3.0)
print(arr.length)
print(JSON.stringify({"a": 2.0}))
import json
print(json.dumps({"a": 2.0, "b": 3.14, "c": 5}))
"#,
    );
    assert_eq!(
        out,
        "0.0015926529164868282\n4.0\n2\n3.14\n8.00\n3\n{\"a\":2}\n{\"a\": 2.0, \"b\": 3.14, \"c\": 5}\n"
    );
}

#[test]
fn lane4_complex_and_random_instance() {
    // Review round 2 (blocker + should-fix): boxed integer-valued floats
    // must flow through the complex-coercion authority (__toComplex backs
    // complex() and every PyComplex dunder), complex components/abs are
    // floats, and random.Random INSTANCE methods carry the float tag.
    let out = run_ps(
        "lane4",
        r#"print(complex(8.0))
print(complex(8.0, 2.0))
print(8.0 + 2j)
print(2j + 8.0)
z = 8.0 + 0j
print(z.real, z.imag)
print(abs(3.0 + 4.0j), [abs(3.0 + 4.0j)])
import random
r = random.Random(0)
print(r.uniform(5, 5), [r.uniform(2, 2)])
"#,
    );
    assert_eq!(
        out,
        "(8+0j)\n(8+2j)\n(8+2j)\n(8+2j)\n8.0 0.0\n5.0 [5.0]\n5.0 [2.0]\n"
    );
}

#[test]
fn js_twin_f64_boxed_iff_integer_valued_control() {
    // JS-TWIN CONTROL — NOT a WASM test (#468). This lane runs via `pyths
    // run`, which NEVER emits WASM (`@wasm` is a no-op there): it exercises
    // the pure-JS twin of the same shapes. It was formerly (mis)named
    // `wasm_f64_boxed_iff_integer_valued`, which overstated it as the
    // "WASM-host interop lane". Kept deliberately as the JS-path companion:
    // the twins are the glue's fault-fallback path, so the same observable
    // contract must hold on both sides of the ladder. The REAL WASM
    // coverage — compiled `--target js+wasm`, executing the emitted `.wasm`
    // + glue under node — is the `wasm_real_*` battery below.
    let out = run_ps(
        "wasmlane",
        r#"@wasm
def scale(x: float, k: float) -> float:
    return x * k

@wasm
def add_big(a: int, b: int) -> int:
    return a + b

print(scale(6.0, 2.0))
print(scale(1.57, 2.0))
print(add_big(2**53, 1))
print([scale(4.0, 2.0)])
"#,
    );
    assert_eq!(out, "12.0\n3.14\n9007199254740993\n[8.0]\n");
}

#[test]
fn wasm_f64_arg_huge_int_raises_overflow() {
    // #465: a huge in-.ps int (BigInt beyond IEEE-754 double range) passed to
    // a WASM-routed `float` parameter must raise OverflowError with CPython's
    // message (`float(10**400)` → "int too large to convert to float"),
    // matching the runtime value-boundary authority __reqNum — NOT silently
    // cross the boundary as Infinity (the pre-fix `Number(bigint)` glue
    // behavior; this exact program printed `inf` on base 7e19ec9b, which also
    // proves the WASM boundary — not the JS twin — decides the outcome).
    //
    // `pyths run` never emits WASM, so this lane compiles --target js+wasm
    // for real and executes the emitted main + glue + .wasm under node (the
    // browser glue's fs fallback), with the runtime materialized the same way
    // `pyths run` does it.
    let dir = std::env::temp_dir().join("pyths_optb_wasmovf");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let ps = dir.join("wasmovf.ps");
    std::fs::write(
        &ps,
        r#"@wasm
def scale(x: float, k: float) -> float:
    return x * k

try:
    print(scale(10**400, 2.0))
except OverflowError as e:
    print("caught:", e)
try:
    print(scale(-(10**400), 2.0))
except OverflowError as e:
    print("caught:", e)
print(scale(2**53 + 1, 1.0))
print(scale(6.5, 2.0))
"#,
    )
    .unwrap();
    let main_js = dir.join("main.mjs");
    let out = pyths_bin()
        .args([
            "compile",
            ps.to_str().unwrap(),
            "--target",
            "js+wasm",
            "-o",
            main_js.to_str().unwrap(),
        ])
        .output()
        .expect("pyths compile failed to spawn");
    assert!(
        out.status.success(),
        "compile --target js+wasm should succeed; stderr={}",
        String::from_utf8_lossy(&out.stderr)
    );
    // The glue must route the f64 args through the #465 authority helper.
    let glue = std::fs::read_to_string(dir.join("main.glue.js")).unwrap();
    assert!(
        glue.contains("__f64Arg("),
        "glue should coerce f64 args via __f64Arg: {glue}"
    );
    // ESM: `main.glue.js` is extension-.js, so the dir needs type=module;
    // the compiled output imports the "pyths-runtime" specifier, so
    // materialize the package exactly like `pyths run` does.
    std::fs::write(dir.join("package.json"), "{\"type\":\"module\"}\n").unwrap();
    pyths_runtime::materialize_runtime_package(&dir).unwrap();
    let node = Command::new("node")
        .arg(&main_js)
        .current_dir(&dir)
        .output()
        .expect("node failed to spawn");
    let stdout = String::from_utf8_lossy(&node.stdout).replace("\r\n", "\n");
    assert!(
        node.status.success(),
        "node should exit 0; stderr={}\nstdout={stdout}",
        String::from_utf8_lossy(&node.stderr)
    );
    assert_eq!(
        stdout,
        "caught: int too large to convert to float\n\
         caught: int too large to convert to float\n\
         9007199254740992.0\n\
         13.0\n"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn wasm_real_f64_return_boxes_iff_integer_valued() {
    // #468 lane 1 — f64 RETURN boxing through the real `.wasm`: a WASM f64
    // result re-enters JS via the glue's `__f64Box` authority, so an
    // integer-valued result (8.0) arrives BOXED (PyFloat carrier) and
    // reprs "8.0", while a non-integer result (2.5) stays a native Number.
    // The CONTAINER print is the discriminating witness: `print([...])`
    // reprs each element dynamically, so an unboxed 8 would print "[8, ...]"
    // (bare `print(scale(...))` goes through the static-float formatter and
    // would mask a boxing failure). Expected values modeled on CPython
    // 3.12.7: scale(4.0,2.0)=8.0, scale(1.25,2.0)=2.5, scale(0.0,3.0)=0.0.
    let out = compile_wasm_run(
        "wasmret",
        r#"@wasm
def scale(x: float, k: float) -> float:
    return x * k

print(scale(4.0, 2.0))
print(scale(1.25, 2.0))
print([scale(4.0, 2.0), scale(1.25, 2.0)])
print(scale(0.0, 3.0))
"#,
        &["scale"],
        &["__f64Box("],
    );
    assert_eq!(out, "8.0\n2.5\n[8.0, 2.5]\n0.0\n");
}

#[test]
fn wasm_real_i64_int_roundtrip_exact() {
    // #468 lane 2 — the i64 int path through the real `.wasm`: int args
    // cross as i64 (BigInt past 2**53, `BigInt(Math.trunc(n))` for small
    // Numbers) and the i64 result re-enters JS via `__i64ToJs` — Number
    // when safe, BigInt-exact past 2**53. 2**53 + 1 = 9007199254740993 is
    // the boundary witness: a float-contaminated path would round it to
    // 9007199254740992. `add_big(2**52 + 1, 2**52)` crosses the safe-int
    // boundary INSIDE WASM (both args are safe Numbers, the sum is not).
    // Expected values modeled on CPython 3.12.7.
    let out = compile_wasm_run(
        "wasmi64",
        r#"@wasm
def add_big(a: int, b: int) -> int:
    return a + b

print(add_big(2**53, 1))
print(add_big(20, 22))
print([add_big(2**53, 1)])
print(add_big(2**52 + 1, 2**52))
print(add_big(-(2**53), -1))
"#,
        &["add_big"],
        &["__i64ToJs("],
    );
    assert_eq!(
        out,
        "9007199254740993\n42\n[9007199254740993]\n9007199254740993\n-9007199254740993\n"
    );
}

#[test]
fn wasm_real_boxed_float_arg_unwraps_to_f64() {
    // #468 lane 3 — boxed-float ARG through the real `.wasm`: `y = 8.0` is
    // the PyFloat carrier (a boxed Number OBJECT) under Option B; passing
    // it to a WASM f64 param, the glue's `__f64Arg` passes the boxed Number
    // through (non-bigint branch); the WASM JS-API boundary's OWN ToNumber
    // then unwraps it via [[NumberData]]. (__f64Arg's active job is the
    // bigint→number/OverflowError case, #465; here it is the pass-through
    // that lets the boundary see a Number, not a bare object.) Round trip:
    // boxed 8.0 in → f64 multiply in WASM → boxed
    // 16.0 out (container [4.0] re-boxes the integer-valued result).
    // Expected values modeled on CPython 3.12.7.
    let out = compile_wasm_run(
        "wasmargbox",
        r#"@wasm
def scale(x: float, k: float) -> float:
    return x * k

y = 8.0
print(scale(y, 2.0))
print([scale(y, 0.5)])
z = 2.5
print(scale(z, 2.0))
"#,
        &["scale"],
        &["__f64Arg("],
    );
    assert_eq!(out, "16.0\n[4.0]\n5.0\n");
}

#[test]
fn wasm_real_list_param_marshalling() {
    // #468 lane 4 — list[float] / list[int] PARAM marshalling through the
    // real `.wasm`: `__list_to_wasm` copies the elements into linear memory
    // (f64 elements unwrap boxed 4.0 via ToNumber at the DataView setter;
    // i64 elements go BigInt-exact), the loop runs IN WASM, and the scalar
    // result re-enters boxed/exact. `isum([2**52, 2**52, 1])` crosses the
    // safe-integer boundary inside WASM. The final case exercises the
    // marshaller's i64-range ELEMENT GUARD: 2**64 exceeds i64, the glue
    // throws its RangeError and the fault ladder re-runs the JS twin, so
    // the observable stays CPython-exact (18446744073709551617) instead of
    // a silently wrapped i64 — the guard string is asserted in the glue.
    //
    // List RETURNS are deliberately NOT covered (#377): the WASM admission
    // gate `pyths_hir::is_scalar_wasm_return` (#364) admits only scalar
    // returns (int/float/bool/None/void) — a `-> list[float]` function is
    // rejected at admission and stays on the JS path, so a "list-return
    // WASM lane" cannot exist today; forcing one would test nothing.
    // (Body note: `for x in xs` lowers for list[float], but the list[int]
    // body must use index form `xs[i]` — the direct-iteration form is a
    // type-lowering gap the compiler correctly refuses to admit; filed as #475.)
    //
    // COMPILER BUG FOUND WHILE WRITING THIS LANE (filed as #474): a
    // module containing BOTH a list[float]-param and a list[int]-param
    // `@wasm` function fails admission NONDETERMINISTICALLY (~25-50% of
    // compiles: "WASM codegen produced invalid WASM for this function
    // (type-lowering gap)", naming one of the two) — hash-order dependent;
    // single-element-kind modules are 12/12 deterministic, as is mixing a
    // list fn with scalar-only fns. Until that is fixed the two element
    // kinds are compiled as SEPARATE single-function modules here, which
    // keeps this lane deterministic without giving up either kind.
    // Expected values modeled on CPython 3.12.7.
    let out = compile_wasm_run(
        "wasmlist_f64",
        r#"@wasm
def total(xs: list[float]) -> float:
    s = 0.0
    for x in xs:
        s = s + x
    return s

print(total([1.5, 2.5, 4.0]))
print([total([1.5, 2.5, 4.0])])
print(total([0.5, 0.25]))
"#,
        &["total"],
        &["__list_to_wasm("],
    );
    assert_eq!(out, "8.0\n[8.0]\n0.75\n");

    let out = compile_wasm_run(
        "wasmlist_i64",
        r#"@wasm
def isum(xs: list[int]) -> int:
    s = 0
    for i in range(len(xs)):
        s = s + xs[i]
    return s

print(isum([1, 2, 3]))
print(isum([2**52, 2**52, 1]))
print(isum([2**64, 1]))
"#,
        &["isum"],
        &[
            "__list_to_wasm(",
            "list element exceeds the i64 range of the WASM fast path",
        ],
    );
    assert_eq!(out, "6\n9007199254740993\n18446744073709551617\n");
}

#[test]
fn jsx_children_and_props_unbox() {
    // Flame-React f093 regression: a boxed integer-valued float passed as a
    // createElement CHILD is an object — React throws "Objects are not
    // valid as a React child (found: [object Number])". Every JSX value
    // boundary (children, all props, both factory forms) must unbox:
    // float literals emit bare at compile time, dynamics unwrap via __pyJs.
    // Native number children render via JS toString (8.0 -> "4") — the
    // React-oracle parity target, deliberately NOT pyStr'd.
    let dir = std::env::temp_dir().join("pyths_optb_jsxchild");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let ps = dir.join("stats.ps");
    std::fs::write(
        &ps,
        r#"from pyths.react import component

@component
def Stats(props):
    avg = 8.0 / 2
    return div("Average: ", avg, span(4.0), width=4.0, tab_index=2.0)
"#,
    )
    .unwrap();
    let out_js = dir.join("stats.mjs");
    let out = pyths_bin()
        .args([
            "compile",
            ps.to_str().unwrap(),
            "-o",
            out_js.to_str().unwrap(),
        ])
        .output()
        .expect("pyths compile failed to spawn");
    assert!(
        out.status.success(),
        "compile: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let js = std::fs::read_to_string(&out_js).unwrap();
    // Dynamic (possibly-boxed) child unwraps through __pyJs.
    assert!(
        js.contains("__pyJs(avg)"),
        "dynamic JSX child must unwrap through __pyJs:
{js}"
    );
    // A float-literal child emits a bare native number — never a box.
    assert!(
        js.contains("createElement(\"span\", null, (4))"),
        "float-literal JSX child must emit bare:
{js}"
    );
    // Numeric props unbox too (width=4.0, tab_index=2.0).
    assert!(
        js.contains("width: (4)") && js.contains("tabIndex: (2)"),
        "float-literal JSX props must emit bare:
{js}"
    );
    // The box must never appear anywhere in the createElement call.
    let ce = js
        .lines()
        .find(|l| l.contains("createElement(\"div\""))
        .expect("createElement line");
    assert!(
        !ce.contains("__pyF"),
        "no box may cross the JSX boundary:
{ce}"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn react_style_values_unbox_for_px_append() {
    // React appends "px" ONLY for `typeof value === "number"` — a boxed
    // 10.0 in a style dict would silently drop the unit. The codegen emits
    // integer-valued float literals bare, wraps statically-float dynamics
    // in __pyJs, and pyNormalizeStyle unboxes fully-dynamic dicts.
    let dir = std::env::temp_dir().join("pyths_optb_style");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let ps = dir.join("card.ps");
    std::fs::write(
        &ps,
        r#"from pyths.react import component

@component
def Card(props):
    pad = 10.0
    return div(
        style={"padding": pad, "margin": 4.0, "opacity": 0.5},
        span("hi"),
    )
"#,
    )
    .unwrap();
    let out_js = dir.join("card.mjs");
    let out = pyths_bin()
        .args([
            "compile",
            ps.to_str().unwrap(),
            "-o",
            out_js.to_str().unwrap(),
        ])
        .output()
        .expect("pyths compile failed to spawn");
    assert!(
        out.status.success(),
        "compile: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let js = std::fs::read_to_string(&out_js).unwrap();
    // Float literal in a style dict → bare native number (compile-time
    // unbox); statically-float variable → __pyJs unwrap.
    assert!(
        js.contains("\"padding\": __pyJs(pad)"),
        "style value of a float-typed variable must unwrap through __pyJs:\n{js}"
    );
    assert!(
        js.contains("\"margin\": (4)"),
        "an integer-valued float LITERAL style value must emit bare:\n{js}"
    );
    assert!(
        !js.contains("\"margin\": __pyF"),
        "style values must never carry a box into React:\n{js}"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

/// Path to the PINNED TypeScript checker for this lane: TypeScript 5.9.3
/// vendored (by lockfile) at `crates/pyths_cli/tests/ts-toolchain/`. The
/// lane invokes ONLY `node <this path>` — never a PATH `tsc`/`tsgo`, never
/// an `npx` download at test time. A gate that resolves its checker from
/// ambient PATH (or the network) is not a gate: it silently changes
/// behavior per machine and can vanish offline. If the checker is absent
/// the lane FAILS with provisioning instructions — it never skips and
/// never downloads. CI provisions it via
/// `npm ci --prefix crates/pyths_cli/tests/ts-toolchain` before the
/// workspace test job (see .github/workflows/ci.yml).
fn pinned_tsc() -> std::path::PathBuf {
    let tsc = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/ts-toolchain/node_modules/typescript/bin/tsc");
    assert!(
        tsc.exists(),
        "pinned TypeScript checker missing at {} — provision it with:\n  \
         npm ci --prefix crates/pyths_cli/tests/ts-toolchain\n\
         (the TS-.d.ts-run lane must not be skipped or fall back to an \
         ambient/downloaded checker)",
        tsc.display()
    );
    tsc
}

#[test]
fn lane5_ts_dts_typecheck_and_run() {
    // The TS-.d.ts-run lane: the emitted `.d.ts` is CONSUMED — a TypeScript
    // file imports the compiled module at its declared types, is type-checked
    // by a real checker (tsgo/tsc), compiled, and EXECUTED under node with
    // runtime assertions at every declared-type sink.
    //
    // HOW THIS CATCHES OPTION A: the reverted Option A (int = BigInt
    // unconditionally, merge c63d0dd7, tagged attempt/int-bigint-optA)
    // emitted `.d.ts` files declaring `int -> number` (dts.rs at that tag:
    // `"int" | "float" => TsType::Number`) while emit.rs emitted EVERY int
    // literal as `{}n` and `__intBin` computed every int op in BigInt with
    // `__norm = identity` (never demoting). `tsc --noEmit`-style gates
    // passed because they trust the declaration and never run the typed
    // consumer. The PRIMARY witness here is therefore a NO-ARGUMENT int
    // export (`answer()` / `answer_sum()`) whose value is produced entirely
    // inside compiled PythScribe: under Option A it is a bigint (verified
    // against the tag's emit.rs — an int literal return emits `return 42n;`
    // and an int-arithmetic return goes through the BigInt-only `__intBin`),
    // while its declaration stays `(): number`. NOTE an argument-taking
    // function is NOT a valid witness: Option A classified every inbound
    // native `number` as float (`__isFloat = typeof x === "number"`), so
    // `double_it(21)` took the float path and returned a native 42 — a
    // strawman that stays green under A. The no-arg witness closes the
    // hole from both sides:
    //   * If the declaration is HONEST about a breaking model (`bigint`),
    //     the type-check step fails (`const a: number = answer()` is a
    //     TS2322).
    //   * If the declaration LIES (`number` over a bigint runtime value),
    //     the type-check passes but the RUN step fails: `typeof a` is
    //     "bigint", `a === 42` is false for 42n, `a * 2` throws "Cannot
    //     mix BigInt and other types", `a.toFixed(0)` throws (no such
    //     method on BigInt), and `Math.max(a, 0)` throws "Cannot convert a
    //     BigInt value to a number". Either way the lane goes red —
    //     exactly the failure that shipped undetected before.
    // A deliberately ill-typed consumer is also compiled and must be
    // REJECTED with the SPECIFIC TS2322 diagnostics, proving the `.d.ts`
    // is really bound and non-`any` (the false-world control: a vacuous
    // declaration surface cannot pass).
    let dir = std::env::temp_dir().join(format!("pyths_optb_tslane_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();

    let ps = dir.join("mod.ps");
    std::fs::write(
        &ps,
        r#"def answer() -> int:
    return 42

def answer_sum() -> int:
    acc = 0
    for k in range(7):
        acc = acc + k
    return acc + 21

def double_it(n: int) -> int:
    return n * 2

def add_ints(a: int, b: int) -> int:
    return a + b

def pi_ish() -> float:
    return 2.5

def whole() -> float:
    return 8.0

def whole_list() -> list[float]:
    return [8.0, 9.0]

def total(xs: list[float]) -> float:
    s = 0.0
    for x in xs:
        s = s + x
    return s

def show_float(x: float) -> str:
    return str(x)
"#,
    )
    .unwrap();

    // Compile to a pure-JS module (+ .d.ts). `--target js` pins the module
    // to the JS lane — the WASM leg has its own lane above.
    let out_js = dir.join("mod.js");
    let out = pyths_bin()
        .args([
            "compile",
            ps.to_str().unwrap(),
            "-o",
            out_js.to_str().unwrap(),
            "--dts",
            "--target",
            "js",
        ])
        .output()
        .expect("pyths compile failed to spawn");
    assert!(
        out.status.success(),
        "compile: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let dts_path = dir.join("mod.d.ts");
    let dts = std::fs::read_to_string(&dts_path).expect(".d.ts must be emitted");
    // Sanity only — the real gate is the consumer below, not string matching.
    for decl in [
        "export declare function answer(): number;",
        "export declare function answer_sum(): number;",
        "export declare function double_it(n: number): number;",
        "export declare function pi_ish(): number;",
        "export declare function whole(): number;",
        "export declare function whole_list(): number[];",
        "export declare function show_float(x: number): string;",
    ] {
        assert!(dts.contains(decl), "missing `{decl}` in .d.ts:\n{dts}");
    }

    // ESM package root + the runtime the compiled module imports.
    std::fs::write(dir.join("package.json"), "{ \"type\": \"module\" }\n").unwrap();
    pyths_runtime::materialize_runtime_package(&dir).expect("materialize runtime");

    // The typed consumer: every export is used AT its declared `.d.ts` type.
    // `check` throws (non-zero node exit) on any mismatch, so the run step
    // is an assertion battery, not a smoke test.
    std::fs::write(
        dir.join("consumer.ts"),
        r#"import {
  answer,
  answer_sum,
  double_it,
  add_ints,
  pi_ish,
  whole,
  whole_list,
  total,
  show_float,
} from "./mod.js";

function check<T>(label: string, got: T, want: T): void {
  if (got !== want) {
    throw new Error(`${label}: got ${String(got)}, want ${String(want)}`);
  }
  console.log(`${label}=${String(got)}`);
}

const numberSink = (n: number): number => n + 1;

// PRIMARY Option-A witness — a NO-ARGUMENT int export, value produced
// entirely inside compiled PythScribe. Under the reverted Option A
// (attempt/int-bigint-optA) this is a bigint at runtime while the .d.ts
// says `number`: typeof/=== fail, and every native sink below throws.
// (An argument-taking export is NOT a witness: Option A classified any
// inbound native number as float, so it returned a native Number.)
const a: number = answer();
check("answer_typeof", typeof a, "number");
check("answer_strict_eq", a === 42, true);
check("answer_mixed_mul", a * 2, 84);
check("answer_toFixed", a.toFixed(0), "42");
check("answer_math_sink", Math.max(a, 0), 42);
check("answer_number_sink", numberSink(a), 43);
const asum: number = answer_sum();
check("answer_sum_typeof", typeof asum, "number");
check("answer_sum_strict_eq", asum === 42, true);

// Inbound-int path: native `number` arguments at declared int params must
// come back as native numbers with exact small-int arithmetic. (Under
// Option A these stayed green via the float classification — they guard
// the Option-B contract, not the Option-A revert.)
const i: number = double_it(21);
check("int_strict_eq", i === 42, true);
check("int_typeof", typeof i, "number");
check("int_mixed_mul", i * 2, 84);
check("int_toFixed", i.toFixed(0), "42");
check("int_math_sink", Math.max(i, 0), 42);
check("int_number_sink", numberSink(i), 43);
check("int_inbound", add_ints(20, 22), 42);

// plain (non-integer-valued) float: stays a native number, full identity.
const f: number = pi_ish();
check("float_strict_eq", f === 2.5, true);
check("float_arith", f + 1, 3.5);

// integer-valued float: arrives as the PyFloat carrier (a boxed Number
// OBJECT), which must BEHAVE as a number at every declared-type use
// (ToNumber, methods via the [[NumberData]] slot) — and its float-ness
// must survive the crossing (show_float round-trip reprs "8.0", never
// "8"). DELIBERATELY NOT CLAIMED here: primitive-number identity
// (`typeof g === "number"`, `g === 8`) — integer-valued float returns
// cross the boundary boxed (typeof "object", strict-eq false), so the
// `number` declaration is behavioral, not primitive-exact, for them.
// That boundary gap is tracked separately (#462); this lane asserts the
// DOCUMENTED Option-B contract only. Primitive identity is asserted
// solely for true-int returns above, where it must hold.
const g: number = whole();
check("ifloat_arith", g + 1, 9);
check("ifloat_math_sink", Math.max(g, 1), 8);
check("ifloat_toFixed", g.toFixed(2), "8.00");
check("ifloat_tonumber", Number(g) === 8, true);
check("ifloat_repr_roundtrip", show_float(g), "8.0");
check("float_repr_plain", show_float(f), "2.5");
check("float_repr_inbound", show_float(4), "4.0");

// container of integer-valued floats at declared `number[]`.
const xs: number[] = whole_list();
check("list_len", xs.length, 2);
check("list_index_arith", xs[0] + 1, 9);
check("list_spread_math", Math.min(...xs), 8);
check("list_map", xs.map((x) => x * 2).join(","), "16,18");
check("list_inbound_total", Number(total([1.5, 2.5])), 4);

console.log("TS-LANE-OK");
"#,
    )
    .unwrap();

    // False-world control: a consumer that uses the exports at WRONG types
    // must be rejected — proves the `.d.ts` really binds and is non-`any`.
    std::fs::write(
        dir.join("consumer_bad.ts"),
        r#"import { double_it, whole_list } from "./mod.js";

const s: string = double_it(21);
const b: boolean[] = whole_list();
console.log(s, b);
"#,
    )
    .unwrap();

    let ts_opts = r#"{
  "compilerOptions": {
    "strict": true,
    "target": "es2022",
    "module": "nodenext",
    "moduleResolution": "nodenext",
    "skipLibCheck": true,
    "types": []
  },
"#;
    std::fs::write(
        dir.join("tsconfig.json"),
        format!("{ts_opts}  \"include\": [\"consumer.ts\"]\n}}\n"),
    )
    .unwrap();
    std::fs::write(
        dir.join("tsconfig.bad.json"),
        format!(
            "{}  \"include\": [\"consumer_bad.ts\"]\n}}\n",
            ts_opts.replace("\"types\": []", "\"types\": [],\n    \"noEmit\": true")
        ),
    )
    .unwrap();

    let tsc = pinned_tsc();

    // Step 1 — TYPE-CHECK the consumer against the emitted .d.ts (and emit
    // consumer.js). This is the gate that trusts the declaration...
    let tc = Command::new("node")
        .arg(&tsc)
        .args(["-p", "tsconfig.json", "--pretty", "false"])
        .current_dir(&dir)
        .output()
        .expect("node (pinned tsc) failed to spawn");
    assert!(
        tc.status.success(),
        "typed consumer must type-check clean against the emitted .d.ts:\n{}\n{}",
        String::from_utf8_lossy(&tc.stdout),
        String::from_utf8_lossy(&tc.stderr)
    );

    // ...and the false-world control proves that gate has teeth. A bare
    // nonzero exit is NOT enough (a broken tsconfig or crashed checker
    // would also exit nonzero): the run must be rejected FOR THE RIGHT
    // REASON — one TS2322 assignability diagnostic per ill-typed
    // assignment, at its exact site.
    let tc_bad = Command::new("node")
        .arg(&tsc)
        .args(["-p", "tsconfig.bad.json", "--pretty", "false"])
        .current_dir(&dir)
        .output()
        .expect("node (pinned tsc) failed to spawn");
    let bad_diag = format!(
        "{}{}",
        String::from_utf8_lossy(&tc_bad.stdout),
        String::from_utf8_lossy(&tc_bad.stderr)
    );
    assert!(
        !tc_bad.status.success(),
        "the ill-typed consumer MUST be rejected — if it passes, the .d.ts \
         surface has decayed to `any` and the type-check step is vacuous:\n{bad_diag}"
    );
    for expected in [
        "consumer_bad.ts(3,7): error TS2322: Type 'number' is not assignable to type 'string'.",
        "consumer_bad.ts(4,7): error TS2322: Type 'number[]' is not assignable to type 'boolean[]'.",
    ] {
        assert!(
            bad_diag.contains(expected),
            "false-world control must fail with the SPECIFIC assignability \
             diagnostic `{expected}` (not an unrelated config/crash exit); got:\n{bad_diag}"
        );
    }

    // Step 2 — RUN the checked consumer. This is the gate the declaration
    // cannot fool: values are asserted at their declared-type sinks.
    let consumer_js = dir.join("consumer.js");
    assert!(consumer_js.exists(), "checker must emit consumer.js");
    let node = Command::new("node")
        .arg(consumer_js.to_str().unwrap())
        .output()
        .expect("node failed to spawn");
    let stdout = String::from_utf8_lossy(&node.stdout).replace("\r\n", "\n");
    let stderr = String::from_utf8_lossy(&node.stderr);
    assert!(
        node.status.success(),
        "typed consumer must RUN clean at its declared types (no bigint \
         mixing/conversion throws, no box misbehavior); stderr={stderr}\nstdout={stdout}"
    );
    assert_eq!(
        stdout,
        "answer_typeof=number\n\
         answer_strict_eq=true\n\
         answer_mixed_mul=84\n\
         answer_toFixed=42\n\
         answer_math_sink=42\n\
         answer_number_sink=43\n\
         answer_sum_typeof=number\n\
         answer_sum_strict_eq=true\n\
         int_strict_eq=true\n\
         int_typeof=number\n\
         int_mixed_mul=84\n\
         int_toFixed=42\n\
         int_math_sink=42\n\
         int_number_sink=43\n\
         int_inbound=42\n\
         float_strict_eq=true\n\
         float_arith=3.5\n\
         ifloat_arith=9\n\
         ifloat_math_sink=8\n\
         ifloat_toFixed=8.00\n\
         ifloat_tonumber=true\n\
         ifloat_repr_roundtrip=8.0\n\
         float_repr_plain=2.5\n\
         float_repr_inbound=4.0\n\
         list_len=2\n\
         list_index_arith=9\n\
         list_spread_math=8\n\
         list_map=16,18\n\
         list_inbound_total=4\n\
         TS-LANE-OK\n"
    );

    let _ = std::fs::remove_dir_all(&dir);
}
