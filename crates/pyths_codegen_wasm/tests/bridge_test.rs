use std::collections::{BTreeMap, BTreeSet};

use pyths_codegen_wasm::types::WasmType;
use pyths_codegen_wasm::{generate_bridge_js, WasmCodegenOutput, WasmExportInfo};

fn make_output(exports: Vec<WasmExportInfo>, needs_pow: bool) -> WasmCodegenOutput {
    let needs_strings = exports.iter().any(|e| {
        e.params.iter().any(|(_, ty)| matches!(ty, WasmType::Ptr))
            || matches!(e.return_type, Some(WasmType::Ptr))
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
    assert!(
        glue.contains("instantiateStreaming"),
        "Has streaming: {}",
        glue
    );
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
