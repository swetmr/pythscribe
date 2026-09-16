//! M2.1 (spec 13-09-26-lib-selfcontained-pip-wheel §5.6): the emitted module carries
//! the `pyths.abi` custom section + the exported immutable `__pyths_abi` global, and
//! the generated glue inlines the SAME constants — all from the one source `abi.rs`.
//! The round trip is asserted BYTE-EXACT (section payload == `abi_section_json()`),
//! so the emit path cannot drop or reorder a field.

use pyths_codegen_wasm::abi;
use pyths_codegen_wasm::{codegen_wasm, generate_bridge_js};
use wasmparser::{ExternalKind, Operator, Parser, Payload, ValType};

const SRC: &str = "def hello(x: float) -> float:\n    return x * 2.0 + 1.0\n";

fn compile(src: &str) -> pyths_codegen_wasm::WasmCodegenOutput {
    let module = pyths_parser::parse(src).expect("parse");
    let out = codegen_wasm(&module);
    assert!(
        !out.wasm.is_empty(),
        "rejected: {:?}",
        out.rejected_functions
    );
    wasmparser::validate(&out.wasm).expect("emitted module validates");
    out
}

#[test]
fn emitted_section_roundtrips_abi_rs_byte_exact() {
    let out = compile(SRC);
    // via the dependency-free reader (what optimize.rs re-append keys on)
    let secs = abi::read_abi_sections(&out.wasm);
    assert_eq!(secs.len(), 1, "exactly one pyths.abi section");
    assert_eq!(secs[0], abi::abi_section_json().as_bytes());
    assert!(abi::has_current_abi_section(&out.wasm));
    // via wasmparser (an independent decoder of the same bytes)
    let mut seen = Vec::new();
    for payload in Parser::new(0).parse_all(&out.wasm) {
        if let Payload::CustomSection(c) = payload.expect("payload") {
            if c.name() == abi::ABI_SECTION_NAME {
                seen.push(c.data().to_vec());
            }
        }
    }
    assert_eq!(seen.len(), 1);
    assert_eq!(seen[0], abi::abi_section_json().as_bytes());
    let text = std::str::from_utf8(&seen[0]).unwrap();
    assert!(text.starts_with(&format!("{{\"abi\":{},\"list_layout\":\"{}\",\"array_layout\":\"{}\",\"compiler\":\"{}\",\"history\":[",
        abi::PYTHS_ABI_MAJOR, abi::LIST_LAYOUT_VERSION, abi::ARRAY_LAYOUT_VERSION, abi::COMPILER_VERSION)), "{text}");
}

#[test]
fn exported_abi_global_is_immutable_i32_equal_to_major() {
    let out = compile(SRC);
    let mut global_index: Option<u32> = None;
    let mut globals: Vec<(bool, ValType, i32)> = Vec::new();
    for payload in Parser::new(0).parse_all(&out.wasm) {
        match payload.expect("payload") {
            Payload::ExportSection(r) => {
                for e in r {
                    let e = e.expect("export");
                    if e.name == abi::ABI_GLOBAL_EXPORT {
                        assert_eq!(
                            e.kind,
                            ExternalKind::Global,
                            "__pyths_abi must be a global export"
                        );
                        global_index = Some(e.index);
                    }
                }
            }
            Payload::GlobalSection(r) => {
                for g in r {
                    let g = g.expect("global");
                    let mut ops = g.init_expr.get_operators_reader();
                    let v = match ops.read().expect("init op") {
                        Operator::I32Const { value } => value,
                        other => panic!("unexpected init expr {other:?}"),
                    };
                    globals.push((g.ty.mutable, g.ty.content_type, v));
                }
            }
            _ => {}
        }
    }
    let idx = global_index.expect("the module exports __pyths_abi") as usize;
    let (mutable, ty, value) = globals[idx];
    assert!(!mutable, "__pyths_abi must be immutable");
    assert_eq!(ty, ValType::I32);
    assert_eq!(value, abi::PYTHS_ABI_MAJOR as i32);
    // and it is the LAST global, so no pre-existing global index moved
    assert_eq!(idx, globals.len() - 1);
}

#[test]
fn glue_inlines_the_same_constants_and_checks_before_handing_out_exports() {
    let out = compile(SRC);
    let glue = generate_bridge_js(&out, "./hello.wasm", None);
    let expect = format!(
        "const __PYTHS_ABI = {{ abi: {}, list_layout: {:?}, array_layout: {:?} }};",
        abi::PYTHS_ABI_MAJOR,
        abi::LIST_LAYOUT_VERSION,
        abi::ARRAY_LAYOUT_VERSION
    );
    assert!(
        glue.contains(&expect),
        "glue must inline __PYTHS_ABI from abi.rs:\n{glue}"
    );
    assert!(
        glue.contains("WebAssembly.Module.customSections(module, \"pyths.abi\")"),
        "{glue}"
    );
    // the check runs on the COMPILED module BEFORE instantiation (codex m2 blocker: instantiation
    // runs `start`; a wrong-ABI module must be refused before it can execute), and before exports
    assert_compile_check_instantiate_order(&glue);
    // every field is compared independently (a dropped-field mutant is what §G-ABI-3 (v)/(vi) catch)
    assert!(
        glue.contains("for (const f of ['abi', 'list_layout', 'array_layout'])"),
        "{glue}"
    );
    // the const is declared before the loader's top-level await (TDZ)
    let decl = glue.find("const __PYTHS_ABI").unwrap();
    let loader = glue.find("const __wasm = await").unwrap();
    assert!(decl < loader);
}

/// The loader's execution order, pinned as TEXT: the module is COMPILED (never
/// compile+instantiate in one step), `__checkPythsAbi` runs on that module, and
/// `WebAssembly.instantiate(__module, imports)` — the call that runs `start` —
/// comes strictly after the check. The behavioral twin (a wrong-ABI module with an
/// `unreachable` start function, run under Node) is
/// `tests/pythscribe/test_wheel_m2.py::test_abi5_*` — a text pin alone would be
/// vacuous against a loader that re-orders semantics without renaming.
fn assert_compile_check_instantiate_order(glue: &str) {
    let check = glue
        .find("__checkPythsAbi(__module);")
        .expect("loader checks the compiled module");
    let inst = glue
        .find("const __instance = await WebAssembly.instantiate(__module, imports);")
        .expect("loader instantiates the CHECKED module");
    let ret = glue
        .find("return __instance.exports;")
        .expect("loader returns exports");
    assert!(
        check < inst && inst < ret,
        "order must be check < instantiate < return:\n{glue}"
    );
    // no one-step compile+instantiate anywhere: those run `start` before any check could
    for forbidden in [
        "WebAssembly.instantiate(bytes",
        "WebAssembly.instantiate(__wasm_bytes",
        "WebAssembly.instantiateStreaming",
        "new WebAssembly.Instance(",
    ] {
        assert!(
            !glue.contains(forbidden),
            "{forbidden} instantiates before the ABI check:\n{glue}"
        );
    }
    // and the compile step precedes the check
    let compile_at = ["WebAssembly.compile(", "WebAssembly.compileStreaming("]
        .iter()
        .filter_map(|s| glue.find(s))
        .min()
        .expect("loader compiles the module");
    assert!(compile_at < check);
}

#[test]
fn every_bridge_target_compiles_then_checks_then_instantiates() {
    use pyths_codegen_wasm::bridge::BridgeTarget;
    let out = compile(SRC);
    for t in [
        BridgeTarget::Browser,
        BridgeTarget::CloudflareWorkers,
        BridgeTarget::Wasi,
        BridgeTarget::Deno,
    ] {
        let glue = pyths_codegen_wasm::generate_bridge_for_target(t, "./hello.wasm", &out);
        assert!(glue.contains("const __PYTHS_ABI = {"), "{t:?}");
        assert_compile_check_instantiate_order(&glue);
    }
}

#[test]
fn a_module_without_the_section_is_not_current_and_a_second_copy_is_ambiguous() {
    let out = compile(SRC);
    // strip: rebuild the module bytes without the custom section
    let mut stripped = out.wasm[..8].to_vec();
    let mut i = 8;
    while i < out.wasm.len() {
        let id = out.wasm[i];
        let mut j = i + 1;
        let mut size: u32 = 0;
        let mut shift = 0;
        loop {
            let b = out.wasm[j];
            j += 1;
            size |= u32::from(b & 0x7f) << shift;
            shift += 7;
            if b & 0x80 == 0 {
                break;
            }
        }
        let end = j + size as usize;
        if id != 0 {
            stripped.extend_from_slice(&out.wasm[i..end]);
        }
        i = end;
    }
    wasmparser::validate(&stripped).expect("stripped module still validates");
    assert!(abi::read_abi_sections(&stripped).is_empty());
    assert!(!abi::has_current_abi_section(&stripped));
    let mut twice = out.wasm.clone();
    twice.extend_from_slice(&abi::abi_section_bytes());
    assert_eq!(abi::read_abi_sections(&twice).len(), 2);
    assert!(!abi::has_current_abi_section(&twice));
}
