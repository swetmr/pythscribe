use std::collections::{BTreeMap, BTreeSet};

use pyths_codegen_wasm::types::WasmType;
use pyths_codegen_wasm::{generate_bridge_js, WasmCodegenOutput, WasmExportInfo};

fn make_output(exports: Vec<WasmExportInfo>, needs_pow: bool) -> WasmCodegenOutput {
    // Mirror the real compiler: `needs_strings` (== "needs the heap/arena") is set
    // for ANY heap-boundary param/return, which `is_any_ptr()` captures (string,
    // list, dict, tuple, closure, AND array) — so array/list tests exercise the
    // production arena `try/finally` placement of the marshalling prelude, not a
    // heap-less shape (bridge review nit, 2026-09-04).
    let needs_strings = exports.iter().any(|e| {
        e.params.iter().any(|(_, ty)| ty.is_any_ptr())
            || e.return_type.as_ref().is_some_and(|ty| ty.is_any_ptr())
    });
    let mut math_imports = BTreeSet::new();
    if needs_pow {
        math_imports.insert("pow".to_string());
    }
    WasmCodegenOutput {
        wasm: vec![0x00], // dummy non-empty
        compiled_functions: exports.iter().map(|e| e.name.clone()).collect(),
        rejected_functions: vec![],
        export_info: exports,
        math_imports,
        needs_strings,
        needs_errors: false,
        needs_dicts: false,
        custom_exceptions: BTreeMap::new(),
        has_ovf: false,
    }
}

fn make_output_with_imports(
    exports: Vec<WasmExportInfo>,
    math_imports: BTreeSet<String>,
) -> WasmCodegenOutput {
    let needs_strings = exports.iter().any(|e| {
        e.params.iter().any(|(_, ty)| matches!(ty, WasmType::Ptr))
            || matches!(e.return_type, Some(WasmType::Ptr))
    });
    WasmCodegenOutput {
        wasm: vec![0x00],
        compiled_functions: exports.iter().map(|e| e.name.clone()).collect(),
        rejected_functions: vec![],
        export_info: exports,
        math_imports,
        needs_strings,
        needs_errors: false,
        needs_dicts: false,
        custom_exceptions: BTreeMap::new(),
        has_ovf: false,
    }
}

fn make_output_with_errors(exports: Vec<WasmExportInfo>) -> WasmCodegenOutput {
    let needs_strings = exports.iter().any(|e| {
        e.params.iter().any(|(_, ty)| matches!(ty, WasmType::Ptr))
            || matches!(e.return_type, Some(WasmType::Ptr))
    });
    WasmCodegenOutput {
        wasm: vec![0x00],
        compiled_functions: exports.iter().map(|e| e.name.clone()).collect(),
        rejected_functions: vec![],
        export_info: exports,
        math_imports: BTreeSet::new(),
        needs_strings,
        needs_errors: true,
        needs_dicts: false,
        custom_exceptions: BTreeMap::new(),
        has_ovf: false,
    }
}

#[test]
fn test_bridge_int_function() {
    let output = make_output(
        vec![WasmExportInfo {
            name: "add".to_string(),
            params: vec![
                ("a".to_string(), WasmType::I64),
                ("b".to_string(), WasmType::I64),
            ],
            return_type: Some(WasmType::I64),
        }],
        false,
    );
    let glue = generate_bridge_js(&output, "./test.wasm", None);
    // Int params accept a BigInt arg as-is (arbitrary-precision value) or
    // coerce a Number via BigInt(Math.trunc(...)).
    assert!(
        glue.contains("typeof a === \"bigint\" ? a : BigInt(Math.trunc(a))"),
        "Int param conversion: {}",
        glue
    );
    assert!(
        glue.contains("typeof b === \"bigint\" ? b : BigInt(Math.trunc(b))"),
        "Int param conversion: {}",
        glue
    );
    // Int return normalizes the i64 BigInt: Number when safe, BigInt past
    // 2**53 (preserves arbitrary precision — bare Number() would lose it).
    assert!(
        glue.contains("__i64ToJs("),
        "Int return conversion: {}",
        glue
    );
    assert!(
        glue.contains("const __i64ToJs ="),
        "i64 normalizer present: {}",
        glue
    );
}

#[test]
fn test_bridge_float_function() {
    let output = make_output(
        vec![WasmExportInfo {
            name: "square".to_string(),
            params: vec![("x".to_string(), WasmType::F64)],
            return_type: Some(WasmType::F64),
        }],
        false,
    );
    let glue = generate_bridge_js(&output, "./test.wasm", None);
    // Value-boundary authority (#38/#461/#465): an f64 param accepts the
    // hybrid int's BigInt form via the glue-local __f64Arg (ToNumber at the
    // WASM JS-API boundary THROWS on BigInt) — the SAME rule as the runtime's
    // __reqNum, including the overflow branch: a BigInt beyond double range
    // raises OverflowError, never a silent Infinity. A native Number still
    // passes through untouched, and there is no Math.trunc truncation.
    assert!(
        !glue.contains("Math.trunc"),
        "No Math.trunc for float: {}",
        glue
    );
    // The helper itself must be emitted (mirror of __reqNum's BigInt branch).
    assert!(
        glue.contains("const __f64Arg = (v) => { if (typeof v !== \"bigint\") return v; const n = Number(v); if (Number.isFinite(n)) return n; const e = new Error(\"int too large to convert to float\"); e.name = \"OverflowError\"; throw e; };"),
        "f64 arg authority helper present: {}",
        glue
    );
    // Option B: an f64 result re-enters JS through __f64Box (boxes iff
    // integer-valued, so square(2.0) -> 4.0 keeps float identity; 2.25
    // stays a native Number). Still no Number() truncation wrapping.
    assert!(
        glue.contains("return __f64Box(__wasm.square(__f64Arg(x)))"),
        "Float return boxed-iff-integer-valued, arg coerced-on-bigint: {}",
        glue
    );
}

#[test]
fn test_bridge_bool_return() {
    let output = make_output(
        vec![WasmExportInfo {
            name: "is_even".to_string(),
            params: vec![("n".to_string(), WasmType::I64)],
            return_type: Some(WasmType::I32),
        }],
        false,
    );
    let glue = generate_bridge_js(&output, "./test.wasm", None);
    assert!(glue.contains("Boolean("), "Bool return wrapping: {}", glue);
}

#[test]
fn test_bridge_void_function() {
    let output = make_output(
        vec![WasmExportInfo {
            name: "do_nothing".to_string(),
            params: vec![("x".to_string(), WasmType::I64)],
            return_type: None,
        }],
        false,
    );
    let glue = generate_bridge_js(&output, "./test.wasm", None);
    // Void function â€” should call but NOT have "return __wasm." in the wrapper
    assert!(glue.contains("__wasm.do_nothing("), "Void call: {}", glue);
    assert!(
        !glue.contains("return __wasm.do_nothing"),
        "No return for void wrapper: {}",
        glue
    );
}

#[test]
fn test_bridge_pow_import() {
    let output = make_output(
        vec![WasmExportInfo {
            name: "power".to_string(),
            params: vec![
                ("base".to_string(), WasmType::F64),
                ("exp".to_string(), WasmType::F64),
            ],
            return_type: Some(WasmType::F64),
        }],
        true,
    );
    let glue = generate_bridge_js(&output, "./test.wasm", None);
    assert!(
        glue.contains("math: { pow: Math.pow }"),
        "Pow import: {}",
        glue
    );
}

#[test]
fn test_bridge_no_pow() {
    let output = make_output(
        vec![WasmExportInfo {
            name: "add".to_string(),
            params: vec![
                ("a".to_string(), WasmType::I64),
                ("b".to_string(), WasmType::I64),
            ],
            return_type: Some(WasmType::I64),
        }],
        false,
    );
    let glue = generate_bridge_js(&output, "./test.wasm", None);
    assert!(!glue.contains("Math.pow"), "No pow import: {}", glue);
    assert!(
        glue.contains("const imports = {};"),
        "Empty imports: {}",
        glue
    );
}

#[test]
fn test_bridge_multiple_functions() {
    let output = make_output(
        vec![
            WasmExportInfo {
                name: "add".to_string(),
                params: vec![
                    ("a".to_string(), WasmType::I64),
                    ("b".to_string(), WasmType::I64),
                ],
                return_type: Some(WasmType::I64),
            },
            WasmExportInfo {
                name: "square".to_string(),
                params: vec![("x".to_string(), WasmType::F64)],
                return_type: Some(WasmType::F64),
            },
        ],
        false,
    );
    let glue = generate_bridge_js(&output, "./test.wasm", None);
    assert!(glue.contains("export function add("), "Has add: {}", glue);
    assert!(
        glue.contains("export function square("),
        "Has square: {}",
        glue
    );
}

#[test]
fn test_bridge_streaming_fallback() {
    let output = make_output(
        vec![WasmExportInfo {
            name: "add".to_string(),
            params: vec![("a".to_string(), WasmType::I64)],
            return_type: Some(WasmType::I64),
        }],
        false,
    );
    let glue = generate_bridge_js(&output, "./test.wasm", None);
    assert!(glue.contains("compileStreaming"), "Has streaming: {}", glue);
    assert!(glue.contains("arrayBuffer"), "Has fallback: {}", glue);
    // Universal loader: detects Node and uses fs.readFile because
    // Node's fetch() can't resolve file: URLs.
    assert!(
        glue.contains("globalThis.process") && glue.contains("node:fs/promises"),
        "Has Node path: {}",
        glue
    );
}

#[test]
fn test_bridge_string_param() {
    let output = make_output(
        vec![WasmExportInfo {
            name: "greet".to_string(),
            params: vec![("name".to_string(), WasmType::Ptr)],
            return_type: Some(WasmType::Ptr),
        }],
        false,
    );
    let glue = generate_bridge_js(&output, "./test.wasm", None);
    // String param should use __str_to_wasm()
    assert!(
        glue.contains("__str_to_wasm(name)"),
        "String param conversion: {}",
        glue
    );
    // String return should use __str_from_wasm()
    assert!(
        glue.contains("__str_from_wasm("),
        "String return conversion: {}",
        glue
    );
}

#[test]
fn test_bridge_string_helpers() {
    let output = make_output(
        vec![WasmExportInfo {
            name: "identity".to_string(),
            params: vec![("s".to_string(), WasmType::Ptr)],
            return_type: Some(WasmType::Ptr),
        }],
        false,
    );
    let glue = generate_bridge_js(&output, "./test.wasm", None);
    // Should include TextEncoder/TextDecoder helpers
    assert!(
        glue.contains("new TextEncoder()"),
        "Has TextEncoder: {}",
        glue
    );
    assert!(
        glue.contains("new TextDecoder()"),
        "Has TextDecoder: {}",
        glue
    );
    assert!(
        glue.contains("function __str_to_wasm(s)"),
        "Has str_to_wasm: {}",
        glue
    );
    assert!(
        glue.contains("function __str_from_wasm(ptr)"),
        "Has str_from_wasm: {}",
        glue
    );
    // Should call __alloc
    assert!(glue.contains("__wasm.__alloc("), "Calls __alloc: {}", glue);
}

#[test]
fn test_bridge_math_imports_object() {
    let mut imports = BTreeSet::new();
    imports.insert("sqrt".to_string());
    imports.insert("sin".to_string());
    imports.insert("pow".to_string());
    let output = make_output_with_imports(
        vec![WasmExportInfo {
            name: "f".to_string(),
            params: vec![("x".to_string(), WasmType::F64)],
            return_type: Some(WasmType::F64),
        }],
        imports,
    );
    let glue = generate_bridge_js(&output, "./test.wasm", None);
    // Imports should appear in alphabetical order (BTreeSet)
    assert!(glue.contains("pow: Math.pow"), "pow: {}", glue);
    assert!(glue.contains("sin: Math.sin"), "sin: {}", glue);
    assert!(glue.contains("sqrt: Math.sqrt"), "sqrt: {}", glue);
    // All under math namespace
    assert!(glue.contains("math: {"), "math namespace: {}", glue);
}

#[test]
fn test_bridge_atan2_import() {
    let mut imports = BTreeSet::new();
    imports.insert("atan2".to_string());
    let output = make_output_with_imports(
        vec![WasmExportInfo {
            name: "a".to_string(),
            params: vec![
                ("y".to_string(), WasmType::F64),
                ("x".to_string(), WasmType::F64),
            ],
            return_type: Some(WasmType::F64),
        }],
        imports,
    );
    let glue = generate_bridge_js(&output, "./test.wasm", None);
    assert!(glue.contains("atan2: Math.atan2"), "atan2 import: {}", glue);
}

#[test]
fn test_bridge_fabs_maps_to_abs() {
    // PythScribe math.fabs maps to JS Math.abs
    let mut imports = BTreeSet::new();
    imports.insert("fabs".to_string());
    let output = make_output_with_imports(
        vec![WasmExportInfo {
            name: "a".to_string(),
            params: vec![("x".to_string(), WasmType::F64)],
            return_type: Some(WasmType::F64),
        }],
        imports,
    );
    let glue = generate_bridge_js(&output, "./test.wasm", None);
    assert!(
        glue.contains("fabs: Math.abs"),
        "fabs maps to abs: {}",
        glue
    );
}

#[test]
fn test_bridge_error_handling() {
    let output = make_output_with_errors(vec![WasmExportInfo {
        name: "may_fail".to_string(),
        params: vec![("x".to_string(), WasmType::I64)],
        return_type: Some(WasmType::I64),
    }]);
    let glue = generate_bridge_js(&output, "./test.wasm", None);
    // Has the err-checking helper
    assert!(
        glue.contains("function __check_err()"),
        "Has check_err: {}",
        glue
    );
    // Reads __err_code global
    assert!(
        glue.contains("__wasm.__err_code.value"),
        "Reads err_code: {}",
        glue
    );
    // Has error name table
    assert!(glue.contains("__ERROR_NAMES"), "Has error names: {}", glue);
    assert!(glue.contains("ValueError"), "Has ValueError name: {}", glue);
    assert!(
        glue.contains("ZeroDivisionError"),
        "Has ZeroDivisionError: {}",
        glue
    );
    // Throws a JS Error
    assert!(glue.contains("throw err"), "Throws: {}", glue);
    // Wrapper calls __check_err
    assert!(glue.contains("__check_err()"), "Calls check_err: {}", glue);
}

#[test]
fn test_bridge_custom_exception_names() {
    let mut customs = BTreeMap::new();
    customs.insert("NotFound".to_string(), 100);
    customs.insert("Forbidden".to_string(), 101);
    let output = WasmCodegenOutput {
        wasm: vec![0x00],
        compiled_functions: vec!["f".to_string()],
        rejected_functions: vec![],
        export_info: vec![WasmExportInfo {
            name: "f".to_string(),
            params: vec![],
            return_type: Some(WasmType::I64),
        }],
        math_imports: BTreeSet::new(),
        needs_strings: false,
        needs_errors: true,
        needs_dicts: false,
        custom_exceptions: customs,
        has_ovf: false,
    };
    let glue = generate_bridge_js(&output, "./test.wasm", None);
    // Custom names should appear in the error name map
    assert!(
        glue.contains("100: \"NotFound\""),
        "NotFound in name map: {}",
        glue
    );
    assert!(
        glue.contains("101: \"Forbidden\""),
        "Forbidden in name map: {}",
        glue
    );
    // Built-ins still present
    assert!(
        glue.contains("'ValueError'"),
        "ValueError still present: {}",
        glue
    );
}

#[test]
fn test_bridge_dict_namespace_emitted() {
    let output = WasmCodegenOutput {
        wasm: vec![0x00],
        compiled_functions: vec!["f".to_string()],
        rejected_functions: vec![],
        export_info: vec![WasmExportInfo {
            name: "f".to_string(),
            params: vec![],
            return_type: Some(WasmType::I64),
        }],
        math_imports: BTreeSet::new(),
        needs_strings: true, // dicts use string keys
        needs_errors: false,
        custom_exceptions: BTreeMap::new(),
        needs_dicts: true,
        has_ovf: false,
    };
    let glue = generate_bridge_js(&output, "./t.wasm", None);
    // Dict host namespace present
    assert!(
        glue.contains("__dict_namespace"),
        "host namespace: {}",
        glue
    );
    assert!(glue.contains("__dict_new:"), "dict_new: {}", glue);
    assert!(glue.contains("__dict_set_str:"), "dict_set_str: {}", glue);
    assert!(glue.contains("__dict_get_str:"), "dict_get_str: {}", glue);
    assert!(glue.contains("__dict_has_str:"), "dict_has_str: {}", glue);
    assert!(glue.contains("__dict_len:"), "dict_len: {}", glue);
    // Imports object passes __dict through
    assert!(
        glue.contains("__dict: __dict_namespace"),
        "imports object: {}",
        glue
    );
}

#[test]
fn test_bridge_no_dict_namespace_when_not_needed() {
    let output = WasmCodegenOutput {
        wasm: vec![0x00],
        compiled_functions: vec!["f".to_string()],
        rejected_functions: vec![],
        export_info: vec![WasmExportInfo {
            name: "f".to_string(),
            params: vec![("a".to_string(), WasmType::I64)],
            return_type: Some(WasmType::I64),
        }],
        math_imports: BTreeSet::new(),
        needs_strings: false,
        needs_errors: false,
        custom_exceptions: BTreeMap::new(),
        needs_dicts: false,
        has_ovf: false,
    };
    let glue = generate_bridge_js(&output, "./t.wasm", None);
    assert!(!glue.contains("__dict_namespace"), "no dict namespace");
    assert!(!glue.contains("__dict_new"), "no dict_new");
}

#[test]
fn test_bridge_no_error_handling_when_not_needed() {
    let output = make_output(
        vec![WasmExportInfo {
            name: "add".to_string(),
            params: vec![("a".to_string(), WasmType::I64)],
            return_type: Some(WasmType::I64),
        }],
        false,
    );
    let glue = generate_bridge_js(&output, "./test.wasm", None);
    // No error helper when not needed
    assert!(!glue.contains("__check_err"), "No check_err: {}", glue);
    assert!(!glue.contains("__ERROR_NAMES"), "No error names: {}", glue);
}

#[test]
fn test_bridge_no_string_helpers_when_not_needed() {
    let output = make_output(
        vec![WasmExportInfo {
            name: "add".to_string(),
            params: vec![
                ("a".to_string(), WasmType::I64),
                ("b".to_string(), WasmType::I64),
            ],
            return_type: Some(WasmType::I64),
        }],
        false,
    );
    let glue = generate_bridge_js(&output, "./test.wasm", None);
    // No string helpers when no string params/returns
    assert!(!glue.contains("TextEncoder"), "No TextEncoder: {}", glue);
    assert!(!glue.contains("__str_to_wasm"), "No str_to_wasm: {}", glue);
    assert!(
        !glue.contains("__str_from_wasm"),
        "No str_from_wasm: {}",
        glue
    );
}

// ============================================================================
// B-031 regression: list-param/return exports must DEFINE the marshalling
// helpers (`__list_to_wasm` / `__list_from_wasm`), not just reference them.
// Before the fix the glue called `__list_to_wasm(...)` with no definition →
// ReferenceError at runtime for every WASM fn taking/returning a list.
// ============================================================================

#[test]
fn test_bridge_defines_list_helpers_for_list_param() {
    let output = make_output(
        vec![WasmExportInfo {
            name: "sum2".to_string(),
            params: vec![("a".to_string(), WasmType::PtrList(Box::new(WasmType::F64)))],
            return_type: Some(WasmType::F64),
        }],
        false,
    );
    let glue = generate_bridge_js(&output, "./test.wasm", None);
    // The helper must be DEFINED, not merely referenced.
    assert!(
        glue.contains("function __list_to_wasm"),
        "list-param glue must define __list_to_wasm: {}",
        glue
    );
    // The call site passes the element-kind tag derived from the element type.
    assert!(
        glue.contains("__list_to_wasm(a, \"f64\")"),
        "list-param call site must pass element kind: {}",
        glue
    );
    // The helper allocates via the module's exported __alloc + memory.
    assert!(
        glue.contains("__wasm.__alloc"),
        "helper uses __alloc: {}",
        glue
    );
    assert!(
        glue.contains("__wasm.memory.buffer"),
        "helper uses memory: {}",
        glue
    );
}

#[test]
fn test_bridge_writes_back_list_out_param() {
    // #484: SYMMETRIC marshalling. A mutable list param must be marshalled
    // through a NAMED local and copied BACK on return, so an in-place fill
    // kernel's mutation is visible to the caller (the pre-fix glue dropped it).
    let output = make_output(
        vec![WasmExportInfo {
            name: "fill".to_string(),
            params: vec![
                (
                    "out".to_string(),
                    WasmType::PtrList(Box::new(WasmType::I64)),
                ),
                ("n".to_string(), WasmType::I64),
            ],
            return_type: Some(WasmType::I64),
        }],
        false,
    );
    let glue = generate_bridge_js(&output, "./test.wasm", None);
    // The write-back helper must be DEFINED (the ONE authority)...
    assert!(
        glue.contains("function __list_write_back(arr, ptr, kind)"),
        "list-out-param glue must define __list_write_back: {glue}"
    );
    // ...the list arg hoisted to a named local so its pointer survives the call...
    assert!(
        glue.contains("const __wb_arg_0 = __list_to_wasm(out, \"i64\")")
            && glue.contains("__wasm.fill(__wb_arg_0,"),
        "list arg must be hoisted and passed by its local: {glue}"
    );
    // ...and written BACK to the SAME JS array after the call.
    assert!(
        glue.contains("__list_write_back(out, __wb_arg_0, \"i64\")"),
        "list out-param must be written back on return: {glue}"
    );
    // The fixed-capacity contract is enforced LOUDLY (never a silent truncation).
    assert!(
        glue.contains("if (n !== arr.length) throw new Error("),
        "write-back must refuse a length-changing mutation loudly: {glue}"
    );
    // The i32 (bool) branch yields a real boolean for a plain array so the glue
    // matches the JS twin; a TypedArray caller (the low-level channel) gets the
    // type-compatible value instead — so write-back never throws on a valid
    // read-only call made with a BigInt64Array (codex review, 2026-09-04).
    assert!(
        glue.contains("const typed = ArrayBuffer.isView(arr);")
            && glue.contains("arr[i] = typed ? v : ((v >= -9007199254740991n && v <= 9007199254740991n) ? Number(v) : v);")
            && glue.contains("arr[i] = typed ? v : (v !== 0);"),
        "write-back must be TypedArray-safe (BigInt for BigInt64Array) and boolean for a plain list[bool]: {glue}"
    );
}

#[test]
fn test_bridge_no_write_back_without_list_param() {
    // Byte-identity guard: a function WITHOUT list params emits NO write-back
    // machinery — the fix is inert for the scalar/str surface.
    let output = make_output(
        vec![WasmExportInfo {
            name: "add".to_string(),
            params: vec![
                ("a".to_string(), WasmType::I64),
                ("b".to_string(), WasmType::I64),
            ],
            return_type: Some(WasmType::I64),
        }],
        false,
    );
    let glue = generate_bridge_js(&output, "./test.wasm", None);
    assert!(
        !glue.contains("__list_write_back") && !glue.contains("__wb_arg_"),
        "no-list glue must not emit write-back machinery: {glue}"
    );
}

#[test]
fn test_bridge_defines_list_from_wasm_for_list_return() {
    let output = make_output(
        vec![WasmExportInfo {
            name: "mk".to_string(),
            params: vec![],
            return_type: Some(WasmType::PtrList(Box::new(WasmType::I64))),
        }],
        false,
    );
    let glue = generate_bridge_js(&output, "./test.wasm", None);
    assert!(
        glue.contains("function __list_from_wasm"),
        "list-return glue must define __list_from_wasm: {}",
        glue
    );
    assert!(
        glue.contains("__list_from_wasm(") && glue.contains("\"i64\""),
        "list-return call site must pass element kind: {}",
        glue
    );
}

// ============================================================================
// Marshalling-table finding: i64 LIST ELEMENTS must be oob-guarded. DataView
// setBigInt64 silently wraps mod 2**64 (ES ToBigInt64), so before the guard
// `pick([2**63+7])` crossed the boundary as -9223372036854775801 (PoC,
// 2026-08-16) while the SCALAR i64 arg path was already `__i64Oob`-guarded.
// The guard throws RangeError — a boundary marshalling fault, which the #364
// fallback ladder re-runs on the exact JS twin (js+wasm) or fails loud (edge).
// The disposition is pinned as the `list-elem-i64-oob` rows of
// verification/marshalling-table.txt.
// ============================================================================

#[test]
fn list_i64_elements_are_oob_guarded() {
    let output = make_output(
        vec![WasmExportInfo {
            name: "pick".to_string(),
            params: vec![("xs".to_string(), WasmType::PtrList(Box::new(WasmType::I64)))],
            return_type: Some(WasmType::I64),
        }],
        false,
    );
    let glue = generate_bridge_js(&output, "./test.wasm", None);
    // The i64 element write must be range-guarded before setBigInt64...
    assert!(
        glue.contains("if (b > 9223372036854775807n || b < -9223372036854775808n) throw new RangeError('OverflowError: list element exceeds the i64 range of the WASM fast path');"),
        "i64 list elements must be oob-guarded: {}",
        glue
    );
    // ...and the store must go through the guarded local, never an unguarded
    // inline coercion (the pre-fix silent-wrap shape).
    assert!(
        glue.contains("view.setBigInt64(off, b, true);"),
        "i64 element store must use the guarded value: {}",
        glue
    );
    assert!(
        !glue.contains("view.setBigInt64(off, typeof arr[i]"),
        "unguarded inline i64 element coercion must be gone: {}",
        glue
    );
}

#[test]
fn test_bridge_no_list_helpers_when_not_needed() {
    let output = make_output(
        vec![WasmExportInfo {
            name: "add".to_string(),
            params: vec![
                ("a".to_string(), WasmType::I64),
                ("b".to_string(), WasmType::I64),
            ],
            return_type: Some(WasmType::I64),
        }],
        false,
    );
    let glue = generate_bridge_js(&output, "./test.wasm", None);
    assert!(
        !glue.contains("__list_to_wasm"),
        "No list helpers when unused: {}",
        glue
    );
    assert!(
        !glue.contains("__list_from_wasm"),
        "No list helpers when unused: {}",
        glue
    );
}

// ---------------------------------------------------------------------------
// M2a-3b: the js+wasm TYPED-ARRAY glue — layout byte-agreement + refusal channel
// ---------------------------------------------------------------------------

/// The typed-array HEADER LAYOUT the glue emits is byte-identical to the SERVER
/// channel `pythscribe/runtime/array_buffer.py` and to the codegen's `PtrArray`
/// load/store (`emit.rs`: `ARRAY_ELEM_OFFSET = 16`, `shape0 @ +8`). This test
/// PINS every offset + dtype tag as an exact string, so ANY layout drift
/// (an offset moved, a tag renumbered) goes RED — the K7 declared-binding
/// discipline extended to the array header (the paired negative control for the
/// layout: the assertions below cannot pass on a shifted header).
#[test]
fn test_bridge_array_header_layout_byte_agreement() {
    use pyths_types::types::ArrayDtype;
    // One export per dtype so every `__ARR_DTYPES` tag/width row is emitted.
    let dtypes = [
        ArrayDtype::Int32,
        ArrayDtype::Int64,
        ArrayDtype::Float32,
        ArrayDtype::Float64,
        ArrayDtype::Uint8,
    ];
    let exports: Vec<WasmExportInfo> = dtypes
        .iter()
        .map(|dt| WasmExportInfo {
            name: format!("k_{}", dt.spelling()),
            params: vec![(
                "a".to_string(),
                WasmType::PtrArray {
                    dtype: *dt,
                    ndim: 1,
                },
            )],
            return_type: None,
        })
        .collect();
    let output = make_output(exports, false);
    let glue = generate_bridge_js(&output, "./test.wasm", None);

    // The marshalling helpers must be DEFINED (referenced-but-undefined would be
    // a ReferenceError at runtime — the B-031 class).
    assert!(
        glue.contains("function __array_to_wasm(arr, dtype, ndim)")
            && glue.contains("function __array_write_back(arr, ptr, dtype, ndim)"),
        "array glue must define both marshallers: {glue}"
    );

    // dtype tag codes == `ArrayDtype` declaration order == `array_buffer.DTYPE_TAG`
    // (int32=0, int64=1, float32=2, float64=3, uint8=4) with the correct element
    // widths. The `esize` is bound to `ArrayDtype::size_bytes` (the SINGLE SOURCE
    // the codegen load/store width also derives from) and checked ROW-LOCALLY —
    // this is the shipping-binding of the WIDTH leg of the Lean `.ptrArray`
    // value-exactness (the M2a-4a marshalling row carries only the dtype spelling,
    // not the width; without a row-local size_bytes bind, flipping one dtype's
    // esize to another admitted width — e.g. int32→8 — would slip past a
    // whole-glue `contains("esize: 4")` because float32 also has esize 4. This
    // closes the M2a-4a-review SF-1 gap in-chunk).
    for (dt, ctor, tag) in [
        (ArrayDtype::Int32, "Int32Array", 0),
        (ArrayDtype::Int64, "BigInt64Array", 1),
        (ArrayDtype::Float32, "Float32Array", 2),
        (ArrayDtype::Float64, "Float64Array", 3),
        (ArrayDtype::Uint8, "Uint8Array", 4),
    ] {
        let spelling = dt.spelling();
        let esize = dt.size_bytes(); // the type-system single source of the width
                                     // Extract THIS dtype's `__ARR_DTYPES` row (the line `  <spelling>: { … }`)
                                     // and assert ctor+esize+tag are ALL on it — a wrong width on one dtype is
                                     // caught even when a sibling dtype legitimately shares that width.
        let row = glue
            .lines()
            .find(|l| l.trim_start().starts_with(&format!("{spelling}:")))
            .unwrap_or_else(|| panic!("missing __ARR_DTYPES.{spelling} row: {glue}"));
        assert!(
            row.contains(&format!("ctor: {ctor}"))
                && row.contains(&format!("esize: {esize}"))
                && row.contains(&format!("tag: {tag}")),
            "dtype {spelling} row must bind ctor={ctor} esize={esize} (=size_bytes) tag={tag}: {row}"
        );
    }

    // THE HEADER LAYOUT (16-byte times-8): dtype@0, ndim@4, shape0(rows)@8,
    // shape1(cols; 0 for 1-D)@12, elems@16 (M2b: the M2a `pad` @12 is now
    // shape1). Byte-identical to array_buffer.py `ARRAY_HEADER_BYTES=16` /
    // `shape0 @8` / `shape1 @12` and to emit.rs `ARRAY_ELEM_OFFSET=16` /
    // `ARRAY_COLS_OFFSET=12`. A shift of ANY of these fails this test.
    assert!(
        glue.contains("view.setInt32(ptr, d.tag, true);"),
        "dtype tag @0: {glue}"
    );
    assert!(
        glue.contains("view.setInt32(ptr + 4, ndim, true);"),
        "ndim @4: {glue}"
    );
    assert!(
        glue.contains("view.setInt32(ptr + 8, nrows, true);"),
        "shape0 (rows) @8: {glue}"
    );
    assert!(
        glue.contains("view.setInt32(ptr + 12, ncols, true);"),
        "shape1 (cols) @12: {glue}"
    );
    assert!(
        glue.contains("new d.ctor(__wasm.memory.buffer, ptr + 16, n).set(arr);"),
        "1-D elements @16, bulk .set() copy IN: {glue}"
    );
    // M2b: 2-D lays each row contiguously at ptr+16 + r*ncols*esize (row-major).
    assert!(
        glue.contains(
            "new d.ctor(__wasm.memory.buffer, ptr + 16 + r * ncols * d.esize, ncols).set(arr[r]);"
        ),
        "2-D rows @16 row-major, one bulk .set() per row: {glue}"
    );
    // Allocation is header+elements rounded to a multiple of 8, and the returned
    // base is aligned UP to 8 (a TypedArray view over WASM memory throws on a
    // misaligned byte offset — requirements B3).
    assert!(
        glue.contains("(16 + n * d.esize + 7) & ~7")
            && glue.contains("const ptr = (raw + 7) & ~7;"),
        "alloc rounds to x8 and returns an 8-aligned base (B3): {glue}"
    );

    // Write-back reads the SAME offsets and re-validates the header (drift guard):
    // tag@0, ndim@4, shape0@8, shape1@12, elems@16 — a corrupted header is
    // refused LOUDLY.
    assert!(
        glue.contains("const tag = view.getInt32(ptr, true);")
            && glue.contains("const hdrNdim = view.getInt32(ptr + 4, true);")
            && glue.contains("const nrows = view.getInt32(ptr + 8, true);")
            && glue.contains("const ncols = view.getInt32(ptr + 12, true);")
            && glue.contains("arr.set(new d.ctor(__wasm.memory.buffer, ptr + 16, nrows));"),
        "write-back must read the header at the same offsets + bulk-copy elems@16: {glue}"
    );
    assert!(
        glue.contains("if (tag !== d.tag) throw new Error(")
            && glue.contains("if (hdrNdim !== ndim) throw new Error(")
            && glue.contains("if (nrows !== arr.length) throw new Error("),
        "write-back must refuse a drifted tag/ndim/length LOUDLY (never a silent misread): {glue}"
    );
}

/// The TOTAL runtime check (requirements B1) is a `RangeError` — a boundary
/// marshalling fault the #364 ladder (`__isWasmFault = ... || e instanceof
/// RangeError`) reroutes to the JS twin, never a silent wrong-width read. This
/// pins the refusal CHANNEL: a wrong-dtype / non-TypedArray / wrong-ndim buffer
/// throws `RangeError` inside `__array_to_wasm` (so a twin-bearing js+wasm target
/// reroutes; a twinless edge target surfaces the loud error). The paired
/// negative control is the assertion that this is a `RangeError` (reroutes), not
/// a silent flat-copy.
#[test]
fn test_bridge_array_mismatch_throws_rangeerror_reroute_channel() {
    use pyths_types::types::ArrayDtype;
    let output = make_output(
        vec![WasmExportInfo {
            name: "k".to_string(),
            params: vec![(
                "a".to_string(),
                WasmType::PtrArray {
                    dtype: ArrayDtype::Int32,
                    ndim: 1,
                },
            )],
            return_type: None,
        }],
        false,
    );
    let glue = generate_bridge_js(&output, "./test.wasm", None);
    // dtype/width mismatch AND a non-TypedArray both refused via RangeError.
    // The total per-row check compares BOTH constructor and element width
    // (shared by 1-D and each 2-D row via `__arrRowOk`).
    assert!(
        glue.contains("row.constructor === d.ctor && row.BYTES_PER_ELEMENT === d.esize"),
        "the total check (__arrRowOk) must compare BOTH constructor and element width: {glue}"
    );
    assert!(
        glue.contains("throw new RangeError('pythscribe: Array['"),
        "a dtype/width mismatch must throw a RangeError (reroutes to the twin): {glue}"
    );
    // M2b: ndim ∉ {1, 2} is refused via RangeError (the 2-D path is admitted; a
    // 3-D / bad ndim reroutes to the twin, never a silent misread).
    assert!(
        glue.contains("Array marshalling supports ndim") && glue.contains("throw new RangeError("),
        "a bad ndim must throw a RangeError: {glue}"
    );
    // The fault predicate that reroutes it must be present in a twin build. Real
    // array kernels are `has_ovf` (they carry int arithmetic), so js+wasm emits
    // the #364 ladder; construct such an output explicitly (make_output defaults
    // has_ovf=false, which suppresses the twin ladder).
    let ovf_output = WasmCodegenOutput {
        wasm: vec![0x00],
        compiled_functions: vec!["k".to_string()],
        rejected_functions: vec![],
        export_info: vec![WasmExportInfo {
            name: "k".to_string(),
            params: vec![(
                "a".to_string(),
                WasmType::PtrArray {
                    dtype: ArrayDtype::Int32,
                    ndim: 1,
                },
            )],
            return_type: None,
        }],
        math_imports: BTreeSet::new(),
        needs_strings: true,
        needs_errors: false,
        needs_dicts: false,
        custom_exceptions: BTreeMap::new(),
        has_ovf: true,
    };
    let twin_glue = generate_bridge_js(&ovf_output, "./test.wasm", Some("export function k(a){}"));
    assert!(
        twin_glue.contains("e instanceof RangeError"),
        "the #364 fault predicate must classify RangeError a fault (reroute to twin): {twin_glue}"
    );
    // ...and the array-mismatch RangeError is thrown INSIDE the try body the
    // ladder wraps, so the reroute is actually reachable for an array param.
    assert!(
        twin_glue.contains("__array_to_wasm(a,")
            && twin_glue.contains("if (__isWasmFault(__e)) return __jsfb.k(a);"),
        "the array marshaller runs inside the #364 try/catch that reroutes on fault: {twin_glue}"
    );
}
