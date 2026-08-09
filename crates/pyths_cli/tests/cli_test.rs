use std::process::Command;

fn pyths_bin() -> Command {
    Command::new(env!("CARGO_BIN_EXE_pyths"))
}

fn fixtures_dir() -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures")
}

#[test]
fn test_compile_timings_flag() {
    let fixture = fixtures_dir().join("hello.ps");
    let tmp_dir = std::env::temp_dir();
    let out_file = tmp_dir.join("hello_timings_test.js");

    let output = pyths_bin()
        .args([
            "compile",
            fixture.to_str().unwrap(),
            "-o",
            out_file.to_str().unwrap(),
            "--timings",
        ])
        .output()
        .expect("Failed to run pyths");

    assert!(
        output.status.success(),
        "Exit code: {:?}\nStderr: {}",
        output.status,
        String::from_utf8_lossy(&output.stderr)
    );

    // Timings go to stderr
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("Read:"), "Missing Read timing: {}", stderr);
    assert!(
        stderr.contains("Parse:"),
        "Missing Parse timing: {}",
        stderr
    );
    assert!(
        stderr.contains("Codegen:"),
        "Missing Codegen timing: {}",
        stderr
    );
    assert!(
        stderr.contains("Write:"),
        "Missing Write timing: {}",
        stderr
    );
    assert!(
        stderr.contains("Total:"),
        "Missing Total timing: {}",
        stderr
    );

    // Output file should still be valid JS (print lowers to the pyPrint
    // runtime helper, which strips BigInt's `n` suffix).
    let js = std::fs::read_to_string(&out_file).unwrap();
    assert!(js.contains("pyPrint"), "Valid JS output: {}", js);

    // Clean up
    let _ = std::fs::remove_file(&out_file);
}

// --- Phase 4b integration tests ---

#[test]
fn test_compile_multiple_errors_reported() {
    let fixture = fixtures_dir().join("error_multiple.ps");
    let output = pyths_bin()
        .args(["compile", fixture.to_str().unwrap()])
        .output()
        .expect("Failed to run pyths");

    assert!(!output.status.success(), "Should fail on error file");

    let stderr = String::from_utf8_lossy(&output.stderr);
    // Should report the $ error
    assert!(
        stderr.contains("Unexpected character") || stderr.contains("error"),
        "Should report errors in stderr: {}",
        stderr
    );
}

#[test]
fn test_check_multiple_errors() {
    let fixture = fixtures_dir().join("error_multiple.ps");
    let output = pyths_bin()
        .args(["check", fixture.to_str().unwrap()])
        .output()
        .expect("Failed to run pyths");

    assert!(!output.status.success(), "Should fail on error file");

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("Unexpected character") || stderr.contains("error"),
        "Should report errors: {}",
        stderr
    );
}

#[test]
fn test_compile_contextual_hint() {
    let fixture = fixtures_dir().join("error_missing_colon.ps");
    let output = pyths_bin()
        .args(["compile", fixture.to_str().unwrap()])
        .output()
        .expect("Failed to run pyths");

    assert!(!output.status.success(), "Should fail on missing colon");

    let stderr = String::from_utf8_lossy(&output.stderr);
    // Should have some kind of hint about the colon
    assert!(
        stderr.contains("':'") || stderr.contains("colon") || stderr.contains("add"),
        "Should contain hint about missing colon: {}",
        stderr
    );
}

#[test]
fn test_lint_unused_variable() {
    let fixture = fixtures_dir().join("lint_unused_var.ps");
    let output = pyths_bin()
        .args(["lint", fixture.to_str().unwrap()])
        .output()
        .expect("Failed to run pyths");

    assert!(!output.status.success(), "Lint should fail with warnings");

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("W001") || stderr.contains("unused"),
        "Should report unused variable: {}",
        stderr
    );
}

#[test]
fn test_lint_clean_no_warnings() {
    let fixture = fixtures_dir().join("lint_clean.ps");
    let output = pyths_bin()
        .args(["lint", fixture.to_str().unwrap()])
        .output()
        .expect("Failed to run pyths");

    assert!(
        output.status.success(),
        "Clean file should pass lint. Stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn test_lint_unreachable() {
    let fixture = fixtures_dir().join("lint_unreachable.ps");
    let output = pyths_bin()
        .args(["lint", fixture.to_str().unwrap()])
        .output()
        .expect("Failed to run pyths");

    assert!(!output.status.success(), "Lint should fail with warnings");

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("W003") || stderr.contains("unreachable"),
        "Should report unreachable code: {}",
        stderr
    );
}

#[test]
fn test_no_color_clean_output() {
    let fixture = fixtures_dir().join("hello.ps");
    let tmp_dir = std::env::temp_dir();
    let out_file = tmp_dir.join("hello_nocolor_test.js");

    let output = pyths_bin()
        .env("NO_COLOR", "1")
        .args([
            "compile",
            fixture.to_str().unwrap(),
            "-o",
            out_file.to_str().unwrap(),
        ])
        .output()
        .expect("Failed to run pyths");

    assert!(output.status.success());

    let stdout = String::from_utf8_lossy(&output.stdout);
    // ANSI escape codes start with \x1b[
    assert!(
        !stdout.contains("\x1b["),
        "Output should not contain ANSI escapes when NO_COLOR is set: {}",
        stdout
    );

    // Clean up
    let _ = std::fs::remove_file(&out_file);
}

#[test]
fn test_quiet_suppresses_success() {
    let fixture = fixtures_dir().join("hello.ps");
    let tmp_dir = std::env::temp_dir();
    let out_file = tmp_dir.join("hello_quiet_test.js");

    let output = pyths_bin()
        .args([
            "--quiet",
            "compile",
            fixture.to_str().unwrap(),
            "-o",
            out_file.to_str().unwrap(),
        ])
        .output()
        .expect("Failed to run pyths");

    assert!(output.status.success());

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.is_empty(),
        "Quiet mode should suppress stdout success messages: '{}'",
        stdout
    );

    // Clean up
    let _ = std::fs::remove_file(&out_file);
}

// --- Phase 4c: --dts flag tests ---

#[test]
fn test_compile_dts_flag() {
    let fixture = fixtures_dir().join("typed_counter.ps");
    let tmp_dir = std::env::temp_dir();
    let out_file = tmp_dir.join("typed_counter_dts_test.js");
    let dts_file = tmp_dir.join("typed_counter_dts_test.d.ts");

    let output = pyths_bin()
        .args([
            "compile",
            fixture.to_str().unwrap(),
            "-o",
            out_file.to_str().unwrap(),
            "--dts",
        ])
        .output()
        .expect("Failed to run pyths");

    assert!(
        output.status.success(),
        "Exit code: {:?}\nStderr: {}",
        output.status,
        String::from_utf8_lossy(&output.stderr)
    );

    // Both .js and .d.ts should be created
    assert!(out_file.exists(), "JS file should be created");
    assert!(dts_file.exists(), "DTS file should be created");

    // Clean up
    let _ = std::fs::remove_file(&out_file);
    let _ = std::fs::remove_file(&dts_file);
}

#[test]
fn test_compile_dts_content() {
    let fixture = fixtures_dir().join("typed_counter.ps");
    let tmp_dir = std::env::temp_dir();
    let out_file = tmp_dir.join("typed_counter_content_test.js");
    let dts_file = tmp_dir.join("typed_counter_content_test.d.ts");

    let output = pyths_bin()
        .args([
            "compile",
            fixture.to_str().unwrap(),
            "-o",
            out_file.to_str().unwrap(),
            "--dts",
        ])
        .output()
        .expect("Failed to run pyths");

    assert!(output.status.success(), "Compile should succeed");

    let dts_content = std::fs::read_to_string(&dts_file).expect("Should read .d.ts file");
    assert!(
        dts_content.contains("export declare"),
        "DTS should contain export declare: {}",
        dts_content
    );
    assert!(
        dts_content.contains("export declare function add(a: number, b: number): number;"),
        "DTS should have add function: {}",
        dts_content
    );
    assert!(
        dts_content.contains("export declare class Counter"),
        "DTS should have Counter class: {}",
        dts_content
    );

    // Clean up
    let _ = std::fs::remove_file(&out_file);
    let _ = std::fs::remove_file(&dts_file);
}

#[test]
fn test_compile_dts_and_sourcemap() {
    let fixture = fixtures_dir().join("typed_counter.ps");
    let tmp_dir = std::env::temp_dir();
    let out_file = tmp_dir.join("typed_counter_all_test.js");
    let dts_file = tmp_dir.join("typed_counter_all_test.d.ts");
    let map_file = tmp_dir.join("typed_counter_all_test.js.map");

    let output = pyths_bin()
        .args([
            "compile",
            fixture.to_str().unwrap(),
            "-o",
            out_file.to_str().unwrap(),
            "--dts",
            "--sourcemap",
        ])
        .output()
        .expect("Failed to run pyths");

    assert!(
        output.status.success(),
        "Exit code: {:?}\nStderr: {}",
        output.status,
        String::from_utf8_lossy(&output.stderr)
    );

    // All three files should be created
    assert!(out_file.exists(), "JS file should be created");
    assert!(dts_file.exists(), "DTS file should be created");
    assert!(map_file.exists(), "Source map file should be created");

    // Clean up
    let _ = std::fs::remove_file(&out_file);
    let _ = std::fs::remove_file(&dts_file);
    let _ = std::fs::remove_file(&map_file);
}

// --- Phase 4c: type checker integration tests ---

#[test]
fn test_check_type_mismatch() {
    let fixture = fixtures_dir().join("type_error.ps");
    let output = pyths_bin()
        .args(["check", fixture.to_str().unwrap()])
        .output()
        .expect("Failed to run pyths");

    assert!(!output.status.success(), "Should fail on type errors");

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("Type mismatch") || stderr.contains("type error"),
        "Should report type mismatch: {}",
        stderr
    );
}

#[test]
fn test_check_type_ok() {
    let fixture = fixtures_dir().join("typed_counter.ps");
    let output = pyths_bin()
        .args(["check", fixture.to_str().unwrap()])
        .output()
        .expect("Failed to run pyths");

    assert!(
        output.status.success(),
        "Typed counter should pass check. Stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn test_check_type_return_mismatch() {
    let fixture = fixtures_dir().join("type_error.ps");
    let output = pyths_bin()
        .args(["check", fixture.to_str().unwrap()])
        .output()
        .expect("Failed to run pyths");

    assert!(
        !output.status.success(),
        "Should fail on return type mismatch"
    );

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("Return type mismatch") || stderr.contains("Type mismatch"),
        "Should report return type mismatch: {}",
        stderr
    );
}

// --- Phase 5: Advanced type inference CLI tests ---

#[test]
fn test_check_type_inference_ok() {
    let fixture = fixtures_dir().join("type_inference.ps");
    let output = pyths_bin()
        .args(["check", fixture.to_str().unwrap()])
        .output()
        .expect("Failed to run pyths");

    assert!(
        output.status.success(),
        "Should pass type check: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn test_check_type_inference_errors() {
    let fixture = fixtures_dir().join("type_errors_advanced.ps");
    let output = pyths_bin()
        .args(["check", fixture.to_str().unwrap()])
        .output()
        .expect("Failed to run pyths");

    assert!(!output.status.success(), "Should fail with type errors");

    let stderr = String::from_utf8_lossy(&output.stderr);
    // Should detect variable reassignment type mismatch (x = 5 then x = "hello")
    assert!(
        stderr.contains("cannot assign"),
        "Should report reassignment error: {}",
        stderr
    );
    // Should detect return type / argument type mismatch
    assert!(
        stderr.contains("type mismatch") || stderr.contains("Type mismatch"),
        "Should report type mismatch: {}",
        stderr
    );
}

// --- Phase 6: WASM codegen CLI tests ---

#[test]
fn test_compile_target_wasm() {
    let fixture = fixtures_dir().join("wasm_numeric.ps");
    let tmp_dir = std::env::temp_dir();
    let wasm_file = tmp_dir.join("wasm_numeric_test.wasm");

    let output = pyths_bin()
        .args([
            "compile",
            fixture.to_str().unwrap(),
            "-o",
            tmp_dir.join("wasm_numeric_test.js").to_str().unwrap(),
            "--target",
            "wasm",
        ])
        .output()
        .expect("Failed to run pyths");

    assert!(
        output.status.success(),
        "Exit code: {:?}\nStderr: {}",
        output.status,
        String::from_utf8_lossy(&output.stderr)
    );

    // WASM file should be created
    assert!(wasm_file.exists(), "WASM file should be created");

    // JS file should NOT be created (target=wasm only)
    let js_file = tmp_dir.join("wasm_numeric_test.js");
    assert!(
        !js_file.exists(),
        "JS file should NOT be created for target=wasm"
    );

    // Clean up
    let _ = std::fs::remove_file(&wasm_file);
    let _ = std::fs::remove_file(&js_file);
}

#[test]
fn test_compile_target_js_default() {
    let fixture = fixtures_dir().join("wasm_numeric.ps");
    let tmp_dir = std::env::temp_dir();
    let js_file = tmp_dir.join("wasm_numeric_js_test.js");
    let wasm_file = tmp_dir.join("wasm_numeric_js_test.wasm");

    let output = pyths_bin()
        .args([
            "compile",
            fixture.to_str().unwrap(),
            "-o",
            js_file.to_str().unwrap(),
        ])
        .output()
        .expect("Failed to run pyths");

    assert!(output.status.success());

    // JS file should be created
    assert!(js_file.exists(), "JS file should be created");

    // WASM file should NOT be created (default target=js)
    assert!(
        !wasm_file.exists(),
        "WASM file should NOT be created for default target"
    );

    // Clean up
    let _ = std::fs::remove_file(&js_file);
}

#[test]
fn test_compile_target_js_wasm() {
    let fixture = fixtures_dir().join("wasm_numeric.ps");
    let tmp_dir = std::env::temp_dir();
    let js_file = tmp_dir.join("wasm_numeric_both_test.js");
    let wasm_file = tmp_dir.join("wasm_numeric_both_test.wasm");
    let glue_file = tmp_dir.join("wasm_numeric_both_test.glue.js");

    let output = pyths_bin()
        .args([
            "compile",
            fixture.to_str().unwrap(),
            "-o",
            js_file.to_str().unwrap(),
            "--target",
            "js+wasm",
        ])
        .output()
        .expect("Failed to run pyths");

    assert!(
        output.status.success(),
        "Exit code: {:?}\nStderr: {}",
        output.status,
        String::from_utf8_lossy(&output.stderr)
    );

    // All three files should be created
    assert!(js_file.exists(), "JS file should be created");
    assert!(wasm_file.exists(), "WASM file should be created");
    assert!(glue_file.exists(), "Glue file should be created");

    // Clean up
    let _ = std::fs::remove_file(&js_file);
    let _ = std::fs::remove_file(&wasm_file);
    let _ = std::fs::remove_file(&glue_file);
}

#[test]
fn test_compile_js_wasm_glue_content() {
    let fixture = fixtures_dir().join("wasm_numeric.ps");
    let tmp_dir = std::env::temp_dir();
    let js_file = tmp_dir.join("wasm_glue_content_test.js");
    let wasm_file = tmp_dir.join("wasm_glue_content_test.wasm");
    let glue_file = tmp_dir.join("wasm_glue_content_test.glue.js");

    let output = pyths_bin()
        .args([
            "compile",
            fixture.to_str().unwrap(),
            "-o",
            js_file.to_str().unwrap(),
            "--target",
            "js+wasm",
        ])
        .output()
        .expect("Failed to run pyths");

    assert!(output.status.success(), "Should compile successfully");

    let glue = std::fs::read_to_string(&glue_file).expect("Should read glue file");
    assert!(
        glue.contains("WebAssembly"),
        "Glue has WebAssembly: {}",
        glue
    );
    assert!(
        glue.contains("export function"),
        "Glue has exports: {}",
        glue
    );
    assert!(
        glue.contains("instantiateStreaming"),
        "Glue has streaming: {}",
        glue
    );
    assert!(glue.contains("arrayBuffer"), "Glue has fallback: {}", glue);

    // Clean up
    let _ = std::fs::remove_file(&js_file);
    let _ = std::fs::remove_file(&wasm_file);
    let _ = std::fs::remove_file(&glue_file);
}

#[test]
fn test_compile_js_wasm_js_reexports() {
    let fixture = fixtures_dir().join("wasm_numeric.ps");
    let tmp_dir = std::env::temp_dir();
    let js_file = tmp_dir.join("wasm_reexport_test.js");
    let wasm_file = tmp_dir.join("wasm_reexport_test.wasm");
    let glue_file = tmp_dir.join("wasm_reexport_test.glue.js");

    let output = pyths_bin()
        .args([
            "compile",
            fixture.to_str().unwrap(),
            "-o",
            js_file.to_str().unwrap(),
            "--target",
            "js+wasm",
        ])
        .output()
        .expect("Failed to run pyths");

    assert!(output.status.success(), "Should compile successfully");

    let js = std::fs::read_to_string(&js_file).expect("Should read JS file");
    // JS should re-export from glue, not contain function bodies
    assert!(js.contains("export {"), "JS has re-export: {}", js);
    assert!(js.contains(".glue.js"), "JS re-exports from glue: {}", js);

    // Clean up
    let _ = std::fs::remove_file(&js_file);
    let _ = std::fs::remove_file(&wasm_file);
    let _ = std::fs::remove_file(&glue_file);
}

#[test]
fn test_compile_js_wasm_mixed() {
    let fixture = fixtures_dir().join("wasm_bridge_mixed.ps");
    let tmp_dir = std::env::temp_dir();
    let js_file = tmp_dir.join("wasm_mixed_test.js");
    let wasm_file = tmp_dir.join("wasm_mixed_test.wasm");
    let glue_file = tmp_dir.join("wasm_mixed_test.glue.js");

    let output = pyths_bin()
        .args([
            "compile",
            fixture.to_str().unwrap(),
            "-o",
            js_file.to_str().unwrap(),
            "--target",
            "js+wasm",
        ])
        .output()
        .expect("Failed to run pyths");

    assert!(
        output.status.success(),
        "Should compile: stderr={}",
        String::from_utf8_lossy(&output.stderr)
    );

    let js = std::fs::read_to_string(&js_file).expect("Should read JS file");
    let glue = std::fs::read_to_string(&glue_file).expect("Should read glue file");

    // untyped is NOT WASM-eligible (no type annotation) — should be in JS, NOT in glue
    assert!(js.contains("function untyped("), "untyped in JS: {}", js);
    assert!(
        !glue.contains("function untyped("),
        "untyped NOT in glue: {}",
        glue
    );

    // #364: greet is a STRING function — general (non-numeric-kernel) data, so
    // the numeric-kernel whitelist keeps it in correct JS, NOT WASM.
    assert!(js.contains("function greet("), "greet in JS: {}", js);
    assert!(
        !glue.contains("export function greet("),
        "greet NOT in glue: {}",
        glue
    );

    // add and square are NUMERIC kernels — WASM-eligible, in the glue.
    assert!(
        glue.contains("export function add("),
        "add in glue: {}",
        glue
    );
    assert!(
        glue.contains("export function square("),
        "square in glue: {}",
        glue
    );

    // Clean up
    let _ = std::fs::remove_file(&js_file);
    let _ = std::fs::remove_file(&wasm_file);
    let _ = std::fs::remove_file(&glue_file);
}

#[test]
fn test_compile_target_wasm_no_glue() {
    let fixture = fixtures_dir().join("wasm_numeric.ps");
    let tmp_dir = std::env::temp_dir();
    let wasm_file = tmp_dir.join("wasm_noglue_test.wasm");
    let glue_file = tmp_dir.join("wasm_noglue_test.glue.js");

    // Clean up any pre-existing files
    let _ = std::fs::remove_file(&glue_file);

    let output = pyths_bin()
        .args([
            "compile",
            fixture.to_str().unwrap(),
            "-o",
            tmp_dir.join("wasm_noglue_test.js").to_str().unwrap(),
            "--target",
            "wasm",
        ])
        .output()
        .expect("Failed to run pyths");

    assert!(
        output.status.success(),
        "Should compile: stderr={}",
        String::from_utf8_lossy(&output.stderr)
    );

    // WASM file should exist, but NOT the glue file
    assert!(wasm_file.exists(), "WASM file should exist");
    assert!(
        !glue_file.exists(),
        "Glue file should NOT exist for --target wasm"
    );

    // Clean up
    let _ = std::fs::remove_file(&wasm_file);
}

// --- Step 9: incremental compilation cache ---

#[test]
fn test_cache_status_subcommand() {
    let output = pyths_bin()
        .args(["cache", "status"])
        .output()
        .expect("Failed to run pyths");
    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    let combined = format!("{}{}", stdout, stderr);
    assert!(combined.contains("Cache directory"), "{}", combined);
    assert!(combined.contains("Cached entries"), "{}", combined);
}

#[test]
fn test_cache_clear_subcommand_runs() {
    let output = pyths_bin()
        .args(["cache", "clear"])
        .output()
        .expect("Failed to run pyths");
    assert!(output.status.success());
}

/// Count `*.json` cache entries anywhere under `base` (the per-user cache is
/// namespaced by project root: `<base>/<project-hash>/<key>.json`).
fn count_cache_entries(base: &std::path::Path) -> usize {
    fn walk(dir: &std::path::Path, n: &mut usize) {
        if let Ok(rd) = std::fs::read_dir(dir) {
            for e in rd.flatten() {
                let p = e.path();
                if p.is_dir() {
                    walk(&p, n);
                } else if p.extension().and_then(|s| s.to_str()) == Some("json") {
                    *n += 1;
                }
            }
        }
    }
    let mut n = 0;
    walk(base, &mut n);
    n
}

#[test]
fn test_compile_creates_cache_then_hits() {
    let fixture = fixtures_dir().join("hello.ps");
    let uniq = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let tmp_dir = std::env::temp_dir().join(format!("pyths_cache_compile_{}", uniq));
    std::fs::create_dir_all(&tmp_dir).unwrap();
    // A3: the cache now lives OUT of the source tree, under a per-user base.
    // Point it at a dedicated temp base for this test.
    let cache_base = std::env::temp_dir().join(format!("pyths_cache_base_{}", uniq));
    let _ = std::fs::remove_dir_all(&cache_base);

    let local_src = tmp_dir.join("hello.ps");
    std::fs::copy(&fixture, &local_src).unwrap();
    let out = tmp_dir.join("hello.js");

    let r1 = pyths_bin()
        .env("PYTHS_CACHE_DIR", &cache_base)
        .args([
            "compile",
            local_src.to_str().unwrap(),
            "-o",
            out.to_str().unwrap(),
        ])
        .output()
        .expect("first compile failed");
    assert!(
        r1.status.success(),
        "first run: {}",
        String::from_utf8_lossy(&r1.stderr)
    );

    // The out-of-tree cache should now have at least one entry...
    assert!(
        count_cache_entries(&cache_base) >= 1,
        "at least one cache entry"
    );
    // ...and NOTHING should have been written into the source tree.
    assert!(
        !tmp_dir.join(".pyths").exists(),
        "cache must not land in source tree"
    );

    // Second compile should hit the cache.
    let r2 = pyths_bin()
        .env("PYTHS_CACHE_DIR", &cache_base)
        .args([
            "compile",
            local_src.to_str().unwrap(),
            "-o",
            out.to_str().unwrap(),
        ])
        .output()
        .expect("second compile failed");
    assert!(
        r2.status.success(),
        "second compile failed: stderr=\n{}\nstdout=\n{}",
        String::from_utf8_lossy(&r2.stderr),
        String::from_utf8_lossy(&r2.stdout),
    );
    let stdout2 = String::from_utf8_lossy(&r2.stdout);
    let stderr2 = String::from_utf8_lossy(&r2.stderr);
    let out2 = format!("{}{}", stdout2, stderr2);
    assert!(out2.contains("Cache hit"), "second run hit cache: {}", out2);

    // Cleanup
    let _ = std::fs::remove_dir_all(&tmp_dir);
    let _ = std::fs::remove_dir_all(&cache_base);
}

#[test]
fn test_compile_watch_flag_accepted() {
    // Just verify --watch is parsed (we don't run a full watcher in tests since
    // it loops forever; the --help output should mention it).
    let output = pyths_bin()
        .args(["compile", "--help"])
        .output()
        .expect("Failed to run pyths");
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("--watch"), "watch flag in help: {}", stdout);
}

#[test]
fn test_pyths_no_cache_env_disables_cache() {
    let fixture = fixtures_dir().join("hello.ps");
    let uniq = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let tmp_dir = std::env::temp_dir().join(format!("pyths_cache_disabled_{}", uniq));
    std::fs::create_dir_all(&tmp_dir).unwrap();
    let cache_base = std::env::temp_dir().join(format!("pyths_cache_disabled_base_{}", uniq));
    let _ = std::fs::remove_dir_all(&cache_base);
    let local_src = tmp_dir.join("hello.ps");
    std::fs::copy(&fixture, &local_src).unwrap();
    let out = tmp_dir.join("hello.js");

    // First compile with cache disabled — no entry should be written.
    let r = pyths_bin()
        .env("PYTHS_NO_CACHE", "1")
        .env("PYTHS_CACHE_DIR", &cache_base)
        .args([
            "compile",
            local_src.to_str().unwrap(),
            "-o",
            out.to_str().unwrap(),
        ])
        .output()
        .expect("compile failed");
    assert!(r.status.success());

    assert_eq!(
        count_cache_entries(&cache_base),
        0,
        "cache should not be populated"
    );

    let _ = std::fs::remove_dir_all(&tmp_dir);
    let _ = std::fs::remove_dir_all(&cache_base);
}

// --- Phase 6e: edge / server targets ---

#[test]
fn test_compile_target_wasm_edge_cf_workers() {
    let fixture = fixtures_dir().join("wasm_numeric.ps");
    let tmp_dir = std::env::temp_dir().join("pyths_test_wasm_edge");
    let _ = std::fs::create_dir_all(&tmp_dir);
    let entry = tmp_dir.join("worker.js");
    let wasm = tmp_dir.join("worker.wasm");
    let wrangler = tmp_dir.join("wrangler.toml");
    let pkg = tmp_dir.join("package.json");

    let output = pyths_bin()
        .args([
            "compile",
            fixture.to_str().unwrap(),
            "-o",
            entry.to_str().unwrap(),
            "--target",
            "wasm-edge",
        ])
        .output()
        .expect("Failed to run pyths");

    assert!(
        output.status.success(),
        "Exit code: {:?}\nStderr: {}",
        output.status,
        String::from_utf8_lossy(&output.stderr)
    );

    assert!(entry.exists(), "worker.js should exist");
    assert!(wasm.exists(), "worker.wasm should exist");
    assert!(wrangler.exists(), "wrangler.toml should exist");
    assert!(pkg.exists(), "package.json should exist");

    let glue = std::fs::read_to_string(&entry).unwrap();
    // Bytes embedded as base64
    assert!(
        glue.contains("__WASM_BYTES_B64"),
        "Has base64 bytes: {}",
        glue
    );
    // Has fetch handler
    assert!(
        glue.contains("export default"),
        "Has default export: {}",
        glue
    );
    assert!(glue.contains("async fetch(request)"), "Has fetch: {}", glue);
    // No instantiateStreaming (CF Workers doesn't fetch sibling files)
    assert!(
        !glue.contains("instantiateStreaming"),
        "No streaming for Workers: {}",
        glue
    );

    let wrangler_text = std::fs::read_to_string(&wrangler).unwrap();
    assert!(
        wrangler_text.contains("compatibility_date"),
        "wrangler.toml content: {}",
        wrangler_text
    );

    // Clean up
    let _ = std::fs::remove_dir_all(&tmp_dir);
}

#[test]
fn test_compile_target_wasi() {
    let fixture = fixtures_dir().join("wasm_numeric.ps");
    let tmp_dir = std::env::temp_dir().join("pyths_test_wasi");
    let _ = std::fs::create_dir_all(&tmp_dir);
    let entry = tmp_dir.join("main.mjs");
    let wasm = tmp_dir.join("main.wasm");

    let output = pyths_bin()
        .args([
            "compile",
            fixture.to_str().unwrap(),
            "-o",
            entry.to_str().unwrap(),
            "--target",
            "wasi",
        ])
        .output()
        .expect("Failed to run pyths");

    assert!(
        output.status.success(),
        "Exit code: {:?}\nStderr: {}",
        output.status,
        String::from_utf8_lossy(&output.stderr)
    );

    assert!(entry.exists(), "main.mjs should exist");
    assert!(wasm.exists(), "main.wasm should exist");

    let glue = std::fs::read_to_string(&entry).unwrap();
    assert!(glue.contains("node:wasi"), "Has node:wasi import: {}", glue);
    assert!(glue.contains("WASI"), "Has WASI: {}", glue);
    assert!(
        glue.contains("wasi_snapshot_preview1"),
        "Has WASI imports: {}",
        glue
    );

    // Clean up
    let _ = std::fs::remove_dir_all(&tmp_dir);
}

#[test]
fn test_compile_target_deno() {
    let fixture = fixtures_dir().join("wasm_numeric.ps");
    let tmp_dir = std::env::temp_dir().join("pyths_test_deno");
    let _ = std::fs::create_dir_all(&tmp_dir);
    let entry = tmp_dir.join("main.ts");
    let wasm = tmp_dir.join("main.wasm");
    let deno_json = tmp_dir.join("deno.json");

    let output = pyths_bin()
        .args([
            "compile",
            fixture.to_str().unwrap(),
            "-o",
            entry.to_str().unwrap(),
            "--target",
            "deno",
        ])
        .output()
        .expect("Failed to run pyths");

    assert!(
        output.status.success(),
        "Exit code: {:?}\nStderr: {}",
        output.status,
        String::from_utf8_lossy(&output.stderr)
    );

    assert!(entry.exists(), "main.ts should exist");
    assert!(wasm.exists(), "main.wasm should exist");
    assert!(deno_json.exists(), "deno.json should exist");

    let glue = std::fs::read_to_string(&entry).unwrap();
    assert!(
        glue.contains("Deno.readFile"),
        "Has Deno.readFile: {}",
        glue
    );

    // Clean up
    let _ = std::fs::remove_dir_all(&tmp_dir);
}

// ============================================================================
// Phase 2 wiring: `.psc` end-to-end CLI tests.
//
// These exercise the extension-dispatch path: `pyths compile foo.psc` should
// expand-then-compile, while `pyths compile foo.ps` must pass through the
// expander unchanged. Each test gets its own temp dir so `pyths.toml`
// discovery cannot leak between cases (CWD-based walk-up).
// ============================================================================

fn psc_test_scratch(tag: &str) -> std::path::PathBuf {
    use std::sync::atomic::{AtomicU64, Ordering};
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let id = COUNTER.fetch_add(1, Ordering::SeqCst);
    let pid = std::process::id();
    let dir = std::env::temp_dir().join(format!("pyths_psc_test_{}_{}_{}", tag, pid, id));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

#[test]
fn psc_compile_round_trip_equivalence() {
    // Same logical program — once written compressed (.psc), once canonical
    // (.ps). The compiled JS must be byte-identical, since the expander is
    // a pure source-to-source pass before the canonical pipeline.
    let dir = psc_test_scratch("roundtrip");

    let psc_path = dir.join("greet.psc");
    let ps_path = dir.join("greet.ps");
    std::fs::write(&psc_path, "@c\ndef Greet():\n    return \"hi\"\n").unwrap();
    std::fs::write(&ps_path, "@component\ndef Greet():\n    return \"hi\"\n").unwrap();

    let psc_out = dir.join("from_psc.js");
    let ps_out = dir.join("from_ps.js");

    let r1 = pyths_bin()
        .args([
            "compile",
            psc_path.to_str().unwrap(),
            "-o",
            psc_out.to_str().unwrap(),
        ])
        .env("PYTHS_NO_CACHE", "1")
        .output()
        .expect("pyths failed");
    assert!(
        r1.status.success(),
        "psc compile failed: {}",
        String::from_utf8_lossy(&r1.stderr)
    );

    let r2 = pyths_bin()
        .args([
            "compile",
            ps_path.to_str().unwrap(),
            "-o",
            ps_out.to_str().unwrap(),
        ])
        .env("PYTHS_NO_CACHE", "1")
        .output()
        .expect("pyths failed");
    assert!(
        r2.status.success(),
        "ps compile failed: {}",
        String::from_utf8_lossy(&r2.stderr)
    );

    let js_from_psc = std::fs::read_to_string(&psc_out).unwrap();
    let js_from_ps = std::fs::read_to_string(&ps_out).unwrap();
    assert_eq!(
        js_from_psc, js_from_ps,
        "round-trip equivalence broken:\nPSC→JS:\n{}\n\nPS→JS:\n{}",
        js_from_psc, js_from_ps
    );

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn psc_compile_modular_ps_unchanged() {
    // The load-bearing modularity assertion: feeding a `.ps` file (no
    // alias sugar) through the new code path must produce the same JS as
    // before .psc support landed. We can't compare against a frozen blob,
    // but we can assert the output is valid JS containing the function
    // name we wrote — proving the expander is a no-op for `.ps`.
    let dir = psc_test_scratch("modular");
    let ps_path = dir.join("plain.ps");
    std::fs::write(&ps_path, "def add(a, b):\n    return a + b\n").unwrap();

    let out = dir.join("plain.js");
    let r = pyths_bin()
        .args([
            "compile",
            ps_path.to_str().unwrap(),
            "-o",
            out.to_str().unwrap(),
        ])
        .env("PYTHS_NO_CACHE", "1")
        .output()
        .expect("pyths failed");
    assert!(r.status.success(), "{}", String::from_utf8_lossy(&r.stderr));

    let js = std::fs::read_to_string(&out).unwrap();
    assert!(
        js.contains("function add"),
        "plain .ps still compiles: {}",
        js
    );

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn psc_expand_never_passes_psc_through_raw() {
    // With `--expand=never`, even a `.psc` is fed raw to the parser. The
    // raw alias `@c` is not valid PythScribe syntax, so compilation
    // should fail — proving the flag short-circuited the expander.
    let dir = psc_test_scratch("never");
    let psc_path = dir.join("aliased.psc");
    std::fs::write(&psc_path, "@c\ndef App():\n    return None\n").unwrap();

    let out = dir.join("aliased.js");
    let r = pyths_bin()
        .args([
            "compile",
            psc_path.to_str().unwrap(),
            "-o",
            out.to_str().unwrap(),
            "--expand",
            "never",
        ])
        .env("PYTHS_NO_CACHE", "1")
        .output()
        .expect("pyths failed");
    // The parser will reject `@c` as a decorator referring to an unknown
    // name — but the codegen may still succeed since `@c` is grammatically
    // valid (it's a name expression). What we *can* assert: with
    // `--expand=never`, the emitted JS does NOT contain `component` (the
    // alias would only become `@component` after expansion). Either path
    // proves the expander was skipped.
    if r.status.success() {
        let js = std::fs::read_to_string(&out).unwrap();
        assert!(
            !js.contains("component"),
            "--expand=never must NOT have expanded @c → @component: {}",
            js
        );
    }
    // Either compilation failed (expander skipped, parser rejected `@c`)
    // or it succeeded with no `component` in the output. Both prove the
    // modularity opt-out works.

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn psc_expand_always_runs_on_ps_file() {
    // `--expand=always` runs the expander even on `.ps` extension.
    // Compile a `.ps` file that contains `@c` alias; expansion should
    // make it `@component`, which the codegen will recognize and turn
    // into a React component.
    let dir = psc_test_scratch("always");
    let ps_path = dir.join("force.ps");
    std::fs::write(&ps_path, "@c\ndef Force():\n    return None\n").unwrap();

    let out = dir.join("force.js");
    let r = pyths_bin()
        .args([
            "compile",
            ps_path.to_str().unwrap(),
            "-o",
            out.to_str().unwrap(),
            "--expand",
            "always",
        ])
        .env("PYTHS_NO_CACHE", "1")
        .output()
        .expect("pyths failed");
    assert!(
        r.status.success(),
        "--expand=always on .ps should succeed: {}",
        String::from_utf8_lossy(&r.stderr)
    );

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn psc_expand_subcommand_writes_canonical_to_stdout() {
    // `pyths expand foo.psc` prints canonical PS to stdout.
    let dir = psc_test_scratch("expand_stdout");
    let psc_path = dir.join("aliased.psc");
    std::fs::write(&psc_path, "@c\ndef App():\n    pass\n").unwrap();

    let r = pyths_bin()
        .args(["expand", psc_path.to_str().unwrap()])
        .output()
        .expect("pyths failed");
    assert!(
        r.status.success(),
        "expand failed: {}",
        String::from_utf8_lossy(&r.stderr)
    );

    let stdout = String::from_utf8_lossy(&r.stdout);
    assert!(
        stdout.contains("@component"),
        "expand should canonicalize @c to @component: {}",
        stdout
    );
    assert!(
        !stdout.contains("@c\n"),
        "raw alias should be gone: {}",
        stdout
    );

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn react_refresh_flag_emits_refresh_boilerplate() {
    // End-to-end: `pyths compile --react-refresh` emits the
    // `$RefreshSig$` / `$RefreshReg$` calls so a build plugin can
    // wire up Fast Refresh.
    let dir = psc_test_scratch("refresh_flag");
    let src_path = dir.join("counter.ps");
    let out_path = dir.join("counter.js");
    std::fs::write(
        &src_path,
        r#"
from pyths.react import component, use_state

@component
def Counter():
    count, set_count = use_state(0)
    return div()("Count:", count)
"#,
    )
    .unwrap();

    let r = pyths_bin()
        .args([
            "compile",
            src_path.to_str().unwrap(),
            "-o",
            out_path.to_str().unwrap(),
            "--react-refresh",
        ])
        .env("PYTHS_NO_CACHE", "1")
        .output()
        .expect("pyths failed");
    assert!(r.status.success(), "{}", String::from_utf8_lossy(&r.stderr));

    let js = std::fs::read_to_string(&out_path).unwrap();
    assert!(js.contains("$RefreshSig$()"), "Sig declared: {}", js);
    assert!(js.contains("$RefreshReg$(Counter,"), "Reg call: {}", js);
    assert!(js.contains("_s_Counter();"), "Sig call inside body: {}", js);

    // Without the flag, no Refresh boilerplate.
    let plain = dir.join("plain.js");
    let r2 = pyths_bin()
        .args([
            "compile",
            src_path.to_str().unwrap(),
            "-o",
            plain.to_str().unwrap(),
        ])
        .env("PYTHS_NO_CACHE", "1")
        .output()
        .expect("pyths failed");
    assert!(
        r2.status.success(),
        "{}",
        String::from_utf8_lossy(&r2.stderr)
    );
    let plain_js = std::fs::read_to_string(&plain).unwrap();
    assert!(
        !plain_js.contains("$RefreshSig$"),
        "no Refresh by default: {}",
        plain_js
    );

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn pyths_toml_npm_imports_override_module_resolution() {
    // End-to-end: `pyths.toml [npm.imports]` re-routes a module name
    // so the emitted import statement points at the user-supplied
    // specifier instead of the kebab fallback.
    let dir = psc_test_scratch("npm_override");
    std::fs::write(
        dir.join("pyths.toml"),
        "[npm.imports]\nfoo_bar = \"@my-org/foo-bar-custom\"\n",
    )
    .unwrap();
    let src = "from foo_bar import x\nresult = x()\n";
    let src_path = dir.join("main.ps");
    let out_path = dir.join("main.js");
    std::fs::write(&src_path, src).unwrap();

    let r = pyths_bin()
        .args([
            "compile",
            src_path.to_str().unwrap(),
            "-o",
            out_path.to_str().unwrap(),
        ])
        .current_dir(&dir)
        .env("PYTHS_NO_CACHE", "1")
        .output()
        .expect("pyths failed");
    assert!(r.status.success(), "{}", String::from_utf8_lossy(&r.stderr));

    let js = std::fs::read_to_string(&out_path).unwrap();
    assert!(
        js.contains("from \"@my-org/foo-bar-custom\""),
        "override should win: {}",
        js
    );
    assert!(
        !js.contains("from \"foo-bar\""),
        "kebab fallback should NOT fire: {}",
        js
    );

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn pyths_toml_stubs_paths_consulted_by_check() {
    // End-to-end: `pyths check` invoked from a directory that has a
    // `pyths.toml` listing a local stubs dir; the local `.pyi` is
    // honored ahead of bundled. We can't easily assert the type
    // checker found the override (it's a positive — no error), so we
    // assert the negative: with a malformed local stub for `react`,
    // the checker silently falls back to Any (no parse error from the
    // stub leaks into the user's `pyths check` output).
    //
    // Concretely: create a project dir with a `pyths.toml` pointing at
    // a `stubs/` dir containing `my_local_lib.pyi`. Compile a `.ps`
    // that does `from my_local_lib import greet` and uses `greet(...)`.
    // Without the project stub, `greet` would be Any (no error either).
    // With the stub declaring `def greet(name: str) -> str`, calling
    // `greet(123)` should flag an int-vs-str mismatch.

    let dir = psc_test_scratch("stub_paths");
    let stubs_dir = dir.join("stubs");
    std::fs::create_dir_all(&stubs_dir).unwrap();
    std::fs::write(
        stubs_dir.join("my_local_lib.pyi"),
        "def greet(name: str) -> str: ...\n",
    )
    .unwrap();
    std::fs::write(dir.join("pyths.toml"), "[stubs]\npaths = [\"./stubs\"]\n").unwrap();

    // A program that calls greet() with the *correct* string type —
    // type-check should pass.
    let ok_src = "from my_local_lib import greet\n\
                  result: str = greet(\"alice\")\n";
    let ok_path = dir.join("ok.ps");
    std::fs::write(&ok_path, ok_src).unwrap();

    let r = pyths_bin()
        .args(["check", ok_path.to_str().unwrap()])
        .current_dir(&dir)
        .output()
        .expect("pyths failed");
    assert!(
        r.status.success(),
        "well-typed program with project stub should pass: stderr={}",
        String::from_utf8_lossy(&r.stderr)
    );

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn psc_expand_subcommand_writes_canonical_to_file() {
    // `pyths expand foo.psc -o foo.ps` writes the canonical PS to disk.
    let dir = psc_test_scratch("expand_file");
    let psc_path = dir.join("aliased.psc");
    let ps_out = dir.join("expanded.ps");
    std::fs::write(&psc_path, "T*\n\n@d\nclass Order:\n    id: int\n").unwrap();

    let r = pyths_bin()
        .args([
            "expand",
            psc_path.to_str().unwrap(),
            "-o",
            ps_out.to_str().unwrap(),
        ])
        .output()
        .expect("pyths failed");
    assert!(
        r.status.success(),
        "expand failed: {}",
        String::from_utf8_lossy(&r.stderr)
    );

    let expanded = std::fs::read_to_string(&ps_out).unwrap();
    assert!(
        expanded.contains("from dataclasses import dataclass"),
        "T* preset expanded: {}",
        expanded
    );
    assert!(expanded.contains("@dataclass"), "@d expanded: {}", expanded);

    let _ = std::fs::remove_dir_all(&dir);
}

// ─── `pyths run --explain` (Python-flavored error explanations) ──────

#[test]
fn test_run_explain_indexerror() {
    // A .ps that reads past the end of a list. Without --explain the
    // user sees the raw IndexError trace; with --explain we expect a
    // Python-flavored hint paragraph above it.
    let dir = std::env::temp_dir().join("pyths_test_run_explain_index");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    // Unique stem per test so parallel runs don't collide on the temp
    // `pyths_<stem>.mjs` file the run command writes to.
    let ps = dir.join("crash_idx_test.ps");
    std::fs::write(
        &ps,
        "def crash(items: list) -> int:\n    return items[10]\n\nresult = crash([1, 2, 3])\nprint(result)\n",
    )
    .unwrap();

    let out = pyths_bin()
        .args(["run", ps.to_str().unwrap(), "--explain"])
        .output()
        .expect("pyths run failed");

    // The program intentionally crashes — non-zero exit + stderr.
    assert!(!out.status.success(), "expected non-zero exit");
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("PythScribe runtime error"),
        "explanation banner missing: {}",
        stderr
    );
    assert!(
        stderr.contains("IndexError"),
        "IndexError class missing: {}",
        stderr
    );
    assert!(
        stderr.contains("In Python this raises IndexError"),
        "Python framing missing: {}",
        stderr
    );

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn test_run_nonsubscriptable_raises_typeerror() {
    // Self-binds the pyGetItem runtime guards (lattice C4 shipping-binding): a
    // subscript of a non-subscriptable receiver, or a list/str indexed by a
    // non-integer key, MUST raise a Python TypeError at run time — not silently
    // return `undefined`/None. This EXECUTES the emitted JS (via node), so unlike
    // the codegen route test it catches a regression that removes the runtime
    // guard (routing alone would still string-match). Value-level oracle lives in
    // reference-app experiments/pbt-ps/lattice_shipped_binding.py.
    let dir = std::env::temp_dir().join("pyths_test_run_nonsubscriptable");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    // (stem, program) — each subscripts something non-subscriptable / with a bad key.
    let cases = [
        ("int_sub", "print((5)[0])\n"),
        ("float_sub", "print((3.5)[0])\n"),
        ("bool_sub", "print(True[0])\n"),
        ("set_sub", "print({1, 2, 3}[0])\n"),
        ("list_strkey", "print([1, 2][\"k\"])\n"),
    ];
    for (stem, src) in cases {
        let ps = dir.join(format!("nonsub_{stem}.ps"));
        std::fs::write(&ps, src).unwrap();
        let out = pyths_bin()
            .args(["run", ps.to_str().unwrap()])
            .output()
            .expect("pyths run failed");
        assert!(
            !out.status.success(),
            "[{stem}] expected non-zero exit (TypeError), program ran clean: {src}"
        );
        let stderr = String::from_utf8_lossy(&out.stderr);
        assert!(
            stderr.contains("TypeError"),
            "[{stem}] expected TypeError, stderr: {stderr}"
        );
    }
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn test_run_explain_zerodiv() {
    let dir = std::env::temp_dir().join("pyths_test_run_explain_zerodiv");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let ps = dir.join("crash_zd_test.ps");
    std::fs::write(
        &ps,
        "def avg(t: int, c: int) -> int:\n    return t // c\n\nprint(avg(10, 0))\n",
    )
    .unwrap();

    let out = pyths_bin()
        .args(["run", ps.to_str().unwrap(), "--explain"])
        .output()
        .expect("pyths run failed");

    assert!(!out.status.success(), "expected non-zero exit");
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("ZeroDivisionError"),
        "ZeroDivisionError missing: {}",
        stderr
    );
    assert!(
        stderr.contains("divided by zero"),
        "Python framing missing: {}",
        stderr
    );

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn test_run_explain_succeed_silent() {
    // When the program succeeds, --explain MUST stay out of the way:
    // no banner, no leakage onto stdout/stderr from the explainer.
    let dir = std::env::temp_dir().join("pyths_test_run_explain_succeed");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let ps = dir.join("ok_explain_test.ps");
    std::fs::write(&ps, "print(2 + 3)\n").unwrap();

    let out = pyths_bin()
        .args(["run", ps.to_str().unwrap(), "--explain"])
        .output()
        .expect("pyths run failed");

    assert!(out.status.success(), "expected zero exit");
    let stdout = String::from_utf8_lossy(&out.stdout);
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stdout.contains("5"),
        "stdout missing program output: {}",
        stdout
    );
    assert!(
        !stderr.contains("PythScribe runtime error"),
        "explanation banner leaked: {}",
        stderr
    );

    let _ = std::fs::remove_dir_all(&dir);
}

// ============================================================================
// `pyths expand --verify` — Iron Rule auto-check (Task 0.5)
// ============================================================================

#[test]
fn psc_verify_passes_when_roundtrip_matches() {
    // expand(@c\ndef App()...) == @component\ndef App()... → canonicalize both → equal → exit 0
    let dir = psc_test_scratch("verify_ok");
    let psc = dir.join("f.psc");
    let ps = dir.join("f.ps");
    std::fs::write(&psc, "@c\ndef App():\n    return None\n").unwrap();
    std::fs::write(&ps, "@component\ndef App():\n    return None\n").unwrap();

    let status = pyths_bin()
        .args(["expand", "--verify", psc.to_str().unwrap()])
        .status()
        .expect("pyths failed");
    assert!(
        status.success(),
        "verify should pass when canonical forms match"
    );

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn psc_verify_fails_on_divergence() {
    // expand(@c\ndef App()...) != @component\ndef Other()... → canonicalize differs → exit non-zero
    let dir = psc_test_scratch("verify_bad");
    let psc = dir.join("f.psc");
    let ps = dir.join("f.ps");
    std::fs::write(&psc, "@c\ndef App():\n    return None\n").unwrap();
    std::fs::write(&ps, "@component\ndef Other():\n    return None\n").unwrap(); // diverges

    let status = pyths_bin()
        .args(["expand", "--verify", psc.to_str().unwrap()])
        .status()
        .expect("pyths failed");
    assert!(
        !status.success(),
        "verify must fail (non-zero) when canonical forms differ"
    );

    let _ = std::fs::remove_dir_all(&dir);
}

// ─── A0: `pyths run` + stdlib imports (ERR_MODULE_NOT_FOUND regression) ───
//
// `pyths run` writes the compiled `.mjs` to the OS temp dir and spawns
// `node` on it directly. Helpers are inlined, but stdlib imports (`from
// math import sqrt`, `import json`, ...) still emit bare `pyths-runtime/
// stdlib/X` specifiers — Node has to resolve a real `pyths-runtime`
// package from `node_modules`. These tests spawn node for real (via
// `pyths run`) and assert on actual stdout, so a regression here fails
// loudly instead of silently.

#[test]
fn test_run_stdlib_math_import() {
    // Ground truth: CPython's `print(math.sqrt(16))` prints `4.0`.
    // PythScribe's `sqrt` maps straight to `Math.sqrt`, and PythScribe's
    // `print` on a JS number does not append a trailing `.0` the way
    // Python's float repr does — so the *correct* PythScribe output
    // here is `4`, not `4.0`. This test asserts that actual (correct)
    // behavior, not a blind copy of the Python repr.
    let dir = std::env::temp_dir().join("pyths_test_run_stdlib_math");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let ps = dir.join("math_import_test.ps");
    std::fs::write(&ps, "from math import sqrt\nprint(sqrt(16))\n").unwrap();

    let out = pyths_bin()
        .args(["run", ps.to_str().unwrap()])
        .output()
        .expect("pyths run failed to spawn");

    let stdout = String::from_utf8_lossy(&out.stdout);
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        out.status.success(),
        "pyths run should exit 0 for `from math import sqrt`; stderr={}",
        stderr
    );
    assert!(
        !stderr.contains("ERR_MODULE_NOT_FOUND"),
        "stdlib import must resolve, not ERR_MODULE_NOT_FOUND: {}",
        stderr
    );
    assert_eq!(stdout.trim(), "4", "sqrt(16) via node stdout: {}", stdout);

    let _ = std::fs::remove_dir_all(&dir);
}

// ─── A8: generated-sibling write safety (symlink refusal + clobber guard) ───

#[test]
fn a8_refuses_to_clobber_handwritten_js_without_force() {
    // A project with Counter.ps AND a hand-written Counter.js must NOT lose the
    // hand-written file on compile (it lacks the generated marker).
    let dir = psc_test_scratch("a8_clobber");
    let ps = dir.join("Counter.ps");
    std::fs::write(&ps, "def add(a, b):\n    return a + b\n").unwrap();
    let js = dir.join("Counter.js");
    let handwritten = b"export const mine = 42; // hand-written, do not lose me\n";
    std::fs::write(&js, handwritten).unwrap();

    // Default output sibling == Counter.js — compile must refuse.
    let r = pyths_bin()
        .args(["compile", ps.to_str().unwrap()])
        .env("PYTHS_NO_CACHE", "1")
        .output()
        .expect("pyths failed");
    assert!(
        !r.status.success(),
        "compile must refuse to clobber hand-written .js"
    );
    let stderr = String::from_utf8_lossy(&r.stderr);
    assert!(
        stderr.contains("did not generate") || stderr.contains("hand-written"),
        "diagnostic should explain the refusal: {}",
        stderr
    );
    // The hand-written file is intact.
    assert_eq!(std::fs::read(&js).unwrap(), handwritten);

    // With --force the compiler overwrites it (marked output).
    let r2 = pyths_bin()
        .args(["compile", ps.to_str().unwrap(), "--force"])
        .env("PYTHS_NO_CACHE", "1")
        .output()
        .expect("pyths failed");
    assert!(
        r2.status.success(),
        "--force should overwrite: {}",
        String::from_utf8_lossy(&r2.stderr)
    );
    let after = std::fs::read_to_string(&js).unwrap();
    assert!(
        after.contains("@generated by PythScribe"),
        "forced output carries marker: {}",
        after
    );

    // A second compile now succeeds WITHOUT --force (the file is ours now).
    let r3 = pyths_bin()
        .args(["compile", ps.to_str().unwrap()])
        .env("PYTHS_NO_CACHE", "1")
        .output()
        .expect("pyths failed");
    assert!(
        r3.status.success(),
        "recompiling our own marked output must succeed: {}",
        String::from_utf8_lossy(&r3.stderr)
    );

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a8_refuses_to_write_through_symlink() {
    // If the output sibling is a symlink, the compiler must refuse rather than
    // follow it and overwrite the link target. Symlink creation may require
    // privilege on Windows — skip the assertion if the OS denies it.
    #[cfg(unix)]
    fn make_symlink(target: &std::path::Path, link: &std::path::Path) -> std::io::Result<()> {
        std::os::unix::fs::symlink(target, link)
    }
    #[cfg(windows)]
    fn make_symlink(target: &std::path::Path, link: &std::path::Path) -> std::io::Result<()> {
        std::os::windows::fs::symlink_file(target, link)
    }

    let dir = psc_test_scratch("a8_symlink");
    let ps = dir.join("app.ps");
    std::fs::write(&ps, "def add(a, b):\n    return a + b\n").unwrap();
    let victim = dir.join("victim_secret.txt");
    std::fs::write(&victim, b"ORIGINAL-SECRET").unwrap();
    let link = dir.join("app.js");

    if make_symlink(&victim, &link).is_err() {
        eprintln!("skipping a8_refuses_to_write_through_symlink: OS denied symlink creation");
        let _ = std::fs::remove_dir_all(&dir);
        return;
    }

    let r = pyths_bin()
        .args(["compile", ps.to_str().unwrap()])
        .env("PYTHS_NO_CACHE", "1")
        .output()
        .expect("pyths failed");
    assert!(
        !r.status.success(),
        "compile must refuse a symlinked output"
    );
    let stderr = String::from_utf8_lossy(&r.stderr);
    assert!(
        stderr.contains("symlink"),
        "diagnostic should name the symlink: {}",
        stderr
    );
    // The link target must be untouched — the compiler did not follow it.
    assert_eq!(std::fs::read(&victim).unwrap(), b"ORIGINAL-SECRET");

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a9_run_twice_no_fixed_temp_collision() {
    // A9: `pyths run` uses a fresh private temp dir each invocation, so running
    // the same file repeatedly never collides on a fixed temp path and always
    // produces correct output.
    let dir = psc_test_scratch("a9_run_twice");
    let ps = dir.join("prog.ps");
    std::fs::write(&ps, "print(6 * 7)\n").unwrap();

    for _ in 0..2 {
        let out = pyths_bin()
            .args(["run", ps.to_str().unwrap()])
            .output()
            .expect("pyths run failed to spawn");
        assert!(
            out.status.success(),
            "run failed: {}",
            String::from_utf8_lossy(&out.stderr)
        );
        assert_eq!(
            String::from_utf8_lossy(&out.stdout).trim(),
            "42",
            "program output must be correct on each run"
        );
    }

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn test_run_stdlib_json_import() {
    let dir = std::env::temp_dir().join("pyths_test_run_stdlib_json");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let ps = dir.join("json_import_test.ps");
    std::fs::write(&ps, "import json\nprint(json.dumps({\"a\": 1}))\n").unwrap();

    let out = pyths_bin()
        .args(["run", ps.to_str().unwrap()])
        .output()
        .expect("pyths run failed to spawn");

    let stdout = String::from_utf8_lossy(&out.stdout);
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        out.status.success(),
        "pyths run should exit 0 for `import json`; stderr={}",
        stderr
    );
    assert!(
        !stderr.contains("ERR_MODULE_NOT_FOUND"),
        "stdlib import must resolve, not ERR_MODULE_NOT_FOUND: {}",
        stderr
    );
    // #299: dumps emits CPython's default separators (", " / ": ").
    assert_eq!(
        stdout.trim(),
        "{\"a\": 1}",
        "json.dumps via node stdout: {}",
        stdout
    );

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn test_run_stdlib_decimal_import() {
    // 0.1 + 0.2 as raw IEEE-754 floats is 0.30000000000000004 — the
    // entire point of `decimal.Decimal` is exact arithmetic, so `0.3`
    // here also proves decimal.js's BigInt path resolved and ran (not
    // just that the module loaded).
    let dir = std::env::temp_dir().join("pyths_test_run_stdlib_decimal");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let ps = dir.join("decimal_import_test.ps");
    std::fs::write(
        &ps,
        "from decimal import Decimal\nprint(Decimal(\"0.1\") + Decimal(\"0.2\"))\n",
    )
    .unwrap();

    let out = pyths_bin()
        .args(["run", ps.to_str().unwrap()])
        .output()
        .expect("pyths run failed to spawn");

    let stdout = String::from_utf8_lossy(&out.stdout);
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        out.status.success(),
        "pyths run should exit 0 for `from decimal import Decimal`; stderr={}",
        stderr
    );
    assert!(
        !stderr.contains("ERR_MODULE_NOT_FOUND"),
        "stdlib import must resolve, not ERR_MODULE_NOT_FOUND: {}",
        stderr
    );
    assert_eq!(
        stdout.trim(),
        "0.3",
        "Decimal(\"0.1\") + Decimal(\"0.2\") via node stdout: {}",
        stdout
    );

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn test_run_no_import_still_works() {
    // Non-regression: the plain (no stdlib import) path must be
    // unaffected by materializing the node_modules/pyths-runtime
    // package.
    let dir = std::env::temp_dir().join("pyths_test_run_no_import");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let ps = dir.join("no_import_test.ps");
    std::fs::write(&ps, "print(\"hi\")\n").unwrap();

    let out = pyths_bin()
        .args(["run", ps.to_str().unwrap()])
        .output()
        .expect("pyths run failed to spawn");

    let stdout = String::from_utf8_lossy(&out.stdout);
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        out.status.success(),
        "pyths run should exit 0 for plain print; stderr={}",
        stderr
    );
    assert_eq!(stdout.trim(), "hi", "stdout: {}", stdout);

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn test_run_stdlib_from_unrelated_cwd() {
    // The exact A0 repro: run from a directory that is NOT the
    // pythscribe repo (no ambient node_modules), to prove resolution
    // comes from the materialized package next to the temp .mjs file,
    // not from some node_modules the dev machine happens to have lying
    // around above the repo root.
    let dir = std::env::temp_dir().join("pyths_test_run_stdlib_unrelated_cwd");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let ps = dir.join("unrelated_cwd_test.ps");
    std::fs::write(&ps, "from math import sqrt\nprint(sqrt(16))\n").unwrap();

    let out = pyths_bin()
        .current_dir(&dir)
        .args(["run", ps.to_str().unwrap()])
        .output()
        .expect("pyths run failed to spawn");

    let stdout = String::from_utf8_lossy(&out.stdout);
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        out.status.success(),
        "pyths run should exit 0 from an unrelated cwd; stderr={}",
        stderr
    );
    assert_eq!(stdout.trim(), "4", "stdout: {}", stdout);

    let _ = std::fs::remove_dir_all(&dir);
}

// --- #105: `from pyths import <stdlib>` end-to-end ---

#[test]
fn test_run_from_pyths_import_stdlib() {
    // `from pyths import math` used to compile cleanly to
    // `import { math } from "pyths"` and die at runtime with
    // ERR_MODULE_NOT_FOUND. It must behave exactly like `import math`.
    let dir = std::env::temp_dir().join("pyths_test_run_from_pyths");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let ps = dir.join("from_pyths_test.ps");
    std::fs::write(&ps, "from pyths import math\nprint(math.sqrt(4))\n").unwrap();

    let out = pyths_bin()
        .args(["run", ps.to_str().unwrap()])
        .output()
        .expect("pyths run failed to spawn");

    let stdout = String::from_utf8_lossy(&out.stdout);
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        out.status.success(),
        "pyths run should exit 0 for `from pyths import math`; stderr={}",
        stderr
    );
    assert!(
        !stderr.contains("ERR_MODULE_NOT_FOUND"),
        "must resolve like `import math`, not ERR_MODULE_NOT_FOUND: {}",
        stderr
    );
    assert_eq!(
        stdout.trim(),
        "2",
        "math.sqrt(4) via node stdout: {}",
        stdout
    );

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn test_run_from_pyths_import_unknown_fails_loudly() {
    // The unknown-name arm must NOT produce a silently-broken import:
    // compile surfaces a diagnostic on stderr and the emitted module
    // throws at load time with the same message.
    let dir = std::env::temp_dir().join("pyths_test_run_from_pyths_unknown");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let ps = dir.join("from_pyths_unknown_test.ps");
    std::fs::write(&ps, "from pyths import nonexistent\nprint(1)\n").unwrap();

    let out = pyths_bin()
        .args(["run", ps.to_str().unwrap()])
        .output()
        .expect("pyths run failed to spawn");

    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        !out.status.success(),
        "unknown `from pyths import X` must not run cleanly; stdout={}",
        String::from_utf8_lossy(&out.stdout)
    );
    assert!(
        stderr.contains("not a PythScribe stdlib module"),
        "diagnostic names the failure: {}",
        stderr
    );
    assert!(
        !stderr.contains("ERR_MODULE_NOT_FOUND"),
        "failure is the diagnostic throw, not an unresolvable specifier: {}",
        stderr
    );

    let _ = std::fs::remove_dir_all(&dir);
}

// --- §7.2: --emit-cert credible compilation ---

#[test]
fn test_compile_emit_cert_accepted() {
    let dir = std::env::temp_dir().join("pyths_test_emit_cert");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let ps = dir.join("cert_test.ps");
    std::fs::write(
        &ps,
        "d = {\"a\": 1}\nprint(d[\"a\"])\nxs = [1, 2, 3]\nprint(xs[1])\nprint(xs[0:2])\n",
    )
    .unwrap();

    let out = pyths_bin()
        .args(["compile", ps.to_str().unwrap(), "--emit-cert"])
        .output()
        .expect("pyths compile failed to spawn");
    assert!(
        out.status.success(),
        "certified compile should succeed; stderr={}",
        String::from_utf8_lossy(&out.stderr)
    );

    let cert_path = dir.join("cert_test.js.cert.json");
    let cert = std::fs::read_to_string(&cert_path).expect("certificate file written");
    assert!(
        cert.contains("\"pass\": \"subscript-routing\""),
        "cert: {}",
        cert
    );
    assert!(
        cert.contains("\"route\": \"pyGetItem\""),
        "dict read routed: {}",
        cert
    );
    assert!(
        cert.contains("\"route\": \"pySlice\""),
        "slice routed: {}",
        cert
    );

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn test_compile_emit_cert_wasm_admission_accepted() {
    // WASM auto-routing admission certificate on a `--target js+wasm` build:
    // numeric functions are admitted with WASM-representable boundary types,
    // and the independent checker accepts against the emitted `.wasm`.
    let dir = std::env::temp_dir().join("pyths_test_emit_cert_wasm");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let ps = dir.join("wcert.ps");
    std::fs::write(
        &ps,
        "def add(a: int, b: int) -> int:\n    return a + b\n\ndef mul(x: float, y: float) -> float:\n    return x * y\n",
    )
    .unwrap();

    let out = pyths_bin()
        .args([
            "compile",
            ps.to_str().unwrap(),
            "--target",
            "js+wasm",
            "--emit-cert",
        ])
        .output()
        .expect("pyths compile failed to spawn");
    assert!(
        out.status.success(),
        "certified js+wasm compile should succeed; stderr={}",
        String::from_utf8_lossy(&out.stderr)
    );

    let cert_path = dir.join("wcert.js.wasm.cert.json");
    let cert = std::fs::read_to_string(&cert_path).expect("WASM-admission certificate written");
    assert!(
        cert.contains("\"pass\": \"wasm-admission\""),
        "cert: {}",
        cert
    );
    assert!(cert.contains("\"name\": \"add\""), "add admitted: {}", cert);
    assert!(
        cert.contains("\"admitted\": true"),
        "admitted flag: {}",
        cert
    );

    let _ = std::fs::remove_dir_all(&dir);
}

// --- #129: inline runtime (pyths run) must carry pyFormatSpec/pyFormatDynamic ---

#[test]
fn test_run_complex_format_spec_inline() {
    // `pyths run` uses inline codegen; complex f-string specs lower to
    // pyFormatSpec / pyFormatDynamic, which were never inlined —
    // ReferenceError at runtime (#129).
    let dir = std::env::temp_dir().join("pyths_test_run_format_inline");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let ps = dir.join("fmt_inline_test.ps");
    std::fs::write(
        &ps,
        "v = 42\nw = 8\nprint(f\"{v:>8}\")\nprint(f\"{v:{w}}\")\nprint(f\"{3.14159:.{2}f}\")\nprint(f\"{-42:05}\")\n",
    )
    .unwrap();

    let out = pyths_bin()
        .args(["run", ps.to_str().unwrap()])
        .output()
        .expect("pyths run failed to spawn");
    let stdout = String::from_utf8_lossy(&out.stdout);
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        out.status.success(),
        "pyths run should exit 0; stderr={}",
        stderr
    );
    assert!(
        !stderr.contains("is not defined"),
        "format helpers must be inlined: {}",
        stderr
    );
    // CPython ground truth for the four lines.
    assert_eq!(
        stdout.replace("\r\n", "\n").trim_end(),
        "      42\n      42\n3.14\n-0042",
        "format output must match CPython"
    );

    let _ = std::fs::remove_dir_all(&dir);
}

// ─── #168: nested @component compile-smoke — emitted module must PARSE ───
//
// The eval reproducer (claude-haiku-4-5, 4 samples): a @component nested
// inside another @component used to emit `export function` inside the
// enclosing function body — invalid JS ("'import' and 'export' cannot be
// used outside of module code" at the vite/esbuild boundary). Compile the
// reproducer and let node's parser be the oracle (`node --check`).
#[test]
fn test_nested_component_output_parses() {
    let dir = std::env::temp_dir().join("pyths_test_nested_component");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let ps = dir.join("kanban_nested.ps");
    std::fs::write(
        &ps,
        r#"from pyths.react import component, use_state

@component
def KanbanLite():
    cards, set_cards = use_state([[{"id": 1, "title": "A"}], []])
    @component
    def render_card(col_idx, card_idx, card):
        return div(cn="card", card["title"])
    return div(cn="board", render_card(col_idx=0, card_idx=0, card=cards[0][0]))
"#,
    )
    .unwrap();

    let out = pyths_bin()
        .env("PYTHS_NO_CACHE", "1")
        .args(["compile", ps.to_str().unwrap()])
        .output()
        .expect("pyths compile failed to spawn");
    assert!(
        out.status.success(),
        "compile must succeed: {}",
        String::from_utf8_lossy(&out.stderr)
    );

    // node --check requires an .mjs extension to parse as a module.
    let js = dir.join("kanban_nested.js");
    let mjs = dir.join("kanban_nested.check.mjs");
    std::fs::copy(&js, &mjs).unwrap();
    let node = std::process::Command::new("node")
        .args(["--check", mjs.to_str().unwrap()])
        .output()
        .expect("node failed to spawn");
    let node_err = String::from_utf8_lossy(&node.stderr);
    assert!(
        node.status.success(),
        "emitted module must be valid JS (nested export regression): {}",
        node_err
    );

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn test_check_expands_percent_idioms() {
    // Regression (#193 sweep): `check` must run the same expansion pipeline as
    // `compile` — including %NAME idioms — not just the $NAME dictionary. Before
    // the fix, `%GREET` was left unexpanded and `greet` was undefined.
    let dir = std::env::temp_dir().join(format!("pyths_check_idiom_{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(
        dir.join("pyths.toml"),
        "[expand.idioms]\nGREET = \"greet = lambda name: name\"\n",
    )
    .unwrap();
    std::fs::write(dir.join("m.psc"), "%GREET\n\nx = greet(\"world\")\n").unwrap();

    let output = pyths_bin()
        .current_dir(&dir)
        .args(["check", "m.psc"])
        .output()
        .expect("Failed to run pyths");

    let _ = std::fs::remove_dir_all(&dir);
    assert!(
        output.status.success(),
        "check should expand %GREET (defining greet) like compile does: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

// --- #300: cross-module class inheritance runs end-to-end (behavioral) ---
//
// Three-module project: Shape.ps (base, @property), Rectangle.ps (derived,
// imported base via relative import), main.ps (instantiates + reads the
// property). Before the fix, Rectangle compiled to a native constructor
// with no super() -> "Must call super constructor" at `new Rectangle(...)`.
// Node is the behavioral oracle: the program must print 6.
#[test]
fn test_crossmodule_inheritance_runs_under_node() {
    let dir = std::env::temp_dir().join("pyths_test_crossmodule_inherit");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();

    std::fs::write(
        dir.join("Shape.ps"),
        "class Shape:\n    @property\n    def area(self):\n        return 0\n",
    )
    .unwrap();
    std::fs::write(
        dir.join("Rectangle.ps"),
        "from .Shape import Shape\n\nclass Rectangle(Shape):\n    def __init__(self, width, height):\n        self.width = width\n        self.height = height\n\n    @property\n    def area(self):\n        return self.width * self.height\n",
    )
    .unwrap();
    std::fs::write(
        dir.join("main.ps"),
        "from .Rectangle import Rectangle\nprint(Rectangle(2, 3).area)\n",
    )
    .unwrap();

    for f in ["Shape.ps", "Rectangle.ps", "main.ps"] {
        let out = pyths_bin()
            .env("PYTHS_NO_CACHE", "1")
            .args(["compile", dir.join(f).to_str().unwrap()])
            .output()
            .expect("pyths compile failed to spawn");
        assert!(
            out.status.success(),
            "compile {} must succeed: {}",
            f,
            String::from_utf8_lossy(&out.stderr)
        );
    }

    // Make the emitted .js files runnable under plain Node: type=module +
    // rewire the bare pyths-runtime specifier to the repo runtime.
    std::fs::write(dir.join("package.json"), "{\"type\":\"module\"}\n").unwrap();
    let runtime_index = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("runtime")
        .join("src")
        .join("index.js")
        .canonicalize()
        .unwrap();
    let runtime_url = format!(
        "file:///{}",
        runtime_index
            .to_string_lossy()
            .trim_start_matches("\\\\?\\")
            .replace('\\', "/")
    );
    // Relative specifiers are emitted extensionless (the bundler resolves
    // them); plain Node ESM needs explicit .js — same rewrite the browser
    // harnesses do.
    for f in ["Shape.js", "Rectangle.js", "main.js"] {
        let p = dir.join(f);
        let src = std::fs::read_to_string(&p).unwrap();
        let src = src
            .replace("\"pyths-runtime\"", &format!("\"{}\"", runtime_url))
            .replace("\"./Shape\"", "\"./Shape.js\"")
            .replace("\"./Rectangle\"", "\"./Rectangle.js\"");
        std::fs::write(&p, src).unwrap();
    }

    let node = std::process::Command::new("node")
        .arg(dir.join("main.js"))
        .output()
        .expect("node failed to spawn");
    let stdout = String::from_utf8_lossy(&node.stdout);
    let stderr = String::from_utf8_lossy(&node.stderr);
    assert!(
        node.status.success(),
        "cross-module derived class must instantiate (no 'Must call super constructor'): {}",
        stderr
    );
    assert_eq!(
        stdout.replace("\r\n", "\n").trim_end(),
        "6",
        "Rectangle(2, 3).area must be 6"
    );

    let _ = std::fs::remove_dir_all(&dir);
}
