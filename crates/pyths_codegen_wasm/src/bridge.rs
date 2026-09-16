use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write as _;

use crate::abi;
use crate::types::WasmType;
use crate::WasmExportInfo;

/// Runtime target for the generated bridge / glue code.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BridgeTarget {
    /// Browser (default). Uses `WebAssembly.compileStreaming(fetch(url))` with `arrayBuffer` fallback.
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

// ── SECURITY: source-derived text serializers (Codex-Security scan 2026-08-12) ──
//
// The bridge interpolates two classes of source/output-derived text into the
// emitted JS: the artifact filename (findings #4 — into a `new URL('...')`
// string) and WASM function parameter names (finding #13 — into JS binding
// positions). Route both through these encoders so a hostile filename or a
// reserved-word parameter cannot break out of its syntactic context. Mirrors
// the codegen_js `escape_js_string` / `sanitize_ident` "one safe API" — the
// crate boundary forces a local copy; keep the two in sync.

/// Escape a string for inclusion in a JS **single-quoted** literal (the form
/// the WASM loaders use for `new URL('<file>', import.meta.url)`). Every
/// character that could terminate the literal, inject a line terminator, or
/// smuggle a control code is escaped, so the argument stays exactly one string.
fn escape_js_single_quoted(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for ch in s.chars() {
        match ch {
            '\\' => out.push_str("\\\\"),
            '\'' => out.push_str("\\'"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            '\u{2028}' => out.push_str("\\u2028"),
            '\u{2029}' => out.push_str("\\u2029"),
            c if (c as u32) < 0x20 || c as u32 == 0x7f => {
                out.push_str(&format!("\\x{:02x}", c as u32));
            }
            c => out.push(c),
        }
    }
    out
}

/// Wrap a filename in a single-quoted JS string literal with escapes applied.
fn js_single_quoted(s: &str) -> String {
    format!("'{}'", escape_js_single_quoted(s))
}

/// JS reserved / contextual / strict-mode-reserved words that are ALSO legal
/// Python/WASM parameter identifiers, so a `def f(default, new): ...` compiled
/// to WASM would emit `export function f(default, new)` — a SyntaxError.
/// Mirrors codegen_js `is_js_reserved_word`.
fn is_js_reserved_word(s: &str) -> bool {
    matches!(
        s,
        "let"
            | "const"
            | "var"
            | "new"
            | "function"
            | "this"
            | "typeof"
            | "delete"
            | "void"
            | "switch"
            | "case"
            | "default"
            | "catch"
            | "do"
            | "enum"
            | "export"
            | "extends"
            | "instanceof"
            | "throw"
            | "static"
            | "debugger"
            | "null"
            | "true"
            | "false"
            | "undefined"
            | "NaN"
            | "Infinity"
            | "arguments"
            | "eval"
            | "await"
            | "yield"
            | "super"
            | "implements"
            | "interface"
            | "package"
            | "private"
            | "protected"
            | "public"
            | "finally"
            | "in"
            | "of"
            | "import"
    )
}

/// Sanitize a WASM parameter name for a JS **binding** position. A reserved
/// word gets a deterministic `$` suffix; because the mapping is a pure function
/// of the name, the declaration and every reference in the wrapper rename
/// identically. The Python names are preserved verbatim in `__pyparams__`
/// (used by the runtime for keyword-argument binding), so the external contract
/// is unchanged — only the local JS binding is renamed.
fn sanitize_js_param(name: &str) -> String {
    if is_js_reserved_word(name) {
        format!("{}$", name)
    } else {
        name.to_string()
    }
}

/// M2.1 (spec 13-09-26 §5.6): the WASM ABI contract inlined into the glue from the
/// ONE source (`abi.rs`) — `__PYTHS_ABI` — plus `__checkPythsAbi(module)`, which
/// every loader runs on the compiled `WebAssembly.Module` BEFORE instantiating it
/// (compile → check → instantiate; instantiation is what runs a `start` function,
/// so a refused module never executes — codex m2 blocker). It reads the module's
/// own `pyths.abi` custom section and compares
/// major AND both layout strings FIELD BY FIELD (a mutant that drops one field's
/// comparison is caught by the per-field patched-module controls, validation
/// §G-ABI-3 (v)/(vi)). A module whose section is absent, duplicated, malformed
/// or disagreeing is refused with a loud `Error` naming the field and both
/// sides — never instantiated, never silently misread at the wrong layout.
fn emit_abi_check(out: &mut String) {
    out.push_str(&format!(
        "const __PYTHS_ABI = {{ abi: {}, list_layout: {:?}, array_layout: {:?} }};\n",
        abi::PYTHS_ABI_MAJOR,
        abi::LIST_LAYOUT_VERSION,
        abi::ARRAY_LAYOUT_VERSION
    ));
    out.push_str("function __checkPythsAbi(module) {\n");
    out.push_str(&format!(
        "  const secs = WebAssembly.Module.customSections(module, {:?});\n",
        abi::ABI_SECTION_NAME
    ));
    out.push_str("  if (secs.length !== 1) throw new Error('pythscribe: WASM ABI check failed: expected exactly one `pyths.abi` custom section, found ' + secs.length + ' (this .wasm was not emitted by the pyths compiler that emitted this glue; rebuild the artifact)');\n");
    out.push_str("  let got;\n");
    out.push_str("  try { got = JSON.parse(new TextDecoder().decode(new Uint8Array(secs[0]))); } catch (e) { throw new Error('pythscribe: WASM ABI check failed: `pyths.abi` section is not valid JSON (' + e.message + ')'); }\n");
    out.push_str("  for (const f of ['abi', 'list_layout', 'array_layout']) {\n");
    out.push_str("    if (got[f] !== __PYTHS_ABI[f]) throw new Error('pythscribe: WASM ABI mismatch on `' + f + '`: module=' + JSON.stringify(got[f]) + ' glue=' + JSON.stringify(__PYTHS_ABI[f]) + ' -- rebuild the artifact with the matching pyths compiler (a mismatched layout would be read at the wrong width; refused, never silently)');\n");
    out.push_str("  }\n");
    out.push_str("}\n");
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

    // M2.1: the ABI contract + loud check, declared BEFORE the loader's
    // top-level await runs (a `const` in the TDZ would throw).
    emit_abi_check(&mut out);

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

    // M2a-3b: typed-array marshalling helpers — emitted whenever an export takes
    // an `Array[dtype, ndim]` param, so `__array_to_wasm` / `__array_write_back`
    // are defined. Byte-agrees with `pythscribe/runtime/array_buffer.py`.
    if uses_array_boundary(exports) {
        emit_array_helpers(&mut out);
    }

    // Normalize a WASM i64 result (always a BigInt) back to a Number when
    // it fits the safe-integer range, else keep the BigInt (faithful past
    // 2**53). Mirrors the runtime's __norm.
    out.push_str(
        "const __i64ToJs = (v) => (v >= -9007199254740991n && v <= 9007199254740991n ? Number(v) : v);\n",
    );

    // Option B (minimal): an f64 WASM result is a Python float — box it,
    // IFF integer-valued, with a glue-local brand class (duck-typed
    // __pyfloat__, recognized by the runtime's repr/arith/eq paths) so a
    // whole-valued result (12.0) keeps its float identity; a non-integer
    // result (3.14) is already unambiguous and stays a native Number.
    // Extends Number, so native JS numeric APIs coerce via [[NumberData]].
    if exports
        .iter()
        .any(|e| matches!(e.return_type, Some(WasmType::F64)))
    {
        out.push_str(
            "class __PyFloatW extends Number { get __pyfloat__() { return true; } }\n\
             const __f64Box = (v) => (Number.isInteger(v) ? new __PyFloatW(v) : v);\n",
        );
    }

    // #465: f64 ARG boundary — glue-local mirror of the runtime value-boundary
    // authority __reqNum (runtime/src/operators.js) on the BigInt branch. A
    // hybrid large int (BigInt past 2**53) entering a float parameter converts
    // with the standard int→float rule; a BigInt beyond IEEE-754 double range
    // raises OverflowError ("int too large to convert to float", CPython
    // float(10**400)) instead of silently crossing as Infinity. This is NOT a
    // marshalling FAULT: the pure-JS twin raises the SAME OverflowError via
    // __reqNum, so the #364 ladder correctly propagates it (plain Error, not
    // RangeError), identically with and without twins.
    if exports
        .iter()
        .any(|e| e.params.iter().any(|(_, ty)| matches!(ty, WasmType::F64)))
    {
        out.push_str(
            "const __f64Arg = (v) => { if (typeof v !== \"bigint\") return v; const n = Number(v); if (Number.isFinite(n)) return n; const e = new Error(\"int too large to convert to float\"); e.name = \"OverflowError\"; throw e; };\n",
        );
    }

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
        // #439: the spliced twins were emitted by codegen_js, which sanitizes a
        // reserved-word function name (`default` → `default$`). The `__jsfb`
        // object shorthand must key on that SAME sanitized name, or `return {
        // default }` references an undeclared `default` (the twin is `function
        // default$`) — a ReferenceError that breaks every fallback.
        let names: Vec<String> = exports.iter().map(|e| sanitize_js_param(&e.name)).collect();
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
        // SECURITY (#13): a WASM param may be a legal Python identifier that is
        // a JS reserved word (`default`, `new`, `in`, ...). Emitting it raw in a
        // binding position produces `export function f(default, new)` — invalid
        // JS. Sanitize once here and use the sanitized name at EVERY JS binding
        // and reference site below; the original Python names are preserved in
        // `__pyparams__` (the keyword-binding contract) verbatim.
        let js_names: Vec<String> = export
            .params
            .iter()
            .map(|(n, _)| sanitize_js_param(n))
            .collect();
        let params_str: Vec<&str> = js_names.iter().map(|s| s.as_str()).collect();
        // #439: the exported wrapper's own NAME can also be a JS reserved word
        // (`def default(...)` → `export function default` is a SyntaxError).
        // Sanitize it at every JS binding/reference site — the wrapper decl, the
        // `__jsfb.<fn>` twin dispatch, and the `<fn>.__pyparams__` attach — so it
        // coordinates with codegen_js (which imports it under the same `default$`)
        // and with the `__jsfb` object built above. The WASM binary export is a
        // STRING keyed by the raw Python name, so `__wasm.<raw>` (a legal
        // property access, reserved words allowed after `.`) stays unsanitized.
        let js_export = sanitize_js_param(&export.name);
        out.push_str(&format!(
            "export function {}({}) {{\n",
            js_export,
            params_str.join(", ")
        ));

        // Everything the wrapper does is accumulated into `core` so it can be
        // wrapped, once, in the #364 runtime fallback-ladder try/catch below.
        let mut core = String::new();

        // #358: boundary guard — BigInt args beyond i64 would wrap in the
        // JS→WASM conversion itself; reroute (or reject) before the call.
        // SECURITY (#13): reference the sanitized JS binding names, not the raw
        // Python names, so a reserved-word i64 param resolves to its `$`-suffixed
        // local rather than an undefined/invalid identifier.
        let i64_params: Vec<&str> = export
            .params
            .iter()
            .zip(js_names.iter())
            .filter(|((_, ty), _)| matches!(ty, WasmType::I64))
            .map(|(_, js)| js.as_str())
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
                    js_export,
                    params_str.join(", ")
                ));
            } else {
                core.push_str(&format!(
                    "  if ({}) throw new RangeError(\"OverflowError: argument exceeds the i64 range of the WASM fast path in {}() \u{2014} compile with --target js for arbitrary precision\");\n",
                    cond, export.name
                ));
            }
        }

        // #484: SYMMETRIC marshalling for mutable `list` out-parameters. A list
        // arg is marshalled into linear memory through a NAMED local
        // (`__wb_arg_<i>`) rather than inline, so the (possibly in-place-mutated)
        // buffer can be copied BACK into the caller's JS array on return — the
        // inverse direction the old glue silently dropped. `writeback` carries
        // (param index, sanitized JS name, element kind) for every list param;
        // scalar/str/dict/tuple/closure params keep their inline conversion, so
        // a function WITHOUT list params emits byte-identical glue to before.
        //
        // OUT OF SCOPE (orthogonal, pre-existing): ALIASED list params — the
        // SAME JS array passed to two parameters, `f(xs, xs)`. The glue marshals
        // each list param into an INDEPENDENT buffer (it has no alias dedup), so
        // two mutated buffers write back to the shared array last-writer-wins,
        // not composed as CPython's single-object aliasing would. Write-back does
        // not introduce this — the buffers were already independent on the IN
        // direction — and closing it is an INPUT-marshalling change (dedup equal
        // array params to one buffer), tracked separately from #484.
        let writeback: Vec<(usize, String, String)> = export
            .params
            .iter()
            .zip(js_names.iter())
            .enumerate()
            .filter_map(|(i, ((_, ty), js))| match ty {
                WasmType::PtrList(inner) => {
                    Some((i, js.clone(), list_elem_kind(inner).to_string()))
                }
                _ => None,
            })
            .collect();
        // M2a-3b: SYMMETRIC marshalling for `Array[dtype, ndim]` out-parameters —
        // the typed-array counterpart of the #484 list write-back. A caller-
        // allocated `TypedArray` is marshalled IN through `__array_to_wasm` (its
        // pointer bound to a `__wb_arg_<i>` local) and the (in-place-filled)
        // buffer copied BACK into the SAME TypedArray on return — the only way a
        // scalar-return @wasm kernel produces array output (#364). Carries
        // (param index, sanitized JS name, dtype spelling, ndim).
        //
        // OUT OF SCOPE (same known limitation as the #484 list case): ALIASED
        // array params — the SAME TypedArray passed to two parameters, `f(x, x)`.
        // Each array param is marshalled into an INDEPENDENT WASM buffer (no alias
        // dedup), so a cross-element kernel (`out[i] = a[i-1]`) computes from the
        // pristine input buffer and last-writer-wins on the shared array, NOT the
        // in-place propagation CPython's aliasing would show. This matches the
        // SERVER path (`array_buffer.py` also uses independent buffers), so it is
        // NOT a js+wasm regression; it is the documented `caller-allocated,
        // distinct out-buffer` precondition (closing it is an input-marshalling
        // alias-dedup change, tracked separately — the #484 aliasing note).
        let array_writeback: Vec<(usize, String, &'static str, u32)> = export
            .params
            .iter()
            .zip(js_names.iter())
            .enumerate()
            .filter_map(|(i, ((_, ty), js))| match ty {
                WasmType::PtrArray { dtype, ndim } => {
                    Some((i, js.clone(), dtype.spelling(), *ndim))
                }
                _ => None,
            })
            .collect();
        let has_writeback = !writeback.is_empty() || !array_writeback.is_empty();

        // Build the call arguments with type conversions. SECURITY (#13): pass
        // the sanitized JS binding name into the conversion so every reference
        // matches the (possibly `$`-suffixed) parameter declaration. A list
        // param references its hoisted `__wb_arg_<i>` local (declared in the
        // prelude below) so the same pointer is available for write-back.
        let converted_args: Vec<String> = export
            .params
            .iter()
            .zip(js_names.iter())
            .enumerate()
            .map(|(i, ((_, ty), js))| match ty {
                // A list OR array param references its hoisted `__wb_arg_<i>`
                // local so the same pointer is available for write-back.
                WasmType::PtrList(_) | WasmType::PtrArray { .. } => format!("__wb_arg_{}", i),
                _ => convert_js_to_wasm(js, ty),
            })
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
                    js_export,
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
        // #484: prelude marshals each list param into its `__wb_arg_<i>` local
        // (inside the arena scope — allocs are reclaimed by the `finally`); the
        // write-back copies each buffer back into its JS array after the call,
        // AFTER `ovf_check` (an i64-overflowed buffer holds wrapped garbage — the
        // twin re-run redoes the mutation faithfully, so ovf returns/throws
        // BEFORE write-back) but BEFORE `__check_err` (a deliberate Python
        // exception raised MID-kernel leaves the buffer holding the faithful
        // PARTIAL mutation — CPython aliasing makes that partial write visible to
        // the caller alongside the raise, so write-back must run before the
        // exception propagates). A WASM trap throws during the call itself, so
        // neither runs and the twin re-executes from the original array. Both
        // strings are empty for a function without list params (inert glue).
        let mut prelude = String::new();
        for (i, js, kind) in &writeback {
            prelude.push_str(&format!(
                "{}const __wb_arg_{} = __list_to_wasm({}, {:?});\n",
                indent, i, js, kind
            ));
        }
        // M2a-3b: array params marshal in alongside list params (same arena
        // scope, same `__wb_arg_<i>` binding convention).
        for (i, js, dtype, ndim) in &array_writeback {
            prelude.push_str(&format!(
                "{}const __wb_arg_{} = __array_to_wasm({}, {:?}, {});\n",
                indent, i, js, dtype, ndim
            ));
        }
        let mut wb = String::new();
        for (i, js, kind) in &writeback {
            wb.push_str(&format!(
                "{}__list_write_back({}, __wb_arg_{}, {:?});\n",
                indent, js, i, kind
            ));
        }
        for (i, js, dtype, ndim) in &array_writeback {
            wb.push_str(&format!(
                "{}__array_write_back({}, __wb_arg_{}, {:?}, {});\n",
                indent, js, i, dtype, ndim
            ));
        }
        let mut body = String::new();
        body.push_str(&prelude);
        match &export.return_type {
            None => {
                body.push_str(&format!("{}{};\n", indent, call_expr));
                ovf_check(&mut body, indent);
                body.push_str(&wb);
                if needs_errors {
                    body.push_str(&format!("{}__check_err();\n", indent));
                }
            }
            Some(ret_ty) => {
                if has_ovf || needs_errors || has_writeback {
                    body.push_str(&format!("{}const __raw = {};\n", indent, call_expr));
                    ovf_check(&mut body, indent);
                    body.push_str(&wb);
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
                js_export,
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
            out.push_str(&format!("{}.__pyparams__ = [{}];\n", js_export, names));
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
    // SECURITY (#4): `wasm_filename` is output-stem-derived. A `'` or newline
    // in it would break out of the single-quoted URL argument and inject JS at
    // module scope. Serialize it as a JS single-quoted string literal.
    out.push_str(&format!(
        "  const url = new URL({}, import.meta.url);\n",
        js_single_quoted(wasm_filename)
    ));
    // Node path: detect via process.versions.node, dynamic-import node:fs.
    out.push_str("  const isNode = typeof globalThis.process !== 'undefined'\n");
    out.push_str("    && globalThis.process.versions != null\n");
    out.push_str("    && typeof globalThis.process.versions.node === 'string';\n");
    // M2.1 (codex m2 blocker): COMPILE -> CHECK -> INSTANTIATE. Every branch only
    // COMPILES (`WebAssembly.compile[Streaming]` never runs the start function);
    // the ABI check runs on that `WebAssembly.Module`, and only a module that
    // passed is instantiated (instantiation is what runs `start`). Mirrors the
    // tab shim (`list_buffer.mjs::instantiate`) ordering exactly.
    out.push_str("  let __module;\n");
    out.push_str("  if (isNode) {\n");
    out.push_str("    const { readFile } = await import('node:fs/promises');\n");
    out.push_str("    const { fileURLToPath } = await import('node:url');\n");
    out.push_str("    const bytes = await readFile(fileURLToPath(url));\n");
    out.push_str("    __module = await WebAssembly.compile(bytes);\n");
    // Browser/Workers/Deno: fetch with streaming compilation when available.
    out.push_str("  } else if (typeof WebAssembly.compileStreaming === 'function') {\n");
    out.push_str("    __module = await WebAssembly.compileStreaming(fetch(url));\n");
    out.push_str("  } else {\n");
    out.push_str("    const bytes = await fetch(url).then(r => r.arrayBuffer());\n");
    out.push_str("    __module = await WebAssembly.compile(bytes);\n");
    out.push_str("  }\n");
    emit_check_then_instantiate(out);
}

/// The shared tail of every loader (M2.1, codex m2 blocker): the ABI check on the
/// COMPILED module, then — and only then — instantiation. A wrong-ABI module whose
/// `start` function traps or never returns is refused BEFORE it can run.
fn emit_check_then_instantiate(out: &mut String) {
    out.push_str("  __checkPythsAbi(__module);\n");
    out.push_str("  const __instance = await WebAssembly.instantiate(__module, imports);\n");
    out.push_str("  return __instance.exports;\n");
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
    out.push_str("  const __module = await WebAssembly.compile(__wasm_bytes);\n");
    emit_check_then_instantiate(out);
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
    // SECURITY (#4): `wasm_filename` is output-stem-derived. A `'` or newline
    // in it would break out of the single-quoted URL argument and inject JS at
    // module scope. Serialize it as a JS single-quoted string literal.
    out.push_str(&format!(
        "  const url = new URL({}, import.meta.url);\n",
        js_single_quoted(wasm_filename)
    ));
    out.push_str("  const bytes = await readFile(fileURLToPath(url));\n");
    out.push_str("  const __module = await WebAssembly.compile(bytes);\n");
    emit_check_then_instantiate(out);
}

/// Deno loader: Deno.readFile + WebAssembly.compile → ABI check → instantiate.
fn emit_loader_deno(
    out: &mut String,
    wasm_filename: &str,
    math_imports: &BTreeSet<String>,
    needs_dicts: bool,
) {
    out.push_str("const __wasm = await (async () => {\n");
    out.push_str(&render_imports_object(math_imports, needs_dicts));
    // SECURITY (#4): `wasm_filename` is output-stem-derived. A `'` or newline
    // in it would break out of the single-quoted URL argument and inject JS at
    // module scope. Serialize it as a JS single-quoted string literal.
    out.push_str(&format!(
        "  const url = new URL({}, import.meta.url);\n",
        js_single_quoted(wasm_filename)
    ));
    out.push_str("  const bytes = await Deno.readFile(url);\n");
    out.push_str("  const __module = await WebAssembly.compile(bytes);\n");
    emit_check_then_instantiate(out);
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
        // #439: the route KEY keeps the raw Python name (a legal object key /
        // HTTP path), but the CALL targets the exported wrapper, whose name is
        // reserved-word-sanitized above (`default` → `default$`).
        out.push_str(&format!(
            "        {}: () => {}({}){}\n",
            e.name,
            sanitize_js_param(&e.name),
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
    // i64 elements: `DataView.setBigInt64` silently wraps mod 2**64 (ES
    // ToBigInt64), so an element past i64 would cross the boundary as a
    // silently WRONG value — the same hazard `__i64Oob` guards for scalar
    // args, which the pre-guard marshaller missed (PoC: pick([2**63+7])
    // returned -9223372036854775801). Guard here and throw RangeError: a
    // boundary marshalling fault, so the #364 fallback ladder transparently
    // re-runs the call on the exact JS twin (js+wasm), and edge targets
    // fail loud — never a wrapped element.
    out.push_str("    else if (kind === 'i64') {\n");
    out.push_str(
        "      const b = typeof arr[i] === 'bigint' ? arr[i] : BigInt(Math.trunc(arr[i]));\n",
    );
    out.push_str("      if (b > 9223372036854775807n || b < -9223372036854775808n) throw new RangeError('OverflowError: list element exceeds the i64 range of the WASM fast path');\n");
    out.push_str("      view.setBigInt64(off, b, true);\n");
    out.push_str("    }\n");
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
    out.push('\n');
    // #484: SYMMETRIC marshalling. `__list_to_wasm` copies a mutable `list`
    // parameter INTO linear memory; on return the (possibly in-place-mutated)
    // buffer must be copied BACK into the SAME JS array so an out-parameter
    // fill kernel (the only way to get array output out of a scalar-return
    // @wasm kernel — #364) is visible to the caller, exactly as CPython's
    // aliasing semantics make it. This is the inverse of `__list_to_wasm`, but
    // it MUTATES `arr` in place (not a fresh array like `__list_from_wasm`), so
    // the caller's binding observes the write — the ONE write-back authority
    // for every element kind (i64 / f64 / i32-bool). FIXED-CAPACITY CONTRACT: a
    // length-changing mutation cannot be reflected into the fixed-cap buffer, so
    // a changed length header is refused LOUDLY (never a silent truncation);
    // length-changing list methods (append/pop/clear/extend) are already refused
    // at WASM admission and stay on the faithful JS path, so this guard is the
    // belt-and-suspenders second layer. The i32 branch yields real booleans: the
    // only admitted i32-element list is `list[bool]` (#364), whose element repr
    // is a Python bool — matching the pure-JS twin so the glue and JS paths agree.
    out.push_str("function __list_write_back(arr, ptr, kind) {\n");
    out.push_str("  const view = new DataView(__wasm.memory.buffer);\n");
    out.push_str("  const n = view.getInt32(ptr, true);\n");
    out.push_str("  if (n !== arr.length) throw new Error('PythScribe: @wasm out-parameter list length changed (' + arr.length + ' -> ' + n + '); a fixed-capacity linear-memory buffer cannot reflect a length-changing mutation \u{2014} compile with --target js for that kernel');\n");
    out.push_str("  const esize = kind === 'i32' ? 4 : 8;\n");
    // `typed` = the caller passed a TypedArray (the low-level channel, e.g.
    // BigInt64Array for list[int]) rather than a plain Array. A BigInt64Array
    // element MUST be assigned a BigInt (a Number throws), and a plain Array
    // wants the JS-idiomatic value (Number-in-safe-range for a Python int repr,
    // a real boolean for list[bool]). Distinguishing them keeps write-back from
    // breaking a valid read-only call made with a typed array (no silent drop,
    // no spurious throw).
    out.push_str("  const typed = ArrayBuffer.isView(arr);\n");
    out.push_str("  for (let i = 0; i < n; i++) {\n");
    out.push_str("    const off = ptr + 8 + i * esize;\n");
    out.push_str("    if (kind === 'f64') arr[i] = view.getFloat64(off, true);\n");
    out.push_str("    else if (kind === 'i64') { const v = view.getBigInt64(off, true); arr[i] = typed ? v : ((v >= -9007199254740991n && v <= 9007199254740991n) ? Number(v) : v); }\n");
    out.push_str(
        "    else { const v = view.getInt32(off, true); arr[i] = typed ? v : (v !== 0); }\n",
    );
    out.push_str("  }\n");
    out.push_str("}\n");
}

/// Whether any export takes an `Array[dtype, ndim]` param (so the glue must
/// define the typed-array marshalling helpers). Array RETURNS are refused at
/// admission (#377 → M6), so only params matter here.
fn uses_array_boundary(exports: &[WasmExportInfo]) -> bool {
    exports.iter().any(|e| {
        e.params
            .iter()
            .any(|(_, ty)| matches!(ty, WasmType::PtrArray { .. }))
    })
}

/// Emit `__array_to_wasm` / `__array_write_back` — the js+wasm TYPED-ARRAY
/// marshallers (M2a-3b). They byte-agree with the SERVER channel
/// `pythscribe/runtime/array_buffer.py` and the codegen's `PtrArray` load/store
/// (`emit.rs`): the 16-byte ×8 header `[dtype tag:i32 @0][ndim:i32 @4]
/// [shape0:i32 @8][pad:i32 @12]` with the element region at ptr+16, row-major.
/// The dtype tag codes are `ArrayDtype` declaration order
/// (int32=0, int64=1, float32=2, float64=3, uint8=4) — the SAME single source as
/// `array_buffer.DTYPE_TAG`.
///
/// The TOTAL runtime check (requirements B1) lives in `__array_to_wasm`: the JS
/// buffer MUST be a `TypedArray` whose constructor and `BYTES_PER_ELEMENT` match
/// the compiled-for dtype, and ndim must be 1 — else it throws a `RangeError`,
/// which the #364 ladder (`__isWasmFault`) reroutes to the exact JS twin
/// (a twinless edge target surfaces the loud error). A dtype/width/ndim mismatch
/// is therefore NEVER flat-copied and read at the wrong width (no silent misread).
/// The element region is crossed in ONE bulk `.set()` each way — "buffer, not
/// elements".
fn emit_array_helpers(out: &mut String) {
    out.push('\n');
    // dtype metadata: TypedArray ctor + element width + the header dtype tag.
    // BigInt64Array elements are BigInt; the others are Number. Kept in
    // `ArrayDtype` declaration order so the tag == the array index it is defined
    // at is coincidental — the tag is written EXPLICITLY.
    out.push_str("const __ARR_DTYPES = {\n");
    out.push_str("  int32:   { ctor: Int32Array,    esize: 4, tag: 0 },\n");
    out.push_str("  int64:   { ctor: BigInt64Array, esize: 8, tag: 1 },\n");
    out.push_str("  float32: { ctor: Float32Array,  esize: 4, tag: 2 },\n");
    out.push_str("  float64: { ctor: Float64Array,  esize: 8, tag: 3 },\n");
    out.push_str("  uint8:   { ctor: Uint8Array,    esize: 1, tag: 4 },\n");
    out.push_str("};\n");
    out.push('\n');
    // A single admitted-dtype TypedArray row check (shared by 1-D and each 2-D
    // row): the buffer must be a TypedArray of EXACTLY the compiled-for dtype
    // (constructor + BYTES_PER_ELEMENT). A plain Array, a wrong-dtype TypedArray
    // or a DataView is refused — RangeError → the #364 ladder reroutes to the JS
    // twin; never flat-copied and read at the wrong width (the silent-misread
    // class the server `check_array_buffer` also bars).
    out.push_str("function __arrRowOk(row, d) {\n");
    out.push_str("  return ArrayBuffer.isView(row) && row.constructor === d.ctor && row.BYTES_PER_ELEMENT === d.esize;\n");
    out.push_str("}\n");
    out.push('\n');
    // __array_to_wasm(arr, dtype, ndim): the TOTAL runtime check + bulk copy in.
    // ndim=1: `arr` is a flat TypedArray. ndim=2: `arr` is an Array of `nrows`
    // rows, each a TypedArray of the compiled-for dtype, all of equal length
    // `ncols` (rectangular / C-contiguous). A non-{1,2} ndim, a wrong row type,
    // or a ragged 2-D input is a boundary fault (RangeError → twin).
    out.push_str("function __array_to_wasm(arr, dtype, ndim) {\n");
    out.push_str("  const d = __ARR_DTYPES[dtype];\n");
    out.push_str("  let nrows, ncols, n;\n");
    out.push_str("  if (ndim === 1) {\n");
    out.push_str("    if (!__arrRowOk(arr, d)) {\n");
    out.push_str("      const got = (arr && arr.constructor && arr.constructor.name) ? arr.constructor.name : typeof arr;\n");
    out.push_str("      throw new RangeError('pythscribe: Array[' + dtype + '] expects a ' + d.ctor.name + ' buffer (dtype/width match); got ' + got + ' \u{2014} refused (rerouted to the JS path, never read at the wrong width)');\n");
    out.push_str("    }\n");
    out.push_str("    nrows = arr.length; ncols = 0; n = arr.length;\n");
    out.push_str("  } else if (ndim === 2) {\n");
    // A 2-D array crosses as an Array of same-dtype rows. A flat TypedArray, a
    // non-array, a ragged shape, or a wrong row dtype is refused (no silent
    // strided / wrong-width read — the server `check_array_buffer` bars the same).
    out.push_str("    if (!Array.isArray(arr)) throw new RangeError('pythscribe: Array[' + dtype + ', 2] expects an array of ' + d.ctor.name + ' rows; got ' + (typeof arr) + ' \u{2014} refused');\n");
    out.push_str("    nrows = arr.length;\n");
    out.push_str("    ncols = 0;\n");
    // Each row is validated (TypedArray of the compiled-for dtype/width) BEFORE its
    // `.length` is read, so a null / non-array row 0 is a RangeError (rerouted to the
    // twin), never a raw JS TypeError. ncols is taken from the validated row 0.
    out.push_str("    for (let r = 0; r < nrows; r++) {\n");
    out.push_str("      if (!__arrRowOk(arr[r], d)) throw new RangeError('pythscribe: Array[' + dtype + ', 2] row ' + r + ' must be a ' + d.ctor.name + ' buffer (dtype/width match) \u{2014} refused');\n");
    out.push_str("      if (r === 0) ncols = arr[0].length;\n");
    out.push_str("      if (arr[r].length !== ncols) throw new RangeError('pythscribe: Array[' + dtype + ', 2] is ragged (row ' + r + ' length ' + arr[r].length + ' != ' + ncols + '); only C-contiguous rectangular 2-D arrays cross \u{2014} refused');\n");
    out.push_str("    }\n");
    out.push_str("    n = nrows * ncols;\n");
    out.push_str("  } else {\n");
    out.push_str("    throw new RangeError('pythscribe: Array marshalling supports ndim \u{2208} {1, 2}; got ndim=' + ndim + ' \u{2014} refused');\n");
    out.push_str("  }\n");
    // Allocate header + elements, rounded up to a multiple of 8 (array_buffer
    // `array_alloc_size`), plus 8 slack so the returned base can be aligned UP to
    // a multiple of 8 regardless of the current bump pointer — a JS TypedArray
    // view over WASM memory THROWS on a non-width-aligned byte offset, and the
    // ×8 header keeps ptr+16 8-aligned once the base is (requirements B3).
    out.push_str("  const allocSize = ((16 + n * d.esize + 7) & ~7) + 8;\n");
    out.push_str("  const raw = __wasm.__alloc(allocSize);\n");
    out.push_str("  const ptr = (raw + 7) & ~7;\n");
    // Header — written with a DataView (unaligned-safe): dtype tag @0, ndim @4,
    // shape0 (rows) @8, shape1 (cols; 0 for 1-D) @12. Elements @ptr+16.
    // The DataView is created AFTER __alloc (which may grow + detach memory).
    out.push_str("  const view = new DataView(__wasm.memory.buffer);\n");
    out.push_str("  view.setInt32(ptr, d.tag, true);\n");
    out.push_str("  view.setInt32(ptr + 4, ndim, true);\n");
    out.push_str("  view.setInt32(ptr + 8, nrows, true);\n");
    out.push_str("  view.setInt32(ptr + 12, ncols, true);\n");
    // Element region: 1-D = one bulk `.set()`; 2-D = one bulk `.set()` per row
    // (row-major, contiguous — "buffer not elements", never per-element).
    out.push_str("  if (ndim === 1) {\n");
    out.push_str("    new d.ctor(__wasm.memory.buffer, ptr + 16, n).set(arr);\n");
    out.push_str("  } else {\n");
    out.push_str("    for (let r = 0; r < nrows; r++) new d.ctor(__wasm.memory.buffer, ptr + 16 + r * ncols * d.esize, ncols).set(arr[r]);\n");
    out.push_str("  }\n");
    out.push_str("  return ptr;\n");
    out.push_str("}\n");
    out.push('\n');
    // __array_write_back(arr, ptr, dtype, ndim): self-checking bulk copy OUT.
    // Mirrors `array_buffer.read_back_element_bytes` — the header is re-parsed and
    // VALIDATED against the compiled-for (dtype tag, ndim, shape) before the
    // element region is trusted, so a drifted / corrupted header is refused
    // (a plain Error — a genuine corruption, NOT a boundary fault to reroute:
    // matches the list write-back length-guard disposition, propagates loud).
    out.push_str("function __array_write_back(arr, ptr, dtype, ndim) {\n");
    out.push_str("  const d = __ARR_DTYPES[dtype];\n");
    out.push_str("  const view = new DataView(__wasm.memory.buffer);\n");
    out.push_str("  const tag = view.getInt32(ptr, true);\n");
    out.push_str("  const hdrNdim = view.getInt32(ptr + 4, true);\n");
    out.push_str("  const nrows = view.getInt32(ptr + 8, true);\n");
    out.push_str("  const ncols = view.getInt32(ptr + 12, true);\n");
    out.push_str("  if (tag !== d.tag) throw new Error('PythScribe: @wasm array write-back dtype tag ' + tag + ' != expected ' + d.tag + ' (' + dtype + '); a wrong-width read would corrupt values \u{2014} refused');\n");
    out.push_str("  if (hdrNdim !== ndim) throw new Error('PythScribe: @wasm array write-back ndim ' + hdrNdim + ' != expected ' + ndim + '; refused');\n");
    out.push_str("  if (ndim === 1) {\n");
    out.push_str("    if (nrows !== arr.length) throw new Error('PythScribe: @wasm array out-parameter length changed (' + arr.length + ' -> ' + nrows + '); a fixed-capacity linear-memory buffer cannot reflect a length-changing mutation \u{2014} compile with --target js for that kernel');\n");
    out.push_str("    arr.set(new d.ctor(__wasm.memory.buffer, ptr + 16, nrows));\n");
    out.push_str("  } else {\n");
    out.push_str("    if (nrows !== arr.length) throw new Error('PythScribe: @wasm 2-D array out-parameter row count changed (' + arr.length + ' -> ' + nrows + '); refused');\n");
    out.push_str("    for (let r = 0; r < nrows; r++) {\n");
    out.push_str("      if (arr[r].length !== ncols) throw new Error('PythScribe: @wasm 2-D array out-parameter row ' + r + ' length changed (' + arr[r].length + ' -> ' + ncols + '); refused');\n");
    out.push_str("      arr[r].set(new d.ctor(__wasm.memory.buffer, ptr + 16 + r * ncols * d.esize, ncols));\n");
    out.push_str("    }\n");
    out.push_str("  }\n");
    out.push_str("}\n");
}

// ---------------------------------------------------------------------------
// Marshalling decision table — the Rust<->Lean binding surface for the
// JS<->WASM VALUE boundary (mirrors cert.rs's `admission_table()` +
// verification/wasm-admission-table.txt; committed fixture:
// verification/marshalling-table.txt).
//
// Three row kinds, all DERIVED from the shipping code, never restated:
//   `arg <shape> -> <bit> ; <expr>` — <bit> = pyths_hir::is_numeric_kernel_param
//       (the #364 boundary admission for params), <expr> = the LITERAL JS
//       conversion `convert_js_to_wasm("x", ..)` emits for the lowered type
//       (`-` when `to_wasm_type` = None: no representation, nothing to marshal).
//   `ret <shape> -> <bit> ; <expr>` — <bit> = pyths_hir::is_scalar_wasm_return
//       (#364 scalar-return restriction), <expr> = `convert_wasm_to_js("x", ..)`.
//   `fault <event> <mode> -> <action>` — the overflow/failure DISPOSITIONS of
//       the boundary, derived by generating REAL probe bridges (twins /
//       no-twins) via `generate_bridge` and locating the discriminating
//       emitted snippet; every derivation panics loudly if the shipped
//       emitter no longer contains the snippet it keys on, so a changed
//       disposition cannot silently keep its old row.
//
// The committed fixture is compared against this from Rust
// (`cargo test marshalling_table_matches_committed_fixture`) AND from Lean
// (`lake exe expanddiff --check-marshalling-table`); either side changing a
// conversion or disposition breaks its own gate. Every `arg` row with bit=1
// also finitely witnesses `marshal_param_admitted_sound` (admitted params
// marshal ONLY through the value-exact numeric converters — see the Lean
// MarshalTable section and the `admitted_arg_rows_use_exact_marshallers`
// test below).
// ---------------------------------------------------------------------------

/// The boundary-type shape alphabet, in table order. Chosen to hit EVERY match
/// arm of `convert_js_to_wasm` / `convert_wasm_to_js` / `list_elem_kind`
/// (i64 / f64 / i32 / str-ptr / list of every leaf incl. the ptr-elem and
/// nested-list i32 kinds / dict / tuple / closure with and without a return
/// value), every arm of `is_numeric_kernel_param`, and every arm of
/// `is_scalar_wasm_return` (incl. the void-return `none`/`void` rows).
fn marshalling_alphabet() -> Vec<(String, pyths_types::types::Type)> {
    use pyths_types::types::{ArrayDtype, Type};
    let leaves = vec![
        ("int", Type::Int),
        ("float", Type::Float),
        ("bool", Type::Bool),
        ("str", Type::Str),
        ("none", Type::NoneType),
        ("any", Type::Any),
        ("void", Type::Void),
        ("named", Type::Named("T".to_string())),
    ];
    let mut shapes: Vec<(String, Type)> = leaves
        .iter()
        .map(|(n, t)| (n.to_string(), t.clone()))
        .collect();
    for (n, t) in &leaves {
        shapes.push((format!("list<{n}>"), Type::List(Box::new(t.clone()))));
    }
    shapes.push((
        "list<list<int>>".to_string(),
        Type::List(Box::new(Type::List(Box::new(Type::Int)))),
    ));
    shapes.push(("set<int>".to_string(), Type::Set(Box::new(Type::Int))));
    shapes.push(("opt<int>".to_string(), Type::Optional(Box::new(Type::Int))));
    shapes.push((
        "dict<str,int>".to_string(),
        Type::Dict(Box::new(Type::Str), Box::new(Type::Int)),
    ));
    shapes.push((
        "tuple<int,float>".to_string(),
        Type::Tuple(vec![Type::Int, Type::Float]),
    ));
    shapes.push((
        "callable<int,int>".to_string(),
        Type::Callable(vec![Type::Int], Box::new(Type::Int)),
    ));
    shapes.push((
        "callable<int,none>".to_string(),
        Type::Callable(vec![Type::Int], Box::new(Type::NoneType)),
    ));
    // M2a-4/M2b: numeric arrays — the crossable buffer shapes. BOTH 1-D and
    // 2-D arrays are now admitted params (`is_numeric_kernel_param` arg bit=1)
    // and cross the boundary, so both appear in the marshalling alphabet (the
    // arg conversion `__array_to_wasm(x, dtype, ndim)` threads ndim). Dtype
    // order matches `WasmArrayDtype` / `cert::admission_table`; per dtype, ndim
    // ascends 1 then 2 (matching the Lean `marshalAlphabet` order).
    for dt in [
        ArrayDtype::Int32,
        ArrayDtype::Int64,
        ArrayDtype::Float32,
        ArrayDtype::Float64,
        ArrayDtype::Uint8,
    ] {
        for ndim in [1u32, 2] {
            shapes.push((
                format!("array<{},{}>", dt.spelling(), ndim),
                Type::Array(dt, ndim),
            ));
        }
    }
    shapes
}

fn bit(b: bool) -> char {
    if b {
        '1'
    } else {
        '0'
    }
}

/// Find the discriminating snippet for a fault row, or panic loudly: a table
/// derivation must never fabricate a disposition the emitter no longer ships.
fn derived(glue: &str, snippet: &str, event: &str, action: &'static str) -> &'static str {
    assert!(
        glue.contains(snippet),
        "marshalling_table: shipped bridge no longer contains the snippet \
         deriving `fault {event} -> {action}`:\n  {snippet}\nregenerate the \
         disposition rows from the current emitter",
        event = event,
        action = action,
        snippet = snippet,
    );
    action
}

/// The full JS<->WASM marshalling decision table. Byte-identical to the Lean
/// `marshallingTable` (same shape alphabet, same row format).
pub fn marshalling_table() -> String {
    use crate::types::to_wasm_type;
    use pyths_hir::{is_numeric_kernel_param, is_scalar_wasm_return};

    let mut out = String::new();

    // Section 1: per-shape conversion rows (arg + ret per shape).
    for (name, ty) in marshalling_alphabet() {
        let arg_expr = match to_wasm_type(&ty) {
            Some(w) => convert_js_to_wasm("x", &w),
            None => "-".to_string(),
        };
        let ret_expr = match to_wasm_type(&ty) {
            Some(w) => convert_wasm_to_js("x", &w),
            None => "-".to_string(),
        };
        let _ = writeln!(
            out,
            "arg {} -> {} ; {}",
            name,
            bit(is_numeric_kernel_param(&ty)),
            arg_expr
        );
        let _ = writeln!(
            out,
            "ret {} -> {} ; {}",
            name,
            bit(is_scalar_wasm_return(&ty)),
            ret_expr
        );
    }

    // Section 1b: write-back rows (#484). SYMMETRIC marshalling — every list
    // param the glue copies IN via `__list_to_wasm` is copied BACK via
    // `__list_write_back` on return (the scalar-return out-parameter fill idiom,
    // the only way #364 leaves to produce array output). One row per shape whose
    // WASM representation is a list pointer, mirroring the `arg` rows'
    // `__list_to_wasm` conversions: `x` = the JS array (mutated in place), `p` =
    // the linear-memory pointer `__list_to_wasm` returned. The element kind is
    // the SAME `list_elem_kind` the `arg`/`ret` list rows bind, so write-back's
    // value fidelity is the arg/ret rows' fidelity in the inverse direction.
    for (name, ty) in marshalling_alphabet() {
        match to_wasm_type(&ty) {
            Some(WasmType::PtrList(inner)) => {
                let _ = writeln!(
                    out,
                    "wb {} -> __list_write_back(x, p, {:?})",
                    name,
                    list_elem_kind(&inner)
                );
            }
            // M2a-4: array out-parameter write-back — the typed-array
            // counterpart of the #484 list write-back. An admitted 1-D array
            // param copied IN via `__array_to_wasm` is copied BACK via
            // `__array_write_back` (bridge.rs array write-back block), mutating
            // the caller's TypedArray in place (the scalar-return array-output
            // idiom). Byte-mirrors the Lean `marshalWbRow` array arm.
            Some(WasmType::PtrArray { dtype, ndim }) => {
                let _ = writeln!(
                    out,
                    "wb {} -> __array_write_back(x, p, {:?}, {})",
                    name,
                    dtype.spelling(),
                    ndim
                );
            }
            _ => {}
        }
    }

    // Section 2: overflow/failure dispositions, derived from REAL probe
    // bridges. probe(x: i64) -> i64 and plist(xs: list[i64]) -> i64 exercise
    // the scalar guard, the element guard, the __ovf check and the ladder.
    let exports = vec![
        WasmExportInfo {
            name: "probe".to_string(),
            params: vec![("x".to_string(), WasmType::I64)],
            return_type: Some(WasmType::I64),
        },
        WasmExportInfo {
            name: "plist".to_string(),
            params: vec![("xs".to_string(), WasmType::PtrList(Box::new(WasmType::I64)))],
            return_type: Some(WasmType::I64),
        },
    ];
    let twins_src =
        "export function probe(x) { return x; }\nexport function plist(xs) { return xs[0]; }";
    let empty_exc = BTreeMap::new();
    let math = BTreeSet::new();
    let twins = generate_bridge(
        "m.wasm",
        &exports,
        &math,
        false,
        true,
        &empty_exc,
        false,
        true,
        Some(twins_src),
    );
    let notwins = generate_bridge(
        "m.wasm", &exports, &math, false, true, &empty_exc, false, true, None,
    );

    // The shipped fault predicate — asserted VERBATIM so the `py-exception ->
    // propagate` row (derived from what the predicate does NOT match) cannot
    // survive a predicate change.
    let fault_pred = "const __isWasmFault = (e) => (typeof WebAssembly !== \"undefined\" && e instanceof WebAssembly.RuntimeError) || e instanceof RangeError;";
    let elem_guard = "throw new RangeError('OverflowError: list element exceeds the i64 range of the WASM fast path');";

    let mut fr = |event: &str, mode: &str, action: &str| {
        let _ = writeln!(out, "fault {} {} -> {}", event, mode, action);
    };

    // i64-arg-oob: BigInt arg beyond i64 would wrap in the JS->WASM conversion.
    fr(
        "i64-arg-oob",
        "twins",
        derived(
            &twins,
            "if (__i64Oob(x)) return __jsfb.probe(x);",
            "i64-arg-oob twins",
            "reroute-twin",
        ),
    );
    fr(
        "i64-arg-oob",
        "notwins",
        derived(
            &notwins,
            "if (__i64Oob(x)) throw new RangeError(",
            "i64-arg-oob notwins",
            "throw-range",
        ),
    );
    // list-elem-i64-oob: element guard throws RangeError; the ladder (twins)
    // classifies RangeError a fault and re-runs on the twin; no ladder (edge)
    // -> the RangeError propagates loud.
    assert!(
        notwins.contains(elem_guard) && !notwins.contains("__isWasmFault"),
        "marshalling_table: no-twins bridge must guard i64 elements and have no fault ladder"
    );
    fr("list-elem-i64-oob", "twins", {
        derived(
            &twins,
            elem_guard,
            "list-elem-i64-oob twins (guard)",
            "reroute-twin",
        );
        derived(
            &twins,
            "if (__isWasmFault(__e)) return __jsfb.plist(xs);",
            "list-elem-i64-oob twins (ladder)",
            "reroute-twin",
        )
    });
    fr("list-elem-i64-oob", "notwins", "throw-range");
    // ovf-flag: the sticky i64-exactness flag set by in-WASM arithmetic.
    fr(
        "ovf-flag",
        "twins",
        derived(
            &twins,
            "if (__wasm.__ovf.value) {\n    __wasm.__ovf.value = 0;\n    __wasm.__err_code.value = 0;\n    return __jsfb.probe(x);",
            "ovf-flag twins",
            "reroute-twin",
        ),
    );
    fr(
        "ovf-flag",
        "notwins",
        derived(
            &notwins,
            "throw new Error(\"OverflowError: integer result exceeds the i64 range",
            "ovf-flag notwins",
            "throw-overflow",
        ),
    );
    // wasm-trap: any WebAssembly.RuntimeError (OOB memory access, unreachable,
    // div-by-zero, ...). Twins: the ladder re-runs on the twin. No twins: no
    // ladder, the trap propagates loud.
    fr("wasm-trap", "twins", {
        derived(
            &twins,
            fault_pred,
            "wasm-trap twins (predicate)",
            "reroute-twin",
        );
        derived(
            &twins,
            "if (__isWasmFault(__e)) return __jsfb.probe(x);",
            "wasm-trap twins (ladder)",
            "reroute-twin",
        )
    });
    fr("wasm-trap", "notwins", "propagate-trap");
    // py-exception: a deliberate Python exception dispatched by __check_err is
    // a plain Error with a .name — NOT matched by the (verbatim-asserted)
    // fault predicate, so the catch rethrows it unchanged in both modes.
    fr("py-exception", "twins", {
        derived(
            &twins,
            fault_pred,
            "py-exception twins (predicate)",
            "propagate-py",
        );
        derived(
            &twins,
            "throw __e;",
            "py-exception twins (rethrow)",
            "propagate-py",
        )
    });
    fr(
        "py-exception",
        "notwins",
        derived(
            &notwins,
            "function __check_err()",
            "py-exception notwins",
            "propagate-py",
        ),
    );

    out
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
        // f64 params take a native double. A large-int arg arrives as a
        // BigInt (the hybrid int form past 2**53) — ToNumber at the WASM
        // JS-API boundary THROWS on BigInt, so __f64Arg coerces it explicitly
        // with the standard int→float conversion. #465: the overflow branch
        // now MATCHES the runtime authority __reqNum — a BigInt beyond double
        // range raises OverflowError ("int too large to convert to float",
        // CPython float(10**400)) instead of silently passing Infinity. A
        // native Number (incl. a boxed __PyFloatW, which ToNumber unwraps via
        // [[NumberData]]) still passes through untouched. #38/#461/#465.
        WasmType::F64 => format!("__f64Arg({n})", n = name),
        WasmType::I32 => format!("{} ? 1 : 0", name),
        WasmType::Ptr => format!("__str_to_wasm({})", name),
        WasmType::PtrList(inner) => {
            format!("__list_to_wasm({}, {:?})", name, list_elem_kind(inner))
        }
        WasmType::PtrDict(_, _) => format!("__dict_to_wasm({})", name),
        WasmType::PtrTuple(_) => format!("__tuple_to_wasm({})", name),
        WasmType::PtrClosure { .. } => format!("__closure_to_wasm({})", name),
        // M2a-3b: FIRST-CLASS array glue. A `TypedArray` param is marshalled
        // into WASM linear memory in the `array_buffer.py` layout (16-byte ×8
        // header `[dtype tag @0][ndim @4][shape0 @8][pad @12]`, elements @16),
        // byte-agreeing with the Rust server marshaller and the codegen's
        // `PtrArray` load/store (`emit.rs`: elems@16, shape0@8). `__array_to_wasm`
        // runs the TOTAL runtime check — the buffer MUST be a `TypedArray` of the
        // compiled-for dtype/ndim, else it throws a `RangeError` so `__isWasmFault`
        // reroutes to the exact JS twin (#364; a twinless edge target surfaces the
        // loud error). Never a silent misread. NB: in the CURRENT codegen every
        // array param is bound through the write-back local `__wb_arg_<i>` and its
        // pointer produced by `__array_to_wasm` in the PRELUDE (see the array
        // write-back block below), so this inline arm is not reached today; it is
        // kept correct-and-defensive and becomes the marshalling-table derivation
        // site once M2a-4 adds `Type::Array` shapes to `marshalling_alphabet`.
        WasmType::PtrArray { dtype, ndim } => {
            format!(
                "__array_to_wasm({}, {:?}, {})",
                name,
                dtype.spelling(),
                ndim
            )
        }
        // A bare F32 scalar never crosses the boundary (f32 is only an array
        // element load/store type; array elements promote to f64 on the compute
        // stack — M2a-0 review W1). Defensive throw, never a compiler panic.
        WasmType::F32 => format!(
            "(()=>{{throw new RangeError('pythscribe: a bare f32 scalar is not a boundary type \
             (f32 is only an array element width). arg={}')}})()",
            name
        ),
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
        WasmType::F64 => format!("__f64Box({})", expr),
        WasmType::I32 => format!("Boolean({})", expr),
        WasmType::Ptr => format!("__str_from_wasm({})", expr),
        WasmType::PtrList(inner) => {
            format!("__list_from_wasm({}, {:?})", expr, list_elem_kind(inner))
        }
        WasmType::PtrDict(_, _) => format!("__dict_from_wasm({})", expr),
        WasmType::PtrTuple(_) => format!("__tuple_from_wasm({})", expr),
        WasmType::PtrClosure { .. } => format!("__closure_from_wasm({})", expr),
        // Non-scalar array RETURNS are #377 → M6 (never in M2 scope), and
        // `check_signature` refuses an `Array` return from WASM admission, so a
        // well-formed admitted function never reaches this arm. Emit a throwing
        // `RangeError` (routed to the JS twin by the #364 ladder) rather than a
        // compiler panic, defensively. F32 is never a top-level return type.
        WasmType::F32 | WasmType::PtrArray { .. } => format!(
            "(()=>{{throw new RangeError('pythscribe: Array/f32 return marshalling is out of \
             scope (array returns are #377/M6). expr={}')}})()",
            expr
        ),
    }
}
