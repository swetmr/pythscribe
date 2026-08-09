use std::collections::{BTreeMap, BTreeSet};

use crate::types::WasmType;
use crate::WasmExportInfo;

/// Runtime target for the generated bridge / glue code.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BridgeTarget {
    /// Browser (default). Uses `WebAssembly.instantiateStreaming(fetch(url))` with `arrayBuffer` fallback.
    Browser,
    /// Cloudflare Workers. WASM bytes are embedded as base64 in the JS module
    /// (Workers cannot fetch sibling files at runtime). Includes a default
    /// `fetch` handler that exposes the compiled functions via JSON URL params.
    CloudflareWorkers,
    /// Node.js with WASI. Uses `node:wasi` to load `wasi_snapshot_preview1` imports.
    Wasi,
    /// Deno. Uses `Deno.readFile()` and top-level await for instantiation.
    Deno,
}

impl BridgeTarget {
    // clippy: public inherent `from_str` is part of the bridge API; renaming would break callers
    #[allow(clippy::should_implement_trait)]
    pub fn from_str(s: &str) -> Option<Self> {
        Some(match s {
            "js+wasm" | "browser" => Self::Browser,
            "wasm-edge" | "cloudflare" | "workers" => Self::CloudflareWorkers,
            "wasi" => Self::Wasi,
            "deno" => Self::Deno,
            _ => return None,
        })
    }
}

/// Map a math.* function name to the JavaScript expression that supplies it
/// to the WASM `math` import namespace.
///
/// All values listed here must accept f64 args and return f64 (matching the
/// signature emit.rs declares for the import).
fn math_import_js(name: &str) -> Option<&'static str> {
    Some(match name {
        "pow" => "Math.pow",
        "sqrt" => "Math.sqrt",
        "sin" => "Math.sin",
        "cos" => "Math.cos",
        "tan" => "Math.tan",
        "asin" => "Math.asin",
        "acos" => "Math.acos",
        "atan" => "Math.atan",
        "atan2" => "Math.atan2",
        "log" => "Math.log",
        "log2" => "Math.log2",
        "log10" => "Math.log10",
        "exp" => "Math.exp",
        "ceil" => "Math.ceil",
        "floor" => "Math.floor",
        "fabs" => "Math.abs",
        _ => return None,
    })
}

/// Render the `imports` JS object that gets passed to `WebAssembly.instantiate`.
fn render_imports_object(math_imports: &BTreeSet<String>, needs_dicts: bool) -> String {
    if math_imports.is_empty() && !needs_dicts {
        return "  const imports = {};\n".to_string();
    }
    let mut out = String::from("  const imports = {");
    let mut sections = Vec::new();
    if !math_imports.is_empty() {
        let mut math_section = String::from(" math: { ");
        let mut first = true;
        for name in math_imports {
            if let Some(js) = math_import_js(name) {
                if !first {
                    math_section.push_str(", ");
                }
                first = false;
                math_section.push_str(&format!("{}: {}", name, js));
            }
        }
        math_section.push_str(" }");
        sections.push(math_section);
    }
    if needs_dicts {
        sections.push(" __dict: __dict_namespace".to_string());
    }
    out.push_str(&sections.join(","));
    out.push_str(" };\n");
    out
}

/// Host-side `__dict` namespace. Emitted ahead of the WASM loader when the
/// module uses dicts. Provides JS-Map-backed handles indexed by i32.
const DICT_HOST: &str = "\
const __dicts = new Map();
let __next_dict_id = 1;
const __dict_namespace = {
  __dict_new: () => {
    const id = __next_dict_id++;
    __dicts.set(id, new Map());
    return id;
  },
  __dict_set_str: (id, kp, v) => {
    const k = __str_from_wasm(kp);
    __dicts.get(id).set(k, Number(v));
  },
  __dict_get_str: (id, kp) => {
    const k = __str_from_wasm(kp);
    const v = __dicts.get(id).get(k);
    return v === undefined ? 0n : BigInt(v);
  },
  __dict_has_str: (id, kp) => {
    const k = __str_from_wasm(kp);
    return __dicts.get(id).has(k) ? 1 : 0;
  },
  __dict_del_str: (id, kp) => {
    const k = __str_from_wasm(kp);
    __dicts.get(id).delete(k);
  },
  __dict_len: (id) => __dicts.get(id).size,
};
";

/// Generate JavaScript bridge/glue code that loads a WASM module and
/// re-exports its functions with proper JSâ†”WASM type conversions.
///
/// Defaults to the Browser target (preserves the pre-Phase-6e behavior).
// clippy: public codegen entry point; the arg list mirrors the bridge feature flags — changing the signature would ripple through all callers
#[allow(clippy::too_many_arguments)]
pub fn generate_bridge(
    wasm_filename: &str,
    exports: &[WasmExportInfo],
    math_imports: &BTreeSet<String>,
    needs_strings: bool,
    needs_errors: bool,
    custom_exceptions: &BTreeMap<String, i32>,
    needs_dicts: bool,
    has_ovf: bool,
    js_twins: Option<&str>,
) -> String {
    generate_bridge_for_target(
        BridgeTarget::Browser,
        wasm_filename,
        &[],
        exports,
        math_imports,
        needs_strings,
        needs_errors,
        custom_exceptions,
        needs_dicts,
        has_ovf,
        js_twins,
    )
}

/// Generate the bridge for a specific runtime target.
///
/// `wasm_bytes` is required when target == CloudflareWorkers (the bytes are
/// embedded as base64). For other targets it can be empty; the WASM is
/// loaded by URL/file at runtime.
// clippy: public codegen entry point; the arg list mirrors the bridge feature flags — changing the signature would ripple through all callers
#[allow(clippy::too_many_arguments)]
pub fn generate_bridge_for_target(
    target: BridgeTarget,
    wasm_filename: &str,
    wasm_bytes: &[u8],
    exports: &[WasmExportInfo],
    math_imports: &BTreeSet<String>,
    needs_strings: bool,
    needs_errors: bool,
    custom_exceptions: &BTreeMap<String, i32>,
    needs_dicts: bool,
    has_ovf: bool,
    js_twins: Option<&str>,
) -> String {
    let has_twins = has_ovf && js_twins.is_some();
    let mut out = String::with_capacity(1024);
    out.push_str("// Auto-generated by PythScribe compiler \u{2014} do not edit.\n");

    // #358: hoist the twin module's runtime imports ahead of everything so
    // the IIFE below (emitted after the loader) can reference the helpers.
    // ESM hoists import declarations regardless of position, but keeping
    // them at the top reads honestly.
    if let Some(twins) = js_twins {
        if has_ovf {
            for line in twins.lines() {
                if line.starts_with("import ") {
                    out.push_str(line);
                    out.push('\n');
                }
            }
        }
    }

    // The dict bridge needs forward references to __wasm (for __str_from_wasm),
    // so emit a host-side __dict namespace and the helper as a single block
    // before the loader. The loader will pass `__dict` through the imports
    // object below.
    if needs_dicts {
        out.push_str(DICT_HOST);
    }

    match target {
        BridgeTarget::Browser => {
            emit_loader_browser(&mut out, wasm_filename, math_imports, needs_dicts)
        }
        BridgeTarget::CloudflareWorkers => {
            emit_loader_workers(&mut out, wasm_bytes, math_imports, needs_dicts)
        }
        BridgeTarget::Wasi => emit_loader_wasi(&mut out, wasm_filename, math_imports, needs_dicts),
        BridgeTarget::Deno => emit_loader_deno(&mut out, wasm_filename, math_imports, needs_dicts),
    }

    // Error-code â†’ JS Error mapping (Tier 7 + Step 5 custom exceptions)
    if needs_errors {
        out.push('\n');
        out.push_str("const __ERROR_NAMES = {\n");
        out.push_str("  1: 'ValueError', 2: 'TypeError', 3: 'IndexError', 4: 'KeyError',\n");
        out.push_str("  5: 'ZeroDivisionError', 6: 'AssertionError', 7: 'RuntimeError',\n");
        // Custom exception classes (codes 100+)
        for (name, code) in custom_exceptions {
            out.push_str(&format!("  {}: {:?},\n", code, name));
        }
        out.push_str("};\n");
        out.push_str("function __check_err() {\n");
        out.push_str("  const code = __wasm.__err_code.value;\n");
        out.push_str("  if (code !== 0) {\n");
        out.push_str("    __wasm.__err_code.value = 0;\n");
        out.push_str("    const name = __ERROR_NAMES[code] || 'Exception';\n");
        // If the WASM module shipped a __err_msg global (only when strings are
        // available), pull the message out of linear memory and surface it on
        // the JS Error. Reset to 0 so it doesn't leak across calls.
        if needs_strings {
            out.push_str("    let msg = name;\n");
            out.push_str("    if (__wasm.__err_msg) {\n");
            out.push_str("      const mp = __wasm.__err_msg.value;\n");
            out.push_str("      if (mp !== 0) {\n");
            out.push_str("        try { msg = `${name}: ${__str_from_wasm(mp)}`; } catch (_) {}\n");
            out.push_str("        __wasm.__err_msg.value = 0;\n");
            out.push_str("      }\n");
            out.push_str("    }\n");
            out.push_str("    const err = new Error(msg);\n");
        } else {
            out.push_str("    const err = new Error(name);\n");
        }
        out.push_str("    err.name = name;\n");
        out.push_str("    throw err;\n");
        out.push_str("  }\n");
        out.push_str("}\n");
    }

    // String marshalling helpers
    if needs_strings {
        out.push('\n');
        out.push_str("const __encoder = new TextEncoder();\n");
        out.push_str("const __decoder = new TextDecoder();\n");
        out.push('\n');
        out.push_str("function __str_to_wasm(s) {\n");
        out.push_str("  const bytes = __encoder.encode(s);\n");
        out.push_str("  const ptr = __wasm.__alloc(bytes.length + 4);\n");
        out.push_str("  const view = new DataView(__wasm.memory.buffer);\n");
        out.push_str("  view.setInt32(ptr, bytes.length, true);\n");
        out.push_str("  new Uint8Array(__wasm.memory.buffer).set(bytes, ptr + 4);\n");
        out.push_str("  return ptr;\n");
        out.push_str("}\n");
        out.push('\n');
        out.push_str("function __str_from_wasm(ptr) {\n");
        out.push_str("  const view = new DataView(__wasm.memory.buffer);\n");
        out.push_str("  const len = view.getInt32(ptr, true);\n");
        out.push_str(
            "  return __decoder.decode(new Uint8Array(__wasm.memory.buffer, ptr + 4, len));\n",
        );
        out.push_str("}\n");
    }

    // List marshalling helpers — emitted whenever an export takes or returns a
    // list, so `convert_js_to_wasm`/`convert_wasm_to_js` can reference them.
    // (Previously referenced-but-undefined → ReferenceError at runtime: B-031.)
    if uses_list_boundary(exports) {
        emit_list_helpers(&mut out);
    }

    // Normalize a WASM i64 result (always a BigInt) back to a Number when
    // it fits the safe-integer range, else keep the BigInt (faithful past
    // 2**53). Mirrors the runtime's __norm.
    out.push_str(
        "const __i64ToJs = (v) => (v >= -9007199254740991n && v <= 9007199254740991n ? Number(v) : v);\n",
    );

    // #358: i64-exactness machinery. `__i64Oob` guards call boundaries —
    // a BigInt argument outside i64 would be silently wrapped by the
    // JS↔WASM conversion, so such calls never reach WASM at all.
    if has_ovf {
        out.push_str(
            "const __i64Oob = (v) => typeof v === \"bigint\" && (v > 9223372036854775807n || v < -9223372036854775808n);\n",
        );
    }
    if has_twins {
        let twins = js_twins.unwrap();
        out.push('\n');
        out.push_str("// #358: exact JS twins of the WASM-compiled functions. When a call\n");
        out.push_str("// would exceed i64 (the `__ovf` flag, or an out-of-range argument),\n");
        out.push_str("// the wrapper transparently re-runs it on these arbitrary-precision\n");
        out.push_str("// implementations — the WASM fast path never returns a wrapped value.\n");
        out.push_str("const __jsfb = (() => {\n");
        for line in twins.lines() {
            if line.starts_with("import ") {
                continue; // hoisted above
            }
            let l = line.strip_prefix("export ").unwrap_or(line);
            out.push_str(l);
            out.push('\n');
        }
        let names: Vec<&str> = exports.iter().map(|e| e.name.as_str()).collect();
        out.push_str(&format!("return {{ {} }};\n}})();\n", names.join(", ")));
    }

    // #364: the fallback-ladder fault predicate. A fallen-back call re-runs on
    // the exact JS twin, so the js+wasm result is identical to never having
    // routed (the #361 transparency property, generalized to ALL observable
    // WASM failures). We fall back on:
    //   - WebAssembly.RuntimeError — any WASM trap (memory access out of
    //     bounds, unreachable, integer overflow, div-by-zero, indirect-call
    //     type mismatch).
    //   - RangeError — a boundary marshalling fault (e.g. NaN cannot be
    //     converted to a BigInt). Pure JS would compute the value instead of
    //     throwing at the boundary, so falling back restores that behavior.
    // Deliberate Python exceptions raised by the compiled function (plain
    // Error with a .name such as ValueError / IndexError, dispatched by
    // __check_err) are NOT faults: they propagate unchanged, exactly as the
    // pure-JS twin would raise them. The happy path pays nothing — the twin
    // only runs on the thrown path.
    if has_twins {
        out.push_str(
            "const __isWasmFault = (e) => (typeof WebAssembly !== \"undefined\" && e instanceof WebAssembly.RuntimeError) || e instanceof RangeError;\n",
        );
    }

    // Export wrapper functions with type conversions
    for export in exports {
        out.push('\n');
        let params_str: Vec<&str> = export.params.iter().map(|(n, _)| n.as_str()).collect();
        out.push_str(&format!(
            "export function {}({}) {{\n",
            export.name,
            params_str.join(", ")
        ));

        // Everything the wrapper does is accumulated into `core` so it can be
        // wrapped, once, in the #364 runtime fallback-ladder try/catch below.
        let mut core = String::new();

        // #358: boundary guard — BigInt args beyond i64 would wrap in the
        // JS→WASM conversion itself; reroute (or reject) before the call.
        let i64_params: Vec<&str> = export
            .params
            .iter()
            .filter(|(_, ty)| matches!(ty, WasmType::I64))
            .map(|(n, _)| n.as_str())
            .collect();
        if has_ovf && !i64_params.is_empty() {
            let cond = i64_params
                .iter()
                .map(|n| format!("__i64Oob({})", n))
                .collect::<Vec<_>>()
                .join(" || ");
            if has_twins {
                core.push_str(&format!(
                    "  if ({}) return __jsfb.{}({});\n",
                    cond,
                    export.name,
                    params_str.join(", ")
                ));
            } else {
                core.push_str(&format!(
                    "  if ({}) throw new RangeError(\"OverflowError: argument exceeds the i64 range of the WASM fast path in {}() \u{2014} compile with --target js for arbitrary precision\");\n",
                    cond, export.name
                ));
            }
        }

        // Build the call arguments with type conversions
        let converted_args: Vec<String> = export
            .params
            .iter()
            .map(|(name, ty)| convert_js_to_wasm(name, ty))
            .collect();

        let call_expr = format!("__wasm.{}({})", export.name, converted_args.join(", "));

        // #358: post-call exactness check, emitted before error dispatch and
        // BEFORE any pointer-return conversion (a flagged call's return value
        // is garbage — never touch it). On fallback, clear both flags so the
        // sticky state can't leak into the next call.
        let ovf_check = |body: &mut String, indent: &str| {
            if !has_ovf {
                return;
            }
            body.push_str(&format!("{}if (__wasm.__ovf.value) {{\n", indent));
            body.push_str(&format!("{}  __wasm.__ovf.value = 0;\n", indent));
            if needs_errors {
                body.push_str(&format!("{}  __wasm.__err_code.value = 0;\n", indent));
            }
            if has_twins {
                body.push_str(&format!(
                    "{}  return __jsfb.{}({});\n",
                    indent,
                    export.name,
                    params_str.join(", ")
                ));
            } else {
                body.push_str(&format!(
                    "{}  throw new Error(\"OverflowError: integer result exceeds the i64 range of the WASM fast path in {}() \u{2014} compile with --target js for arbitrary precision\");\n",
                    indent, export.name
                ));
            }
            body.push_str(&format!("{}}}\n", indent));
        };

        // Build the wrapper body. When the module has a heap (`needs_strings`),
        // wrap the call in an arena scope: save the bump pointer, run the call,
        // and restore it in `finally` so transient argument memory marshalled
        // by `__list_to_wasm`/`__str_to_wasm` is reclaimed every call (B-034 —
        // otherwise repeated list-arg calls exhaust linear memory and crash).
        // Pointer return values are converted (copied out to JS) BEFORE the
        // `finally` runs, so the reset never clobbers a value still in use.
        let indent = if needs_strings { "    " } else { "  " };
        let mut body = String::new();
        match &export.return_type {
            None => {
                body.push_str(&format!("{}{};\n", indent, call_expr));
                ovf_check(&mut body, indent);
                if needs_errors {
                    body.push_str(&format!("{}__check_err();\n", indent));
                }
            }
            Some(ret_ty) => {
                if has_ovf || needs_errors {
                    body.push_str(&format!("{}const __raw = {};\n", indent, call_expr));
                    ovf_check(&mut body, indent);
                    if needs_errors {
                        body.push_str(&format!("{}__check_err();\n", indent));
                    }
                    body.push_str(&format!(
                        "{}return {};\n",
                        indent,
                        convert_wasm_to_js("__raw", ret_ty)
                    ));
                } else {
                    let wrapped = convert_wasm_to_js(&call_expr, ret_ty);
                    body.push_str(&format!("{}return {};\n", indent, wrapped));
                }
            }
        }

        if needs_strings {
            core.push_str("  const __sp = __wasm.__heap_ptr.value;\n");
            core.push_str("  try {\n");
            core.push_str(&body);
            core.push_str("  } finally {\n");
            core.push_str("    __wasm.__heap_ptr.value = __sp;\n");
            core.push_str("  }\n");
        } else {
            core.push_str(&body);
        }

        // #364 runtime fallback ladder: wrap the whole wrapper body so ANY WASM
        // fault (trap or boundary RangeError) transparently re-runs on the exact
        // JS twin. Only emitted when a twin exists (js+wasm target); edge targets
        // have no twin and keep their loud-throw behavior. The happy path pays
        // nothing — the catch is entered only on a thrown fault.
        if has_twins {
            out.push_str("  try {\n");
            out.push_str(&core);
            out.push_str(&format!(
                "  }} catch (__e) {{\n    if (__isWasmFault(__e)) return __jsfb.{}({});\n    throw __e;\n  }}\n",
                export.name,
                params_str.join(", ")
            ));
        } else {
            out.push_str(&core);
        }

        out.push_str("}\n");

        // #364: attach Python keyword-argument metadata to the exported wrapper
        // so call sites using keyword arguments (`f(a=1, b=2)`) bind correctly
        // through the runtime's __pyCallKw. Without this the wrapper falls
        // through to the legacy trailing-options-object convention, so a kwarg
        // call passes an object as the first positional arg — which then
        // marshals to NaN/garbage and crashes or silently miscompiles. This was
        // the dominant real-code miscompile (kwarg calls of admitted functions).
        if !export.params.is_empty() {
            let names = export
                .params
                .iter()
                .map(|(n, _)| format!("{:?}", n))
                .collect::<Vec<_>>()
                .join(", ");
            out.push_str(&format!("{}.__pyparams__ = [{}];\n", export.name, names));
        }
    }

    out
}

/// Universal-environment loader for the `js+wasm` target. Works in:
///   - Browsers (fetch + instantiateStreaming, with arrayBuffer fallback)
///   - Cloudflare Workers (fetch — same path as browsers)
///   - Deno (fetch with file: support)
///   - Node 18+ (fs.readFile via dynamic import — fetch in Node doesn't
///     support file: URLs, so we detect Node and read the bytes directly)
///
/// The Node detection uses `globalThis.process?.versions?.node`, which is
/// safe to evaluate in every environment (returns undefined elsewhere).
/// We branch on Node first because Node *does* have a `fetch` global but
/// it can't load file: URLs, which is what `import.meta.url` resolves to
/// for sibling files.
fn emit_loader_browser(
    out: &mut String,
    wasm_filename: &str,
    math_imports: &BTreeSet<String>,
    needs_dicts: bool,
) {
    out.push_str("const __wasm = await (async () => {\n");
    out.push_str(&render_imports_object(math_imports, needs_dicts));
    out.push_str(&format!(
        "  const url = new URL('{}', import.meta.url);\n",
        wasm_filename
    ));
    // Node path: detect via process.versions.node, dynamic-import node:fs.
    out.push_str("  const isNode = typeof globalThis.process !== 'undefined'\n");
    out.push_str("    && globalThis.process.versions != null\n");
    out.push_str("    && typeof globalThis.process.versions.node === 'string';\n");
    out.push_str("  if (isNode) {\n");
    out.push_str("    const { readFile } = await import('node:fs/promises');\n");
    out.push_str("    const { fileURLToPath } = await import('node:url');\n");
    out.push_str("    const bytes = await readFile(fileURLToPath(url));\n");
    out.push_str("    return (await WebAssembly.instantiate(bytes, imports)).instance.exports;\n");
    out.push_str("  }\n");
    // Browser/Workers/Deno: fetch with streaming when available.
    out.push_str("  if (typeof WebAssembly.instantiateStreaming === 'function') {\n");
    out.push_str(
        "    return (await WebAssembly.instantiateStreaming(fetch(url), imports)).instance.exports;\n",
    );
    out.push_str("  }\n");
    out.push_str("  const bytes = await fetch(url).then(r => r.arrayBuffer());\n");
    out.push_str("  return (await WebAssembly.instantiate(bytes, imports)).instance.exports;\n");
    out.push_str("})();\n");
}

/// Cloudflare Workers loader: WASM bytes embedded as base64. Workers cannot
/// fetch sibling files at module scope, so we inline the binary.
fn emit_loader_workers(
    out: &mut String,
    wasm_bytes: &[u8],
    math_imports: &BTreeSet<String>,
    needs_dicts: bool,
) {
    let b64 = base64_encode(wasm_bytes);
    out.push_str(&format!("const __WASM_BYTES_B64 = \"{}\";\n", b64));
    out.push_str("const __wasm_bytes = (() => {\n");
    out.push_str("  const bin = atob(__WASM_BYTES_B64);\n");
    out.push_str("  const arr = new Uint8Array(bin.length);\n");
    out.push_str("  for (let i = 0; i < bin.length; i++) arr[i] = bin.charCodeAt(i);\n");
    out.push_str("  return arr;\n");
    out.push_str("})();\n");
    out.push_str("const __wasm = await (async () => {\n");
    out.push_str(&render_imports_object(math_imports, needs_dicts));
    out.push_str(
        "  return (await WebAssembly.instantiate(__wasm_bytes, imports)).instance.exports;\n",
    );
    out.push_str("})();\n");
}

/// WASI loader for Node.js. Uses `node:wasi` and `node:fs` to load and
/// instantiate the module with `wasi_snapshot_preview1` imports.
fn emit_loader_wasi(
    out: &mut String,
    wasm_filename: &str,
    math_imports: &BTreeSet<String>,
    needs_dicts: bool,
) {
    out.push_str("import { WASI } from 'node:wasi';\n");
    out.push_str("import { readFile } from 'node:fs/promises';\n");
    out.push_str("import { fileURLToPath } from 'node:url';\n");
    out.push('\n');
    out.push_str("const __wasm = await (async () => {\n");
    out.push_str("  const wasi = new WASI({\n");
    out.push_str("    version: 'preview1',\n");
    out.push_str("    args: process.argv,\n");
    out.push_str("    env: process.env,\n");
    out.push_str("  });\n");
    out.push_str(&render_imports_object(math_imports, needs_dicts));
    out.push_str("  imports.wasi_snapshot_preview1 = wasi.wasiImport;\n");
    out.push_str(&format!(
        "  const url = new URL('{}', import.meta.url);\n",
        wasm_filename
    ));
    out.push_str("  const bytes = await readFile(fileURLToPath(url));\n");
    out.push_str("  const instance = (await WebAssembly.instantiate(bytes, imports)).instance;\n");
    out.push_str("  return instance.exports;\n");
    out.push_str("})();\n");
}

/// Deno loader: Deno.readFile + WebAssembly.instantiate.
fn emit_loader_deno(
    out: &mut String,
    wasm_filename: &str,
    math_imports: &BTreeSet<String>,
    needs_dicts: bool,
) {
    out.push_str("const __wasm = await (async () => {\n");
    out.push_str(&render_imports_object(math_imports, needs_dicts));
    out.push_str(&format!(
        "  const url = new URL('{}', import.meta.url);\n",
        wasm_filename
    ));
    out.push_str("  const bytes = await Deno.readFile(url);\n");
    out.push_str("  return (await WebAssembly.instantiate(bytes, imports)).instance.exports;\n");
    out.push_str("})();\n");
}

/// Append a CF Workers `fetch` handler that exposes the compiled exports
/// over HTTP. Each export is reachable at `GET /<name>?<param>=<value>&...`
/// returning the result as JSON: `{ result: <value> }`.
pub fn append_workers_fetch_handler(out: &mut String, exports: &[WasmExportInfo]) {
    out.push('\n');
    out.push_str("export default {\n");
    out.push_str("  async fetch(request) {\n");
    out.push_str("    try {\n");
    out.push_str("      const url = new URL(request.url);\n");
    out.push_str("      const path = url.pathname.replace(/^\\//, '');\n");
    out.push_str("      const params = url.searchParams;\n");
    out.push_str("      const exports = {\n");
    for (i, e) in exports.iter().enumerate() {
        let names: Vec<String> = e
            .params
            .iter()
            .map(|(n, ty)| {
                format!(
                    "{}(params.get('{}'))",
                    match ty {
                        WasmType::I64 | WasmType::I32 => "Number",
                        WasmType::F64 => "Number",
                        WasmType::Ptr => "String",
                        // Collections via fetch handler are passed through as JSON.parse
                        _ => "JSON.parse",
                    },
                    n
                )
            })
            .collect();
        let comma = if i + 1 < exports.len() { "," } else { "" };
        out.push_str(&format!(
            "        {}: () => {}({}){}\n",
            e.name,
            e.name,
            names.join(", "),
            comma
        ));
    }
    out.push_str("      };\n");
    out.push_str("      const fn = exports[path];\n");
    out.push_str("      if (!fn) {\n");
    out.push_str("        return new Response(JSON.stringify({ available: Object.keys(exports) }), { status: 404, headers: { 'content-type': 'application/json' } });\n");
    out.push_str("      }\n");
    out.push_str("      const result = fn();\n");
    out.push_str("      return new Response(JSON.stringify({ result }), { headers: { 'content-type': 'application/json' } });\n");
    out.push_str("    } catch (e) {\n");
    out.push_str("      return new Response(JSON.stringify({ error: e.message, name: e.name }), { status: 400, headers: { 'content-type': 'application/json' } });\n");
    out.push_str("    }\n");
    out.push_str("  }\n");
    out.push_str("};\n");
}

/// Standard base64 encoder. Avoid pulling in the `base64` crate for one helper.
fn base64_encode(input: &[u8]) -> String {
    const CHARS: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(input.len().div_ceil(3) * 4);
    let mut i = 0;
    while i + 3 <= input.len() {
        let n = ((input[i] as u32) << 16) | ((input[i + 1] as u32) << 8) | (input[i + 2] as u32);
        out.push(CHARS[((n >> 18) & 0x3F) as usize] as char);
        out.push(CHARS[((n >> 12) & 0x3F) as usize] as char);
        out.push(CHARS[((n >> 6) & 0x3F) as usize] as char);
        out.push(CHARS[(n & 0x3F) as usize] as char);
        i += 3;
    }
    let rem = input.len() - i;
    if rem == 1 {
        let n = (input[i] as u32) << 16;
        out.push(CHARS[((n >> 18) & 0x3F) as usize] as char);
        out.push(CHARS[((n >> 12) & 0x3F) as usize] as char);
        out.push('=');
        out.push('=');
    } else if rem == 2 {
        let n = ((input[i] as u32) << 16) | ((input[i + 1] as u32) << 8);
        out.push(CHARS[((n >> 18) & 0x3F) as usize] as char);
        out.push(CHARS[((n >> 12) & 0x3F) as usize] as char);
        out.push(CHARS[((n >> 6) & 0x3F) as usize] as char);
        out.push('=');
    }
    out
}

/// The JS-side element-kind tag for a list's element type. Drives the
/// `DataView` accessor (`f64`/`i64`/`i32`) and element width in the
/// `__list_to_wasm` / `__list_from_wasm` marshallers. Nested collections and
/// strings are stored as i32 pointers into linear memory.
fn list_elem_kind(inner: &WasmType) -> &'static str {
    match inner {
        WasmType::F64 => "f64",
        WasmType::I64 => "i64",
        // bool / nested-ptr / str-ptr all live in an i32 slot.
        _ => "i32",
    }
}

/// Whether any export's params or return type reference a list (so the glue
/// must define the list marshalling helpers).
fn uses_list_boundary(exports: &[WasmExportInfo]) -> bool {
    let is_list = |ty: &WasmType| matches!(ty, WasmType::PtrList(_));
    exports.iter().any(|e| {
        e.params.iter().any(|(_, ty)| is_list(ty)) || e.return_type.as_ref().is_some_and(is_list)
    })
}

/// Emit the `__list_to_wasm` / `__list_from_wasm` marshallers. Mirrors the
/// in-WASM list layout produced by `emit_list_literal`:
///   `[i32 length][i32 capacity][elements...]` (header 8 bytes, elements at
///   their natural width: f64/i64 → 8 bytes, i32 → 4 bytes).
fn emit_list_helpers(out: &mut String) {
    out.push('\n');
    // __list_to_wasm allocates via the module's bump allocator (__alloc). The
    // memory is transient: each exported wrapper saves/restores __heap_ptr
    // around the call (arena scope), so this argument memory is reclaimed when
    // the call returns — repeated calls do not accumulate (B-034). The fresh
    // `DataView(__wasm.memory.buffer)` below is created AFTER __alloc (which may
    // grow memory and detach the old buffer), so it always covers `ptr`.
    out.push_str("function __list_to_wasm(arr, kind) {\n");
    out.push_str("  const n = arr.length;\n");
    out.push_str("  const esize = kind === 'i32' ? 4 : 8;\n");
    out.push_str("  const ptr = __wasm.__alloc(8 + n * esize);\n");
    out.push_str("  const view = new DataView(__wasm.memory.buffer);\n");
    out.push_str("  view.setInt32(ptr, n, true);\n");
    out.push_str("  view.setInt32(ptr + 4, n, true);\n");
    out.push_str("  for (let i = 0; i < n; i++) {\n");
    out.push_str("    const off = ptr + 8 + i * esize;\n");
    out.push_str("    if (kind === 'f64') view.setFloat64(off, arr[i], true);\n");
    out.push_str("    else if (kind === 'i64') view.setBigInt64(off, typeof arr[i] === 'bigint' ? arr[i] : BigInt(Math.trunc(arr[i])), true);\n");
    out.push_str("    else view.setInt32(off, arr[i] | 0, true);\n");
    out.push_str("  }\n");
    out.push_str("  return ptr;\n");
    out.push_str("}\n");
    out.push('\n');
    out.push_str("function __list_from_wasm(ptr, kind) {\n");
    out.push_str("  const view = new DataView(__wasm.memory.buffer);\n");
    out.push_str("  const n = view.getInt32(ptr, true);\n");
    out.push_str("  const esize = kind === 'i32' ? 4 : 8;\n");
    out.push_str("  const out = new Array(n);\n");
    out.push_str("  for (let i = 0; i < n; i++) {\n");
    out.push_str("    const off = ptr + 8 + i * esize;\n");
    out.push_str("    if (kind === 'f64') out[i] = view.getFloat64(off, true);\n");
    out.push_str("    else if (kind === 'i64') { const v = view.getBigInt64(off, true); out[i] = (v >= -9007199254740991n && v <= 9007199254740991n) ? Number(v) : v; }\n");
    out.push_str("    else out[i] = view.getInt32(off, true);\n");
    out.push_str("  }\n");
    out.push_str("  return out;\n");
    out.push_str("}\n");
}

/// Convert a JS argument to the WASM parameter type.
fn convert_js_to_wasm(name: &str, ty: &WasmType) -> String {
    match ty {
        // i64 params take a JS BigInt. An int arg may already be a BigInt
        // (the arbitrary-precision representation for values past 2**53) or
        // a Number (small int) — accept both.
        WasmType::I64 => format!(
            "(typeof {n} === \"bigint\" ? {n} : BigInt(Math.trunc({n})))",
            n = name
        ),
        WasmType::F64 => name.to_string(),
        WasmType::I32 => format!("{} ? 1 : 0", name),
        WasmType::Ptr => format!("__str_to_wasm({})", name),
        WasmType::PtrList(inner) => {
            format!("__list_to_wasm({}, {:?})", name, list_elem_kind(inner))
        }
        WasmType::PtrDict(_, _) => format!("__dict_to_wasm({})", name),
        WasmType::PtrTuple(_) => format!("__tuple_to_wasm({})", name),
        WasmType::PtrClosure { .. } => format!("__closure_to_wasm({})", name),
    }
}

/// Convert a WASM return value to the appropriate JS type.
fn convert_wasm_to_js(expr: &str, ty: &WasmType) -> String {
    match ty {
        // A WASM i64 result arrives as a JS BigInt. Normalize to a Number
        // when it fits the safe-integer range (keeps the fast native
        // representation) but PRESERVE the BigInt past 2**53 — downcasting
        // with bare Number() there would silently lose precision and defeat
        // the arbitrary-precision guarantee for WASM-routed functions.
        WasmType::I64 => format!("__i64ToJs({})", expr),
        WasmType::F64 => expr.to_string(),
        WasmType::I32 => format!("Boolean({})", expr),
        WasmType::Ptr => format!("__str_from_wasm({})", expr),
        WasmType::PtrList(inner) => {
            format!("__list_from_wasm({}, {:?})", expr, list_elem_kind(inner))
        }
        WasmType::PtrDict(_, _) => format!("__dict_from_wasm({})", expr),
        WasmType::PtrTuple(_) => format!("__tuple_from_wasm({})", expr),
        WasmType::PtrClosure { .. } => format!("__closure_from_wasm({})", expr),
    }
}
