use std::path::Path;
use std::time::{Duration, Instant};

use super::cache;
use super::output::{self, Verbosity};
use super::safewrite::{self, generated_header, OutputKind, OutputPlan, PlannedOutput};

/// Prepend one empty generated line to a source map's `mappings` field (a
/// leading `;`) so mapped frames still resolve after a single header line was
/// prepended to the `.js`. Returns the original string unchanged if the field
/// can't be located (defensive — better an unshifted map than a corrupted one).
fn shift_sourcemap_one_line(map_json: &str) -> String {
    let needle = "\"mappings\":";
    if let Some(i) = map_json.find(needle) {
        let after = &map_json[i + needle.len()..];
        let trimmed = after.trim_start();
        let ws = after.len() - trimmed.len();
        if trimmed.starts_with('"') {
            let insert_at = i + needle.len() + ws + 1; // just past the opening quote
            let mut out = String::with_capacity(map_json.len() + 1);
            out.push_str(&map_json[..insert_at]);
            out.push(';');
            out.push_str(&map_json[insert_at..]);
            return out;
        }
    }
    map_json.to_string()
}

/// A16: build the `deno.json` scaffold for the `deno` target. The entry
/// filename is derived from the user's `-o`, so it can contain a space or a
/// `"`. Building the JSON via `serde_json` guarantees the value is escaped and
/// the emitted `deno.json` is always valid JSON (a hand-rolled `format!` with
/// the raw filename could break the string or splice into the task command).
///
/// NOTE (follow-up): the task runs `deno run --allow-read <entry>`. The blanket
/// `--allow-read` is broad for a scaffold; tightening it to the emitted file
/// set (and quoting the entry for the task shell when it contains spaces) is a
/// tracked hardening enhancement, not done here.
fn build_deno_json(entry_basename: &str) -> String {
    let value = serde_json::json!({
        "tasks": {
            "start": format!("deno run --allow-read {}", entry_basename)
        }
    });
    format!(
        "{}\n",
        serde_json::to_string_pretty(&value).expect("serialize deno.json")
    )
}

/// SEC #4 (CWE-94/116): a source-derived output name carrying a control
/// character can break out of a single-line `//# sourceMappingURL=` comment
/// (CR/LF), a TOML string, or another single-line generated context. Reject
/// such names before any of them reaches a generated syntax context.
pub(crate) fn reject_control_bearing_name(kind: &str, name: &str) -> Result<(), Box<dyn std::error::Error>> {
    // `char::is_control` covers only the Unicode Cc category (U+0000–001F,
    // U+007F–009F). It MISSES U+2028 (LINE SEPARATOR) and U+2029 (PARAGRAPH
    // SEPARATOR), which JavaScript treats as line terminators — so a name
    // carrying one breaks out of a generated `// …` comment into executable
    // bundle code exactly like a raw newline would (this function's entire
    // reason to exist — the CWE-94/116 sink). Reject those two explicitly.
    if let Some(c) = name
        .chars()
        .find(|c| c.is_control() || *c == '\u{2028}' || *c == '\u{2029}')
    {
        return Err(format!(
            "refusing to build: the {} {:?} contains a control or line-separator \
             character (U+{:04X}).\n  \
             Such a name can break out of a generated comment or config file; rename the \
             source file or choose a different output path (-o).",
            kind, name, c as u32
        )
        .into());
    }
    Ok(())
}

/// SEC #4: serialize a string as a TOML basic string (quoted + escaped) so an
/// output name containing `"`, `\`, or control characters cannot corrupt the
/// generated Wrangler config.
fn toml_basic_string(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    out.push('"');
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 || c as u32 == 0x7f => {
                out.push_str(&format!("\\u{:04X}", c as u32))
            }
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

/// Drain the JS codegen depth-overflow marker and, if set, render a clean
/// compile-time diagnostic. `emit_expr` records the marker (and emits a
/// runtime-throwing placeholder) instead of overflowing the native stack when
/// an expression nests deeper than `MAX_EMIT_DEPTH`; this converts that marker
/// into a rendered error so the compilation fails cleanly (non-zero exit)
/// rather than shipping a booby-trapped module.
fn codegen_depth_error(source: &str, filename: &str) -> Option<Box<dyn std::error::Error>> {
    let offset = pyths_codegen_js::take_emit_overflow()?;
    let diag = pyths_diagnostic::errors::Diagnostic::error(
        format!(
            "expression nested too deeply to compile (exceeds the maximum of {} levels)",
            pyths_codegen_js::MAX_EMIT_DEPTH
        ),
        pyths_syntax::span::Span::new(offset, offset),
    )
    .with_note(
        "simplify or split the deeply-nested expression — this limit guards against \
         stack exhaustion on pathological input",
    );
    let rendered = pyths_diagnostic::render::render_diagnostics(source, filename, &[diag]);
    Some(rendered.into())
}

/// §4.7.1 (pythscribe #357): the resolved compilation target plus whether the
/// user pinned it explicitly. `explicit == false` means automatic routing —
/// the additive-tier union, implemented today as `js+wasm` semantics
/// (numeric kernels → WASM with transparent glue, everything else → JS).
/// The conflict model (§4.7.3) keys off `explicit`: a flag is a CONSTRAINT
/// (so `@wasm` + explicit `--target js` is a hard conflict), while absence is
/// full auto (bare functions never error on placement).
#[derive(Debug)]
pub(crate) struct ResolvedTarget<'a> {
    pub name: &'a str,
    pub explicit: bool,
}

/// The valid `--target` values (the explicit pinning escape hatches).
const KNOWN_TARGETS: &[&str] = &["js", "worker", "wasm", "js+wasm", "wasm-edge", "wasi", "deno"];

/// ONE consolidated place where "what target are we compiling for?" is
/// decided (deep-over-local: every consumer below reads the resolved value,
/// never the raw flag).
///
/// - `None` (no `--target` flag) → automatic routing = `js+wasm` semantics.
///   EXCEPTION: with `--stdout` the auto default resolves to `js` — stdout
///   mode prints one module and must not spray `.wasm`/`.glue.js` sidecar
///   files next to the source (same principle as `.d.ts` staying off for
///   `--stdout`). An EXPLICIT `--target js+wasm --stdout` still writes the
///   WASM artifacts (the user pinned it).
/// - `Some(t)` → explicit target, validated against the known set.
pub(crate) fn resolve_target<'a>(
    arg: Option<&'a str>,
    stdout: bool,
) -> Result<ResolvedTarget<'a>, Box<dyn std::error::Error>> {
    match arg {
        Some(t) => {
            if !KNOWN_TARGETS.contains(&t) {
                return Err(format!(
                    "unknown --target `{}` (expected one of: {})",
                    t,
                    KNOWN_TARGETS.join(", ")
                )
                .into());
            }
            Ok(ResolvedTarget {
                name: t,
                explicit: true,
            })
        }
        None => Ok(ResolvedTarget {
            name: if stdout { "js" } else { "js+wasm" },
            explicit: false,
        }),
    }
}

/// Names of top-level functions carrying a user-written `@wasm` decorator.
/// (WASM placement analysis only considers top-level functions, so the strict
/// assertion scan matches that scope.)
fn wasm_decorated_functions(module: &pyths_syntax::ast::Module) -> Vec<String> {
    let mut out = Vec::new();
    for stmt in &module.body {
        if let pyths_syntax::ast::StmtKind::FuncDef {
            name,
            decorator_list,
            ..
        } = &stmt.kind
        {
            if decorator_list
                .iter()
                .any(|d| matches!(&d.kind, pyths_syntax::ast::ExprKind::Name(n) if n == "wasm"))
            {
                out.push(name.clone());
            }
        }
    }
    out
}

/// §4.7.3 conflict model — decorator = ASSERTION, flag = CONSTRAINT,
/// absence = full auto. Enforced in ONE place, before anything is written:
///
/// - `@wasm` on a function the WASM admission gate rejects → hard compile
///   error carrying the eligibility-rejection reason (previously the function
///   silently stayed JS — the assertion was soft).
/// - `@wasm` under an EXPLICIT JS-only `--target` (js / worker) → hard
///   compile error with a hint.
/// - `@wasm` under the auto default with `--stdout` (which suppresses WASM
///   artifacts, see `resolve_target`) → hard compile error with a hint.
/// - BARE functions (no decorator) never error on placement under any
///   target: auto simply constrains routing to the pinned set.
fn enforce_wasm_placement(
    module: &pyths_syntax::ast::Module,
    target: &ResolvedTarget,
    needs_wasm: bool,
    wasm_output: Option<&pyths_codegen_wasm::WasmCodegenOutput>,
) -> Result<(), Box<dyn std::error::Error>> {
    let decorated = wasm_decorated_functions(module);
    if decorated.is_empty() {
        return Ok(());
    }

    if !needs_wasm {
        if target.explicit {
            return Err(format!(
                "`@wasm` on function `{}` conflicts with the explicit `--target {}`: \
                 this target never emits WASM.\n  \
                 hint: drop the `@wasm` decorator, or compile with automatic routing \
                 (omit --target) or an explicit `--target js+wasm`.",
                decorated[0], target.name
            )
            .into());
        }
        // Auto default + --stdout: WASM artifacts are suppressed (see
        // resolve_target), so the assertion cannot be honored.
        return Err(format!(
            "`@wasm` on function `{}` cannot be honored with --stdout: stdout mode \
             prints a single JS module and writes no WASM artifacts.\n  \
             hint: drop --stdout (automatic routing will emit the WASM), pass an \
             explicit `--target js+wasm`, or drop the `@wasm` decorator.",
            decorated[0]
        )
        .into());
    }

    // WASM-emitting target: every @wasm-asserted function must actually have
    // been admitted. The analysis records the rejection reason.
    let wo = wasm_output.expect("wasm_output present whenever needs_wasm");
    for name in &decorated {
        if wo.compiled_functions.contains(name) {
            continue;
        }
        let reason = wo
            .rejected_functions
            .iter()
            .find(|(n, _)| n == name)
            .map(|(_, r)| r.as_str())
            .unwrap_or("the function was not admitted by the WASM placement analysis");
        return Err(format!(
            "`@wasm` on function `{}` cannot be honored: {}\n  \
             hint: make the function WASM-eligible, or drop the `@wasm` decorator \
             to let automatic routing keep it on the JS path.",
            name, reason
        )
        .into());
    }
    Ok(())
}

/// Derive the `.d.ts` path from the output path by replacing only the FINAL
/// extension (round-5 fix): the old global `output_path.replace(".js", …)`
/// also rewrote DIRECTORY components, so `-o build.js/app.js --dts` derived
/// `build.d.ts/app.d.ts` — a directory that was never created — and the build
/// wrote the JS, then failed late on the missing dir (a partial build).
fn dts_path_for(output_path: &str) -> String {
    Path::new(output_path)
        .with_extension("d.ts")
        .to_string_lossy()
        .to_string()
}

/// #98: create the parent directory of an output path if it's missing, with
/// a clear error naming the directory (instead of the raw
/// "The system cannot find the path specified. (os error 3)").
fn ensure_parent_dir(out_path: &Path) -> Result<(), Box<dyn std::error::Error>> {
    if let Some(parent) = out_path.parent() {
        if !parent.as_os_str().is_empty() && !parent.exists() {
            std::fs::create_dir_all(parent).map_err(|e| {
                format!(
                    "could not create output directory {}: {}",
                    parent.display(),
                    e
                )
            })?;
        }
    }
    Ok(())
}

/// Returns `true` if the file should be passed through the `.psc` expander
/// before parsing, given the resolved mode and file extension.
fn should_expand(mode: pyths_config::ExpandMode, path: &Path) -> bool {
    match mode {
        pyths_config::ExpandMode::Always => true,
        pyths_config::ExpandMode::Never => false,
        pyths_config::ExpandMode::Auto => path
            .extension()
            .and_then(|s| s.to_str())
            .map(|ext| ext.eq_ignore_ascii_case("psc"))
            .unwrap_or(false),
    }
}

/// Step 9 (--watch enhancement): polling-based file watcher that recompiles
/// `file` whenever its mtime changes. Avoids pulling in the `notify` crate
/// for the simplest possible implementation. Cache makes no-op rebuilds
/// near-instant.
///
/// Exits on Ctrl+C (signal handling left to the OS / shell).
#[allow(clippy::too_many_arguments)]
pub fn watch(
    file: &str,
    output_path_arg: Option<&str>,
    sourcemap: bool,
    no_sources_content: bool,
    timings: bool,
    dts: bool,
    no_dts: bool,
    target: Option<&str>,
    expand_mode: Option<pyths_config::ExpandMode>,
    react_refresh: bool,
    force: bool,
    verbosity: Verbosity,
) -> Result<(), Box<dyn std::error::Error>> {
    let path = Path::new(file);
    if !path.exists() {
        return Err(format!("File not found: {}", file).into());
    }
    if verbosity != Verbosity::Quiet {
        output::success(&format!("Watching {} (Ctrl+C to exit)", file));
    }

    // Initial compile.
    let _ = run(
        file,
        output_path_arg,
        false,
        sourcemap,
        no_sources_content,
        timings,
        dts,
        no_dts,
        target,
        expand_mode,
        react_refresh,
        false,
        force,
        verbosity,
    );

    let mut last_mtime = std::fs::metadata(path).and_then(|m| m.modified()).ok();

    loop {
        std::thread::sleep(Duration::from_millis(200));
        let cur_mtime = match std::fs::metadata(path).and_then(|m| m.modified()) {
            Ok(m) => Some(m),
            Err(_) => continue,
        };
        if cur_mtime != last_mtime {
            last_mtime = cur_mtime;
            if verbosity != Verbosity::Quiet {
                eprintln!("--- change detected, recompiling ---");
            }
            // Run again. Errors print themselves; we keep watching.
            if let Err(e) = run(
                file,
                output_path_arg,
                false,
                sourcemap,
                no_sources_content,
                timings,
                dts,
                no_dts,
                target,
                expand_mode,
                react_refresh,
                false,
                force,
                verbosity,
            ) {
                output::error_summary(&format!("Error: {}", e));
            }
        }
    }
}

#[allow(clippy::too_many_arguments)]
pub fn run(
    file: &str,
    output_path_arg: Option<&str>,
    stdout: bool,
    sourcemap: bool,
    no_sources_content: bool,
    timings: bool,
    dts: bool,
    no_dts: bool,
    target: Option<&str>,
    expand_mode: Option<pyths_config::ExpandMode>,
    react_refresh: bool,
    emit_cert: bool,
    force: bool,
    verbosity: Verbosity,
) -> Result<(), Box<dyn std::error::Error>> {
    // §4.7.1: resolve the target ONCE. Everything below reads the resolved
    // name; `resolved.explicit` feeds the §4.7.3 conflict diagnostics.
    let resolved = resolve_target(target, stdout)?;
    let target = resolved.name;
    // `--dts` is now a redundant explicit-on (declarations emit by default
    // for file builds); it is accepted for script compatibility. `--no-dts`
    // is the suppression escape hatch and wins.
    let _ = dts;

    if emit_cert && (sourcemap || stdout) {
        return Err("--emit-cert does not support --sourcemap/--stdout".into());
    }
    if emit_cert && !matches!(target, "js" | "js+wasm" | "wasm") {
        return Err("--emit-cert supports --target js (subscript-routing) and js+wasm / wasm (WASM-admission) only".into());
    }
    let path = Path::new(file);
    if !path.exists() {
        return Err(format!("File not found: {}", file).into());
    }

    let show_timings = timings || verbosity == Verbosity::Verbose;

    let t_read = Instant::now();
    let raw_source = std::fs::read_to_string(path)?;
    let d_read = t_read.elapsed();

    // Phase 2: `.psc` expansion. Resolve effective mode from CLI flag,
    // pyths.toml, then default. Mode `Auto` checks the file extension;
    // `Always` expands unconditionally; `Never` short-circuits.
    let config = pyths_config::load_or_default();
    let mode = expand_mode.unwrap_or(config.expand.mode);
    let t_expand = Instant::now();
    let source = if should_expand(mode, path) {
        // Thread both the $NAME dictionary and the %NAME idiom map through.
        pyths_expand::expand_with_config(
            &raw_source,
            &config.expand.dictionary,
            &config.expand.idioms,
        )
    } else {
        raw_source
    };
    let d_expand = t_expand.elapsed();
    let source_len = source.len();

    // Step 9: incremental cache lookup. If the source + target + compiler
    // version have already produced an output that's still on disk with the
    // recorded hash, we skip the whole pipeline. The cache only tracks single
    // JS outputs — multi-file targets (js+wasm and edge targets that emit
    // worker.js + worker.wasm + scaffolds) skip the cache entirely. Since
    // `.d.ts` emission is now default-on (a second output the cache does not
    // track), the cache only engages for `--target js --no-dts` builds.
    // A relative star import (`from .mod import *`) and the `from . import X`
    // disambiguation both read SIBLING filesystem state (commands::relstar)
    // that the single-file cache key does not cover — a cache hit could
    // serve a stale artifact after only the sibling changed. Conservative
    // textual pre-check.
    let cache_disabled = std::env::var("PYTHS_NO_CACHE").is_ok()
        || stdout
        || sourcemap
        || !no_dts
        || target != "js"
        || super::relstar::may_depend_on_siblings(&source);
    let cache_dir = cache::cache_dir_for(path);
    let project_root = cache::project_root_for(path);
    let cache_key_value = cache::cache_key(&source, target);
    if !cache_disabled {
        // A3: the lookup re-validates the source content and confines the
        // record's output_path to the project root, so a hostile/out-of-tree
        // record is never trusted here.
        if let Some(rec) = cache::cache_lookup(&cache_dir, cache_key_value, &project_root, &source)
        {
            // The cached output is fresh. Determine where the user expects it
            // and copy if the dest is a genuinely different file from the
            // cached path.
            let want = output_path_arg.map(|s| s.to_string()).unwrap_or_else(|| {
                let stem = path.file_stem().unwrap().to_string_lossy();
                let parent = path.parent().unwrap_or(Path::new("."));
                parent
                    .join(format!("{}.js", stem))
                    .to_string_lossy()
                    .to_string()
            });
            // P6: a cache HIT returns before the main name validation below, so
            // validate the output name here too — otherwise a forbidden `-o`
            // name (CR/LF) would be written for the cached artifact.
            if let Some(base) = Path::new(&want).file_name() {
                reject_control_bearing_name("output filename", &base.to_string_lossy())?;
            }
            // Compare by canonical path, not string, so we never `fs::copy` a
            // file onto itself (which would truncate it).
            let same_file = std::fs::canonicalize(&rec.output_path)
                .ok()
                .and_then(|src| {
                    std::fs::canonicalize(Path::new(&want))
                        .ok()
                        .map(|dst| src == dst)
                })
                .unwrap_or(false);
            if !same_file {
                // #98: create missing output parent dirs (`-o dist/app.js`
                // with no dist/ previously died with a raw os error 3).
                ensure_parent_dir(Path::new(&want))?;
                // A8: route the cached copy through the ONE safe writer so the
                // destination is symlink-refused and a hand-written file is not
                // clobbered (the cached bytes already carry the marker).
                let cached_bytes = std::fs::read(&rec.output_path)?;
                safewrite::write_single(
                    Path::new(&want),
                    &cached_bytes,
                    OutputKind::MarkedText,
                    force,
                )?;
            }
            if verbosity != Verbosity::Quiet {
                output::success(&format!(
                    "Cache hit: {} (skipped recompile, copied to {})",
                    rec.output_path, want
                ));
            }
            return Ok(());
        }
    }

    let filename = path.file_name().unwrap().to_string_lossy();

    // Parse
    let t_parse = Instant::now();
    let mut module = pyths_parser::parse(&source).map_err(|errors| {
        let diags: Vec<_> = errors
            .iter()
            .map(|e| {
                let mut diag = pyths_diagnostic::errors::Diagnostic::error(
                    &e.message,
                    pyths_syntax::span::Span::new(e.span.start, e.span.end),
                );
                for note in &e.notes {
                    diag = diag.with_note(note);
                }
                diag
            })
            .collect();

        pyths_diagnostic::render::render_diagnostics(&source, &filename, &diags)
    })?;
    // Relative star imports (`from .mod import *`) expand into explicit named
    // imports HERE — the one CLI-owned pass (commands::relstar) — so codegen
    // never sees a relative star (its backstop is a hard error, never the old
    // silent drop).
    super::relstar::normalize_relative_imports(&mut module, path)?;
    let module = module;
    let d_parse = t_parse.elapsed();

    let line_count = source.lines().count();

    // public #3 GATE: run a certified codegen pass up front and FAIL the
    // compile on any codegen diagnostic (unimplemented builtins, rejected
    // imports, unsupported methods). Previously the emitter printed
    // "error: ..." to stderr but `pyths compile` still exited 0 and wrote a
    // throwing artifact — a known-unsupported builtin must be a compile
    // FAILURE, not a runtime surprise. One early gate covers every emission
    // branch below (sourcemap / plain / wasm-bridge); the messages
    // themselves are printed by the emitter as it records them.
    {
        let gate_opts = pyths_codegen_js::CodegenOptions {
            npm_imports: Some(&config.npm.imports),
            ..Default::default()
        };
        let gate = pyths_codegen_js::codegen_certified(&module, &gate_opts);
        if !gate.errors.is_empty() {
            return Err(format!(
                "{} compile error(s) — output not written",
                gate.errors.len()
            )
            .into());
        }
    }

    // Determine output path
    let output_path = if let Some(out) = output_path_arg {
        out.to_string()
    } else {
        let stem = path.file_stem().unwrap().to_string_lossy();
        let parent = path.parent().unwrap_or(Path::new("."));
        parent
            .join(format!("{}.js", stem))
            .to_string_lossy()
            .to_string()
    };

    // Derive output stem and directory from output_path (used for .wasm, .glue.js, etc.)
    let out_path = Path::new(&output_path);
    let out_stem = out_path.file_stem().unwrap().to_string_lossy().to_string();
    let out_dir = out_path.parent().unwrap_or(Path::new("."));

    // SEC #4 (CWE-94/116): the output stem and basename flow into a
    // `//# sourceMappingURL=` comment, a Wrangler TOML string, and import
    // specifiers. A control character (esp. CR/LF, from a source filename in a
    // hostile CI checkout) can break out of those single-line contexts. Reject
    // it before any generated syntax is produced. (TOML string values are also
    // escaped at their sink via `toml_basic_string`.)
    reject_control_bearing_name("output stem", &out_stem)?;
    if let Some(base) = out_path.file_name() {
        reject_control_bearing_name("output filename", &base.to_string_lossy())?;
    }

    // #98: `pyths compile app.ps -o dist/app.js` with no dist/ previously
    // failed with a raw `os error 3` from fs::write. Create the missing
    // parent directories up front (every subsequent write — .js, .wasm,
    // .glue.js, .map, .d.ts — lands inside out_dir).
    ensure_parent_dir(out_path)?;

    // Edge / server targets all require WASM compilation. Each implies a
    // different bridge variant + auxiliary file layout.
    let is_edge_target = matches!(target, "wasm-edge" | "wasi" | "deno");
    let needs_wasm = matches!(target, "wasm" | "js+wasm") || is_edge_target;
    let target_only_wasm = target == "wasm";

    // === WASM codegen (pure — nothing is written yet) ===
    let mut d_wasm = Duration::ZERO;
    let wasm_output: Option<pyths_codegen_wasm::WasmCodegenOutput> = if needs_wasm {
        let t_wasm = Instant::now();
        let wo = pyths_codegen_wasm::codegen_wasm(&module);
        d_wasm = t_wasm.elapsed();
        Some(wo)
    } else {
        None
    };

    // §4.7.3: strict `@wasm` assertion + target-conflict diagnostics. Checked
    // BEFORE any output handling so a violated assertion aborts the build with
    // nothing written (and takes precedence over the edge-target
    // "no eligible functions" error, whose message is less specific).
    enforce_wasm_placement(&module, &resolved, needs_wasm, wasm_output.as_ref())?;

    if let Some(ref wo) = wasm_output {
        if wo.wasm.is_empty() {
            if is_edge_target {
                return Err(format!(
                    "--target {} requires at least one WASM-eligible function in the source.",
                    target
                )
                .into());
            }
            // Graceful degrade (§4.7.1): under automatic routing a module
            // with no WASM-eligible functions still builds — it simply emits
            // a single plain `.js`. Only warn when the user PINNED a
            // WASM-bearing target; the auto default staying all-JS is the
            // expected quiet outcome, not a warning.
            if resolved.explicit && verbosity != Verbosity::Quiet {
                output::warning_summary("No WASM-eligible functions found");
            }
            if verbosity == Verbosity::Verbose {
                for (name, reason) in &wo.rejected_functions {
                    output::info(&format!("  Skipped {}: {}", name, reason), verbosity);
                }
            }
        }
    }
    let wasm_emitted = wasm_output
        .as_ref()
        .map(|w| !w.wasm.is_empty())
        .unwrap_or(false);

    // === P10 (round-4 root fix): declare the COMPLETE output graph and
    // preflight EVERY destination — existence, ownership, symlink, hard-link
    // count, dir-vs-file, and destination aliasing — BEFORE any write. A
    // refusal aborts here with NOTHING written, so a partial artifact set
    // (e.g. a fresh .wasm beside a refused .js) can no longer be produced.
    // Every write below goes through `plan.write`, which refuses any
    // destination that was not declared here (fail closed).
    let wasm_path_buf = out_dir.join(format!("{}.wasm", out_stem));
    let wasm_path = wasm_path_buf.to_string_lossy().to_string();
    let wasm_cert_path = format!("{}.wasm.cert.json", output_path);
    let glue_path = out_dir.join(format!("{}.glue.js", out_stem));
    let map_path = format!("{}.map", output_path);
    let js_cert_path = format!("{}.cert.json", output_path);
    let dts_path = dts_path_for(&output_path);
    let scaffold_names: &[&str] = if is_edge_target {
        match target {
            "wasm-edge" => &["wrangler.toml", "package.json"],
            "deno" => &["deno.json"],
            _ => &[],
        }
    } else {
        &[]
    };
    let emits_js = !target_only_wasm && !is_edge_target && !stdout;
    // Default-on `.d.ts` (release_v0.2.2 §5): declarations emit whenever a
    // `.js` module FILE is written (never for --stdout / wasm-only / edge
    // scaffolds). `--no-dts` is the escape hatch; `--dts` is a redundant
    // explicit-on kept for compatibility.
    let emit_dts = !no_dts && emits_js;

    let mut planned: Vec<PlannedOutput> = Vec::new();
    if wasm_emitted {
        planned.push(PlannedOutput::wasm(&wasm_path_buf));
        if emit_cert {
            // The certificate is a marker-less JSON sidecar of the `.wasm`.
            planned.push(PlannedOutput::sidecar(
                Path::new(&wasm_cert_path),
                &wasm_path_buf,
            ));
        }
        if target == "js+wasm" {
            planned.push(PlannedOutput::marked_text(&glue_path));
        }
        if is_edge_target {
            planned.push(PlannedOutput::marked_text(Path::new(&output_path)));
            for name in scaffold_names {
                planned.push(PlannedOutput::scaffold(out_dir.join(name)));
            }
        }
    }
    if emits_js {
        planned.push(PlannedOutput::marked_text(Path::new(&output_path)));
        if sourcemap {
            // The `.js.map` is a marker-less sidecar of the `.js`: overwritable
            // only when itself absent or when the EXISTING `.js` is owned — a
            // fresh (absent) `.js` authorizes nothing.
            planned.push(PlannedOutput::sidecar(
                Path::new(&map_path),
                Path::new(&output_path),
            ));
        }
        if emit_cert {
            planned.push(PlannedOutput::sidecar(
                Path::new(&js_cert_path),
                Path::new(&output_path),
            ));
        }
        if emit_dts {
            planned.push(PlannedOutput::marked_text(Path::new(&dts_path)));
        }
    }
    let plan = OutputPlan::preflight(&planned, force)?;

    // === WASM write phase ===
    if let Some(ref wasm_out) = wasm_output {
        if wasm_emitted {
            let t_write_wasm = Instant::now();
            // The `.wasm` carries its ownership as a WASM custom section, so a
            // later compile recognizes its own module (rebuild without --force)
            // while a foreign `.wasm` is refused at preflight.
            let mut wasm_bytes = wasm_out.wasm.clone();
            pyths_codegen_wasm::optimize::append_generated_marker(&mut wasm_bytes);
            plan.write(&wasm_path_buf, &wasm_bytes)?;
            let d_write_wasm = t_write_wasm.elapsed();

            // Credible compilation: WASM auto-routing ADMISSION certificate.
            // Independently re-derives the FULL admission verdict (boundary +
            // body-fragment membership) from the module and cross-checks the
            // emitted `.wasm` (every admitted boundary type is WASM-representable
            // + lowerable; exports match the admitted set). It does NOT certify
            // body VALUE-equivalence (the CRIT-A6 classes) — that stays with the
            // differential + Livermore layers; see cert.rs's module header. A
            // rejected certificate fails THIS compilation.
            if emit_cert {
                let cert = pyths_codegen_wasm::cert::build_certificate(&module);
                let violations =
                    pyths_codegen_wasm::cert::check_certificate(&cert, &wasm_out.wasm, &module);
                if !violations.is_empty() {
                    for v in &violations {
                        eprintln!("error: WASM-admission certificate violation: {}", v);
                    }
                    return Err(format!(
                            "WASM-admission certificate REJECTED ({} violation(s)) — compilation discarded",
                            violations.len()
                        )
                        .into());
                }
                plan.write(Path::new(&wasm_cert_path), cert.to_json().as_bytes())?;
                if verbosity != Verbosity::Quiet {
                    output::success(&format!(
                        "Certificate ACCEPTED → {} (WASM-admission)",
                        wasm_cert_path
                    ));
                }
            }

            if verbosity != Verbosity::Quiet {
                output::success(&format!(
                    "Compiled {} → {} ({} functions)",
                    file,
                    wasm_path,
                    wasm_out.compiled_functions.len()
                ));
            }

            if verbosity == Verbosity::Verbose {
                for name in &wasm_out.compiled_functions {
                    output::info(&format!("  WASM: {}", name), verbosity);
                }
                for (name, reason) in &wasm_out.rejected_functions {
                    output::info(&format!("  Skipped {}: {}", name, reason), verbosity);
                }
            }

            if show_timings {
                eprintln!("  WASM Codegen: {:.2}ms", d_wasm.as_secs_f64() * 1000.0);
                eprintln!(
                    "  WASM Write:   {:.2}ms",
                    d_write_wasm.as_secs_f64() * 1000.0
                );
            }

            // Generate glue.js for js+wasm mode (browser target)
            if target == "js+wasm" {
                // #358: compile the eligible functions ONCE MORE through the
                // plain JS backend — the exact, arbitrary-precision twins.
                // The glue embeds them and re-runs any call whose WASM
                // result would exceed i64 (`__ovf` flag / boundary guard),
                // so the fast path can never return a silently wrapped int.
                let twin_js: Option<String> = {
                    let twin_body: Vec<pyths_syntax::ast::Stmt> = module
                        .body
                        .iter()
                        .filter(|s| {
                            matches!(
                                &s.kind,
                                pyths_syntax::ast::StmtKind::FuncDef { name, .. }
                                    if wasm_out.compiled_functions.contains(name)
                            )
                        })
                        .cloned()
                        .collect();
                    if twin_body.is_empty() {
                        None
                    } else {
                        let twin_module = pyths_syntax::ast::Module {
                            body: twin_body,
                            span: module.span,
                        };
                        let twin_opts = pyths_codegen_js::CodegenOptions {
                            npm_imports: Some(&config.npm.imports),
                            ..Default::default()
                        };
                        Some(pyths_codegen_js::codegen_with_options(
                            &twin_module,
                            &twin_opts,
                        ))
                    }
                };
                let glue_js = pyths_codegen_wasm::generate_bridge_js(
                    wasm_out,
                    &format!("./{}.wasm", out_stem),
                    twin_js.as_deref(),
                );
                // SEC #10: glue is text — prepend the marker so it is ownership-
                // gated (refuses an unowned same-name file; rebuilds cleanly).
                let glue_marked = format!("{}{}", generated_header(), glue_js);
                plan.write(&glue_path, glue_marked.as_bytes())?;
                if verbosity != Verbosity::Quiet {
                    output::success(&format!("Generated {}", glue_path.to_string_lossy()));
                }
            }

            // Edge / server targets — write self-contained module + scaffold
            if is_edge_target {
                use pyths_codegen_wasm::bridge::{append_workers_fetch_handler, BridgeTarget};
                let bridge_target = match target {
                    "wasm-edge" => BridgeTarget::CloudflareWorkers,
                    "wasi" => BridgeTarget::Wasi,
                    "deno" => BridgeTarget::Deno,
                    _ => unreachable!(),
                };
                let mut glue_js = pyths_codegen_wasm::generate_bridge_for_target(
                    bridge_target,
                    &format!("./{}.wasm", out_stem),
                    wasm_out,
                );

                // Pick scaffold files (configs) per target; entry filename
                // follows the user's -o (or compile.rs's default). The NAMES
                // must stay in sync with `scaffold_names` above so every
                // scaffold destination was preflighted.
                let scaffold_files: Vec<(String, String)> = match target {
                    "wasm-edge" => {
                        append_workers_fetch_handler(&mut glue_js, &wasm_out.export_info);
                        let entry_basename = out_path
                            .file_name()
                            .map(|s| s.to_string_lossy().to_string())
                            .unwrap_or_else(|| "worker.js".to_string());
                        // SEC #4: serialize the source-derived name and entry
                        // through a TOML basic-string escaper so a `"`, `\`, or
                        // control char cannot break out of the config grammar.
                        let wrangler = format!(
                            "name = {}\nmain = {}\ncompatibility_date = \"2024-09-25\"\n",
                            toml_basic_string(&out_stem),
                            toml_basic_string(&format!("./{}", entry_basename)),
                        );
                        let pkg = "{\n  \"private\": true,\n  \"type\": \"module\",\n  \"devDependencies\": {\n    \"wrangler\": \"^3\"\n  }\n}\n".to_string();
                        vec![
                            ("wrangler.toml".to_string(), wrangler),
                            ("package.json".to_string(), pkg),
                        ]
                    }
                    "wasi" => vec![],
                    "deno" => {
                        let entry_basename = out_path
                            .file_name()
                            .map(|s| s.to_string_lossy().to_string())
                            .unwrap_or_else(|| "main.ts".to_string());
                        // A16 hardening: build the JSON via serde_json (see
                        // `build_deno_json`) so a `"` or space in the output
                        // name cannot break the JSON string.
                        let deno_json = build_deno_json(&entry_basename);
                        vec![("deno.json".to_string(), deno_json)]
                    }
                    _ => unreachable!(),
                };

                // Write the entry file at the user-specified output_path.
                // SEC #10: text entry — prepend the marker so it is ownership-
                // gated like the ordinary `.js` output.
                let entry_marked = format!("{}{}", generated_header(), glue_js);
                plan.write(Path::new(&output_path), entry_marked.as_bytes())?;
                if verbosity != Verbosity::Quiet {
                    output::success(&format!("Generated {}", output_path));
                }

                for (name, contents) in &scaffold_files {
                    let p = out_dir.join(name);
                    // SEC #12: keep the user's existing config; a symlink —
                    // including a DANGLING one — was already refused by the
                    // preflight, and creation is exclusive.
                    if plan.write(&p, contents.as_bytes())? && verbosity != Verbosity::Quiet {
                        output::success(&format!("Generated {}", p.to_string_lossy()));
                    }
                }
            }

            // Run wasm-opt -Os (size-optimized for web) if available. The
            // binary is resolved through the ONE exe resolver
            // (`procutil::resolve_program` — PATH only, never the cwd;
            // `PYTHS_WASM_OPT` overrides), and the optimizer runs inside a
            // private tempfile::TempDir (see optimize.rs).
            match super::procutil::resolve_program("wasm-opt", Some("PYTHS_WASM_OPT")) {
                None => {
                    if verbosity == Verbosity::Verbose {
                        output::info(
                                "  wasm-opt not found on PATH — install with `npm i -g wasm-opt` for size reduction",
                                verbosity,
                            );
                    }
                }
                Some(tool) => match pyths_codegen_wasm::optimize::run_wasm_opt_at(
                    &tool,
                    &wasm_path,
                    pyths_codegen_wasm::optimize::OptLevel::Size,
                ) {
                    Ok(Some(r)) => {
                        // Round-5: the optimized module is PLACED through the
                        // same fd-verified safe writer as the original write
                        // (preflighted destination + ownership proven on the
                        // fd) — never a raw rename over the target.
                        plan.rewrite(&wasm_path_buf, &r.optimized)?;
                        if verbosity != Verbosity::Quiet {
                            let pct = (1.0 - r.size_after as f64 / r.size_before as f64) * 100.0;
                            output::info(
                                &format!(
                                    "  wasm-opt -{}: {} → {} bytes ({:.0}% reduction)",
                                    r.level, r.size_before, r.size_after, pct
                                ),
                                verbosity,
                            );
                        }
                    }
                    Ok(None) => {
                        if verbosity == Verbosity::Verbose {
                            output::info(
                                "  wasm-opt failed its version probe — skipped",
                                verbosity,
                            );
                        }
                    }
                    Err(e) => {
                        if verbosity != Verbosity::Quiet {
                            output::info(&format!("  wasm-opt error: {}", e), verbosity);
                        }
                    }
                },
            }
        }

        // If target is "wasm" only or any edge target, we're done after WASM
        if target_only_wasm || is_edge_target {
            if show_timings {
                let d_total = d_read + d_expand + d_parse + d_wasm;
                eprintln!("  Read:    {:.2}ms", d_read.as_secs_f64() * 1000.0);
                if d_expand.as_micros() > 0 {
                    eprintln!("  Expand:  {:.2}ms", d_expand.as_secs_f64() * 1000.0);
                }
                eprintln!("  Parse:   {:.2}ms", d_parse.as_secs_f64() * 1000.0);
                eprintln!(
                    "  Total:   {:.2}ms ({} lines, {:.1} KB)",
                    d_total.as_secs_f64() * 1000.0,
                    line_count,
                    source_len as f64 / 1024.0,
                );
            }
            return Ok(());
        }
    }

    // === JS compilation ===
    if sourcemap && !stdout {
        // Compile with source map, through the SAME CodegenOptions set as the
        // non-sourcemap branch below. This closes the former auto+kernel gap:
        // `--sourcemap` used to ignore wasm_skip/glue (and react_refresh), so
        // a kernel module's mapped `.js` re-implemented the kernels in JS
        // instead of re-exporting the WASM glue like the unmapped `.js` did.
        let t_codegen = Instant::now();
        let glue_filename;
        let mut opts = pyths_codegen_js::CodegenOptions {
            npm_imports: Some(&config.npm.imports),
            react_refresh,
            ..Default::default()
        };
        if target == "worker" {
            opts.worker_runtime = true;
        }
        if let Some(ref wo) = wasm_output {
            if !wo.compiled_functions.is_empty() {
                glue_filename = format!("./{}.glue.js", out_stem);
                opts.wasm_skip = Some(&wo.compiled_functions);
                opts.wasm_glue_filename = Some(&glue_filename);
            }
        }
        let result = pyths_codegen_js::codegen_with_sourcemap_full(
            &module,
            &source,
            &filename,
            &output_path,
            &opts,
            no_sources_content,
        );
        let d_codegen = t_codegen.elapsed();

        if let Some(err) = codegen_depth_error(&source, &filename) {
            return Err(err);
        }

        // A8: prepend the generated marker so a later compile recognizes its
        // own output. The marker line sits BEFORE the mapped code, so we also
        // pad the source map with one leading `;` (below) to keep line numbers
        // aligned. Placing it here (not appended) keeps `//# sourceMappingURL`
        // last.
        let header = generated_header();
        let js_with_ref = format!(
            "{}{}\n//# sourceMappingURL={}\n",
            header,
            result.js,
            Path::new(&map_path).file_name().unwrap().to_string_lossy()
        );

        let t_write = Instant::now();
        // Both destinations were preflighted above (P10): a refusal on either
        // aborted before ANY write, so a fresh `.js` can never sit beside a
        // stale or destroyed `.map`.
        plan.write(Path::new(&output_path), js_with_ref.as_bytes())?;
        if let Some(map) = result.source_map {
            // The marker added one line ahead of all generated code; shift the
            // VLQ mappings down by one line so frames still resolve correctly.
            let shifted = shift_sourcemap_one_line(&map);
            plan.write(Path::new(&map_path), shifted.as_bytes())?;
        }
        let d_write = t_write.elapsed();

        if verbosity != Verbosity::Quiet {
            output::success(&format!(
                "Compiled {} → {} (+ {})",
                file, output_path, map_path
            ));
        }

        if show_timings {
            let d_total = d_read + d_expand + d_parse + d_codegen + d_write;
            eprintln!("  Read:    {:.2}ms", d_read.as_secs_f64() * 1000.0);
            if d_expand.as_micros() > 0 {
                eprintln!("  Expand:  {:.2}ms", d_expand.as_secs_f64() * 1000.0);
            }
            eprintln!("  Parse:   {:.2}ms", d_parse.as_secs_f64() * 1000.0);
            eprintln!("  Codegen: {:.2}ms", d_codegen.as_secs_f64() * 1000.0);
            eprintln!("  Write:   {:.2}ms", d_write.as_secs_f64() * 1000.0);
            eprintln!(
                "  Total:   {:.2}ms ({} lines, {:.1} KB)",
                d_total.as_secs_f64() * 1000.0,
                line_count,
                source_len as f64 / 1024.0,
            );
        }
    } else {
        // Compile without source map
        let t_codegen = Instant::now();

        // For js+wasm with compiled functions, use bridge-aware codegen.
        // The `CodegenOptions` builder routes React-Refresh emission +
        // npm overrides + WASM bridge through one path so we don't have
        // to fan out to specific helper variants.
        let glue_filename;
        let mut opts = pyths_codegen_js::CodegenOptions {
            npm_imports: Some(&config.npm.imports),
            react_refresh,
            ..Default::default()
        };
        // --target worker: emit DOM-free runtime imports from pyths-runtime/core
        // instead of pyths-runtime. Only the import path changes; all other
        // codegen is identical. (B-030 follow-up D)
        if target == "worker" {
            opts.worker_runtime = true;
        }
        if let Some(ref wo) = wasm_output {
            if !wo.compiled_functions.is_empty() {
                glue_filename = format!("./{}.glue.js", out_stem);
                opts.wasm_skip = Some(&wo.compiled_functions);
                opts.wasm_glue_filename = Some(&glue_filename);
            }
        }
        // Credible compilation (§7.2): certified codegen returns the
        // subscript-routing certificate; the checker re-applies the
        // rules to every recorded site and cross-checks the emitted JS.
        // A rejected certificate fails THIS compilation (Axon-style
        // translation validation — validate the run, not the pass).
        let mut cert_json: Option<String> = None;
        let js = if emit_cert {
            let certified = pyths_codegen_js::codegen_certified(&module, &opts);
            let violations =
                pyths_codegen_js::cert::check_certificate(&certified.certificate, &certified.js);
            if !violations.is_empty() {
                for v in &violations {
                    eprintln!("error: certificate violation: {}", v);
                }
                return Err(format!(
                    "subscript-routing certificate REJECTED ({} violation(s)) — compilation discarded",
                    violations.len()
                )
                .into());
            }
            cert_json = Some(certified.certificate.to_json());
            certified.js
        } else {
            pyths_codegen_js::codegen_with_options(&module, &opts)
        };

        let d_codegen = t_codegen.elapsed();

        if let Some(err) = codegen_depth_error(&source, &filename) {
            return Err(err);
        }

        if stdout {
            print!("{}", js);

            if show_timings {
                let d_total = d_read + d_expand + d_parse + d_codegen;
                eprintln!("  Read:    {:.2}ms", d_read.as_secs_f64() * 1000.0);
                if d_expand.as_micros() > 0 {
                    eprintln!("  Expand:  {:.2}ms", d_expand.as_secs_f64() * 1000.0);
                }
                eprintln!("  Parse:   {:.2}ms", d_parse.as_secs_f64() * 1000.0);
                eprintln!("  Codegen: {:.2}ms", d_codegen.as_secs_f64() * 1000.0);
                eprintln!(
                    "  Total:   {:.2}ms ({} lines, {:.1} KB)",
                    d_total.as_secs_f64() * 1000.0,
                    line_count,
                    source_len as f64 / 1024.0,
                );
            }
            return Ok(());
        }

        // A8: prepend the generated marker so a later compile recognizes its
        // own output and a hand-written `.js` is protected.
        let final_js = format!("{}{}", generated_header(), js);

        let t_write = Instant::now();
        // Both the `.js` and its `.cert.json` were preflighted above (P10), so
        // a certificate refusal cannot leave a new `.js` beside a stale cert.
        plan.write(Path::new(&output_path), final_js.as_bytes())?;
        if let Some(json) = &cert_json {
            plan.write(Path::new(&js_cert_path), json.as_bytes())?;
            if verbosity != Verbosity::Quiet {
                output::success(&format!(
                    "Certificate ACCEPTED → {} (subscript-routing)",
                    js_cert_path
                ));
            }
        }
        let d_write = t_write.elapsed();

        // Step 9: write a cache record so subsequent compiles can skip codegen.
        if !cache_disabled {
            let record = cache::CacheRecord {
                key: cache_key_value,
                source: file.to_string(),
                source_hash: cache::fnv1a64(source.as_bytes()),
                target: target.to_string(),
                output_path: output_path.clone(),
                // Hash the ACTUAL bytes on disk (marker included) so the
                // lookup's output re-validation matches.
                output_hash: cache::fnv1a64(final_js.as_bytes()),
                timestamp: cache::current_timestamp(),
            };
            // B15: cache-store failure must not break the compile, but a silent
            // swallow means a persistent permissions problem degrades every
            // build to a full recompile with no signal. Warn once.
            if let Err(e) = cache::cache_store(&cache_dir, &record) {
                if verbosity != Verbosity::Quiet {
                    eprintln!(
                        "warning: could not write compile cache ({}): {} — \
                         builds will not be cached (recompiling in full each time).",
                        cache_dir.display(),
                        e
                    );
                }
            }
        }

        if verbosity != Verbosity::Quiet {
            output::success(&format!("Compiled {} → {}", file, output_path));
        }

        if show_timings {
            let d_total = d_read + d_expand + d_parse + d_codegen + d_write;
            eprintln!("  Read:    {:.2}ms", d_read.as_secs_f64() * 1000.0);
            if d_expand.as_micros() > 0 {
                eprintln!("  Expand:  {:.2}ms", d_expand.as_secs_f64() * 1000.0);
            }
            eprintln!("  Parse:   {:.2}ms", d_parse.as_secs_f64() * 1000.0);
            eprintln!("  Codegen: {:.2}ms", d_codegen.as_secs_f64() * 1000.0);
            eprintln!("  Write:   {:.2}ms", d_write.as_secs_f64() * 1000.0);
            eprintln!(
                "  Total:   {:.2}ms ({} lines, {:.1} KB)",
                d_total.as_secs_f64() * 1000.0,
                line_count,
                source_len as f64 / 1024.0,
            );
        }
    }

    // Emit .d.ts (default-on for file builds; --no-dts suppresses)
    if emit_dts {
        let dts_content = format!(
            "{}{}",
            generated_header(),
            pyths_codegen_js::codegen_dts(&module)
        );
        // A8: symlink-refusal + marker-gated overwrite (a `.d.ts` can carry a
        // `//` comment marker). Preflighted as part of the output graph.
        plan.write(Path::new(&dts_path), dts_content.as_bytes())?;
        if verbosity != Verbosity::Quiet {
            output::success(&format!("Emitted {}", dts_path));
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── §4.7.1 — consolidated target resolution ────────────────────────────
    #[test]
    fn resolve_target_auto_default_is_js_wasm_semantics() {
        let rt = resolve_target(None, false).unwrap();
        assert_eq!(rt.name, "js+wasm");
        assert!(!rt.explicit);
    }

    #[test]
    fn resolve_target_auto_with_stdout_stays_single_js() {
        // stdout mode must not spray .wasm/.glue.js sidecars.
        let rt = resolve_target(None, true).unwrap();
        assert_eq!(rt.name, "js");
        assert!(!rt.explicit);
    }

    #[test]
    fn resolve_target_explicit_passes_through_and_is_marked_explicit() {
        for t in ["js", "worker", "wasm", "js+wasm", "wasm-edge", "wasi", "deno"] {
            let rt = resolve_target(Some(t), false).unwrap();
            assert_eq!(rt.name, t);
            assert!(rt.explicit);
        }
        // Explicit js+wasm is honored even under stdout (user pinned it).
        let rt = resolve_target(Some("js+wasm"), true).unwrap();
        assert_eq!(rt.name, "js+wasm");
    }

    #[test]
    fn resolve_target_rejects_unknown() {
        let err = resolve_target(Some("cobol"), false).unwrap_err().to_string();
        assert!(err.contains("unknown --target"), "got: {}", err);
    }

    #[test]
    fn dts_path_replaces_only_the_final_extension() {
        // Round-5: directory components must never be rewritten.
        assert_eq!(dts_path_for("app.js"), "app.d.ts");
        assert_eq!(
            Path::new(&dts_path_for("build.js/app.js")),
            Path::new("build.js/app.d.ts")
        );
        assert_eq!(
            Path::new(&dts_path_for("dist/app.js")),
            Path::new("dist/app.d.ts")
        );
        // No extension: the .d.ts is appended, not spliced.
        assert_eq!(dts_path_for("app"), "app.d.ts");
    }

    #[test]
    fn sourcemap_shift_prepends_one_semicolon() {
        let map = r#"{"version":3,"sources":["a.ps"],"mappings":"AAAA,CAAC"}"#;
        let shifted = shift_sourcemap_one_line(map);
        assert!(
            shifted.contains(r#""mappings":";AAAA,CAAC""#),
            "got: {}",
            shifted
        );
    }

    #[test]
    fn deno_json_is_valid_for_normal_name() {
        let json = build_deno_json("main.ts");
        let v: serde_json::Value = serde_json::from_str(&json).expect("valid JSON");
        assert_eq!(v["tasks"]["start"], "deno run --allow-read main.ts");
    }

    #[test]
    fn deno_json_escapes_space_and_quote_in_name() {
        // A16: an output name with a space or a `"` must still yield valid JSON
        // (previously a raw format! would break the string / splice the task).
        for name in ["my app.ts", "we\"ird.ts", "a\\b.ts"] {
            let json = build_deno_json(name);
            let v: serde_json::Value = serde_json::from_str(&json)
                .unwrap_or_else(|e| panic!("invalid JSON for {name:?}: {e}\n{json}"));
            assert_eq!(
                v["tasks"]["start"],
                serde_json::Value::String(format!("deno run --allow-read {name}")),
                "task value round-trips for {name:?}"
            );
        }
    }

    // ── SEC #4 — source-derived name serialization ─────────────────────────
    #[test]
    fn reject_control_bearing_name_catches_crlf_nul() {
        // U+2028/U+2029 are JS line terminators — they break out of a generated
        // `// …` comment just like a newline, but are NOT `char::is_control`.
        for bad in ["a\nb", "a\rb", "a\0b", "x\u{7f}y", "a\u{2028}b", "a\u{2029}b"] {
            assert!(
                reject_control_bearing_name("output stem", bad).is_err(),
                "{bad:?} should be rejected"
            );
        }
        for ok in ["app", "my-file.v2", "über"] {
            assert!(
                reject_control_bearing_name("output stem", ok).is_ok(),
                "{ok:?} should be accepted"
            );
        }
    }

    #[test]
    fn toml_basic_string_escapes_injection() {
        assert_eq!(toml_basic_string("worker"), "\"worker\"");
        assert_eq!(toml_basic_string("a\"b"), "\"a\\\"b\"");
        assert_eq!(toml_basic_string("a\\b"), "\"a\\\\b\"");
        assert_eq!(toml_basic_string("a\nb"), "\"a\\nb\"");
        // A control char is \u-escaped, so it cannot terminate the string.
        assert!(!toml_basic_string("a\rb").contains('\r'));
    }
}
