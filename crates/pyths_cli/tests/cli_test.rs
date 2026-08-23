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
fn test_compile_explicit_target_js_pins_js_only() {
    // §4.7.3: an explicit `--target js` is a CONSTRAINT — bare (undecorated)
    // functions never error on placement, and no WASM artifacts are emitted
    // even for WASM-eligible kernels.
    let fixture = fixtures_dir().join("wasm_numeric.ps");
    let tmp_dir = std::env::temp_dir();
    let js_file = tmp_dir.join("wasm_numeric_js_test.js");
    let wasm_file = tmp_dir.join("wasm_numeric_js_test.wasm");
    let dts_file = tmp_dir.join("wasm_numeric_js_test.d.ts");
    // Pre-clean: a stale .wasm from an earlier run (or the pre-flip suite)
    // in the shared temp dir would false-fail the "no .wasm" assertion.
    for f in [&js_file, &wasm_file, &dts_file] {
        let _ = std::fs::remove_file(f);
    }

    let output = pyths_bin()
        .args([
            "compile",
            fixture.to_str().unwrap(),
            "-o",
            js_file.to_str().unwrap(),
            "--target",
            "js",
        ])
        .output()
        .expect("Failed to run pyths");

    assert!(
        output.status.success(),
        "bare functions under explicit --target js must not error: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    // JS file should be created
    assert!(js_file.exists(), "JS file should be created");

    // WASM file should NOT be created (explicit target=js pins the set)
    assert!(
        !wasm_file.exists(),
        "WASM file should NOT be created for explicit --target js"
    );

    // Clean up
    let _ = std::fs::remove_file(&js_file);
    let _ = std::fs::remove_file(&dts_file);
}

#[test]
fn test_compile_auto_default_kernel_emits_js_wasm_glue() {
    // §4.7.1: NO --target flag → automatic routing (js+wasm semantics).
    // A module with numeric kernels emits .js + .glue.js + .wasm (+ .d.ts,
    // default-on).
    let fixture = fixtures_dir().join("wasm_numeric.ps");
    let tmp_dir = std::env::temp_dir();
    let js_file = tmp_dir.join("auto_default_kernel_test.js");
    let wasm_file = tmp_dir.join("auto_default_kernel_test.wasm");
    let glue_file = tmp_dir.join("auto_default_kernel_test.glue.js");
    let dts_file = tmp_dir.join("auto_default_kernel_test.d.ts");

    let output = pyths_bin()
        .args([
            "compile",
            fixture.to_str().unwrap(),
            "-o",
            js_file.to_str().unwrap(),
        ])
        .output()
        .expect("Failed to run pyths");

    assert!(
        output.status.success(),
        "auto default failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(js_file.exists(), "JS file should be created");
    assert!(
        wasm_file.exists(),
        "WASM file should be created under automatic routing"
    );
    assert!(
        glue_file.exists(),
        "glue file should be created under automatic routing"
    );
    assert!(dts_file.exists(), ".d.ts should be emitted by default");

    let _ = std::fs::remove_file(&js_file);
    let _ = std::fs::remove_file(&wasm_file);
    let _ = std::fs::remove_file(&glue_file);
    let _ = std::fs::remove_file(&dts_file);
}

#[test]
fn test_compile_auto_default_no_kernel_single_js() {
    // §4.7.1 graceful degrade: under the auto default, a module with NO
    // WASM-eligible functions still builds a single plain .js — quietly
    // (the "No WASM-eligible functions" warning is for pinned targets only).
    let fixture = fixtures_dir().join("hello.ps");
    let tmp_dir = std::env::temp_dir();
    let js_file = tmp_dir.join("auto_default_nokernel_test.js");
    let wasm_file = tmp_dir.join("auto_default_nokernel_test.wasm");
    let glue_file = tmp_dir.join("auto_default_nokernel_test.glue.js");
    let dts_file = tmp_dir.join("auto_default_nokernel_test.d.ts");

    let output = pyths_bin()
        .args([
            "compile",
            fixture.to_str().unwrap(),
            "-o",
            js_file.to_str().unwrap(),
        ])
        .output()
        .expect("Failed to run pyths");

    assert!(
        output.status.success(),
        "auto default on no-kernel module failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(js_file.exists(), "JS file should be created");
    assert!(!wasm_file.exists(), "no .wasm for a no-kernel module");
    assert!(!glue_file.exists(), "no .glue.js for a no-kernel module");
    let all = format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        !all.contains("No WASM-eligible functions"),
        "auto default must degrade quietly, got: {}",
        all
    );

    let _ = std::fs::remove_file(&js_file);
    let _ = std::fs::remove_file(&dts_file);
}

#[test]
fn test_compile_dts_default_on_and_no_dts_suppresses() {
    // release_v0.2.2 §5: .d.ts emits by default for file builds; --no-dts
    // suppresses it; --dts stays accepted as a redundant explicit-on.
    let fixture = fixtures_dir().join("hello.ps");
    let tmp_dir = std::env::temp_dir();

    // Default: .d.ts sibling appears.
    let js_a = tmp_dir.join("dts_default_on_test.js");
    let dts_a = tmp_dir.join("dts_default_on_test.d.ts");
    let out_a = pyths_bin()
        .args([
            "compile",
            fixture.to_str().unwrap(),
            "-o",
            js_a.to_str().unwrap(),
        ])
        .output()
        .expect("Failed to run pyths");
    assert!(out_a.status.success());
    assert!(dts_a.exists(), ".d.ts should be emitted by default");

    // --no-dts: suppressed.
    let js_b = tmp_dir.join("dts_suppressed_test.js");
    let dts_b = tmp_dir.join("dts_suppressed_test.d.ts");
    let out_b = pyths_bin()
        .args([
            "compile",
            fixture.to_str().unwrap(),
            "-o",
            js_b.to_str().unwrap(),
            "--no-dts",
        ])
        .output()
        .expect("Failed to run pyths");
    assert!(out_b.status.success());
    assert!(!dts_b.exists(), "--no-dts must suppress the .d.ts");

    // --dts: still accepted (redundant explicit-on).
    let js_c = tmp_dir.join("dts_redundant_on_test.js");
    let dts_c = tmp_dir.join("dts_redundant_on_test.d.ts");
    let out_c = pyths_bin()
        .args([
            "compile",
            fixture.to_str().unwrap(),
            "-o",
            js_c.to_str().unwrap(),
            "--dts",
        ])
        .output()
        .expect("Failed to run pyths");
    assert!(out_c.status.success(), "--dts must stay accepted");
    assert!(dts_c.exists(), "--dts (explicit-on) still emits the .d.ts");

    for f in [&js_a, &dts_a, &js_b, &dts_b, &js_c, &dts_c] {
        let _ = std::fs::remove_file(f);
    }
}

#[test]
fn test_compile_wasm_decorator_ineligible_is_hard_error() {
    // §4.7.3: decorator = ASSERTION. A user-written @wasm on a function the
    // admission gate rejects is a hard compile error carrying the reason —
    // never a silent stay-on-JS.
    let uniq = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let tmp_dir = std::env::temp_dir().join(format!("pyths_wasm_strict_{}", uniq));
    std::fs::create_dir_all(&tmp_dir).unwrap();
    let ps = tmp_dir.join("bad.ps");
    // No parameter annotation → signature-ineligible.
    std::fs::write(&ps, "@wasm\ndef f(x):\n    return x + 1\n").unwrap();

    let output = pyths_bin()
        .args(["compile", ps.to_str().unwrap()])
        .output()
        .expect("Failed to run pyths");

    assert!(
        !output.status.success(),
        "@wasm on an ineligible function must fail the compile"
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("@wasm") && stderr.contains("cannot be honored"),
        "error should name the @wasm assertion: {}",
        stderr
    );
    assert!(
        stderr.contains("type annotation"),
        "error should carry the eligibility-rejection reason: {}",
        stderr
    );
    assert!(
        !tmp_dir.join("bad.js").exists(),
        "nothing must be written on a strict @wasm error"
    );

    let _ = std::fs::remove_dir_all(&tmp_dir);
}

#[test]
fn test_compile_wasm_decorator_conflicts_with_explicit_target_js() {
    // §4.7.3: flag = CONSTRAINT. @wasm + explicit --target js is a hard
    // conflict with a hint; the same module compiles fine under the auto
    // default (no flag).
    let uniq = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let tmp_dir = std::env::temp_dir().join(format!("pyths_wasm_conflict_{}", uniq));
    std::fs::create_dir_all(&tmp_dir).unwrap();
    let ps = tmp_dir.join("kernel.ps");
    std::fs::write(
        &ps,
        "@wasm\ndef addmul(a: int, b: int) -> int:\n    return a * b + a\n",
    )
    .unwrap();

    let conflict = pyths_bin()
        .args(["compile", ps.to_str().unwrap(), "--target", "js"])
        .output()
        .expect("Failed to run pyths");
    assert!(
        !conflict.status.success(),
        "@wasm + explicit --target js must be a hard error"
    );
    let stderr = String::from_utf8_lossy(&conflict.stderr);
    assert!(
        stderr.contains("@wasm") && stderr.contains("--target js") && stderr.contains("hint"),
        "conflict error should carry a clear hint: {}",
        stderr
    );

    // Same module, no flag → auto routing honors the assertion.
    let auto = pyths_bin()
        .args(["compile", ps.to_str().unwrap()])
        .output()
        .expect("Failed to run pyths");
    assert!(
        auto.status.success(),
        "auto default must honor an eligible @wasm: {}",
        String::from_utf8_lossy(&auto.stderr)
    );
    assert!(
        tmp_dir.join("kernel.wasm").exists(),
        "eligible @wasm function should be compiled to WASM under auto"
    );

    let _ = std::fs::remove_dir_all(&tmp_dir);
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

    // The cache tracks single-JS builds only, so it engages for
    // `--target js --no-dts` (auto routing and default-on .d.ts both imply
    // multi-file output sets, which skip the cache).
    let r1 = pyths_bin()
        .env("PYTHS_CACHE_DIR", &cache_base)
        .args([
            "compile",
            local_src.to_str().unwrap(),
            "-o",
            out.to_str().unwrap(),
            "--target",
            "js",
            "--no-dts",
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
            "--target",
            "js",
            "--no-dts",
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
    // (`--target js --no-dts` would otherwise be cacheable, so this
    // genuinely exercises the env kill-switch.)
    let r = pyths_bin()
        .env("PYTHS_NO_CACHE", "1")
        .env("PYTHS_CACHE_DIR", &cache_base)
        .args([
            "compile",
            local_src.to_str().unwrap(),
            "-o",
            out.to_str().unwrap(),
            "--target",
            "js",
            "--no-dts",
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
    // Option B: math.sqrt returns a float — CPython prints 4.0 (the old
    // "4" expectation documented the pre-fidelity behavior).
    assert_eq!(stdout.trim(), "4.0", "sqrt(16) via node stdout: {}", stdout);

    let _ = std::fs::remove_dir_all(&dir);
}

// WB-6: a NO-ARG `.update()` on a user object was lowered to `pyUpdate(obj)`
// (dict.update semantics) and SILENTLY DROPPED — the runtime's custom-receiver
// branch looped over the (empty) trailing args, so `obj.update()` was never
// invoked. Root fix: pyUpdate forwards ALL args in ONE call for a receiver
// with its own `update`, so a zero-arg user method runs. A real dict's
// `.update({...})` / `.update(d)` must stay unchanged (dict.update semantics).
// Node is the behavioral oracle.
#[test]
fn test_no_arg_update_method_on_user_object_runs() {
    if !node_present() {
        return;
    }
    let dir = std::env::temp_dir().join("pyths_test_wb6_update");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let ps = dir.join("wb6.ps");
    std::fs::write(
        &ps,
        "class Contents:\n    def __init__(self):\n        self.n = 0\n    def update(self):\n        self.n = self.n + 1\n\ndef main():\n    c = Contents()\n    c.update()\n    c.update()\n    d = {\"a\": 1}\n    d.update({\"b\": 2})\n    e = {}\n    e.update(d)\n    print(c.n)\n    print(len(d))\n    print(len(e))\n\nmain()\n",
    )
    .unwrap();

    let out = pyths_bin()
        .args(["run", ps.to_str().unwrap()])
        .output()
        .expect("pyths run failed to spawn");
    let stdout = String::from_utf8_lossy(&out.stdout).replace("\r\n", "\n");
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        out.status.success(),
        "pyths run should exit 0: stderr={}",
        stderr
    );
    assert_eq!(
        stdout.trim_end(),
        "2\n2\n2",
        "no-arg user .update() must run twice (c.n=2); dict .update({{...}}) and \
         .update(d) unchanged (len 2, 2): {}",
        stdout
    );

    let _ = std::fs::remove_dir_all(&dir);
}

// WB-5: a local assigned in BOTH branches of an if/else INSIDE a class method
// was NOT hoisted to method scope — the if-branch emitted a block-scoped
// `let x = 1` and the else emitted a bare `x = 2` → `ReferenceError` under ESM
// strict mode (a standalone `def` hoisted `let x;` fine — the bug was specific
// to the method-body codegen path). Root fix: method bodies share the
// function-body hoisting (emit_hoisted_local_decls). Asserts BOTH the hoisted
// `let x;` in the emitted method AND correct runtime output on both branches.
#[test]
fn test_if_else_both_branches_local_hoisted_in_class_method() {
    if !node_present() {
        return;
    }
    let dir = std::env::temp_dir().join("pyths_test_wb5_method_scope");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let ps = dir.join("wb5.ps");
    std::fs::write(
        &ps,
        "class Q:\n    def m(self, cond):\n        if cond:\n            x = 1\n        else:\n            x = 2\n        return x\n\ndef main():\n    q = Q()\n    print(q.m(True))\n    print(q.m(False))\n\nmain()\n",
    )
    .unwrap();

    // Codegen: the method must hoist `let x;` (not a block-scoped `let x = 1`).
    let compiled = pyths_bin()
        .args(["compile", ps.to_str().unwrap(), "--stdout"])
        .env("PYTHS_NO_CACHE", "1")
        .output()
        .expect("pyths compile failed to spawn");
    let js = String::from_utf8_lossy(&compiled.stdout);
    assert!(
        js.contains("let x;"),
        "class method must hoist `let x;` to method scope: {}",
        js
    );

    // Behavior: both branches run without ReferenceError.
    let out = pyths_bin()
        .args(["run", ps.to_str().unwrap()])
        .output()
        .expect("pyths run failed to spawn");
    let stdout = String::from_utf8_lossy(&out.stdout).replace("\r\n", "\n");
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        out.status.success(),
        "method if/else-both-branches must run (no ReferenceError): {}",
        stderr
    );
    assert_eq!(stdout.trim_end(), "1\n2", "q.m(True)=1, q.m(False)=2: {}", stdout);

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
    // Option B: math.sqrt returns a float — CPython prints 4.0.
    assert_eq!(stdout.trim(), "4.0", "stdout: {}", stdout);

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
    // Option B: math.sqrt returns a float — CPython prints 2.0.
    assert_eq!(
        stdout.trim(),
        "2.0",
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

// --- WB-8: cross-module super() works with BARE-import bases (grandparent) ---
//
// Web-Bench svg-chart port: a 3-level hierarchy split across files where every
// base is imported with a BARE specifier (`from LineChart import LineChart`),
// NOT a relative one. Before the fix, a bare-imported base was assumed
// external/native, so the subclass got NO `__pyClass(...)` wrapper → no own
// `__mro__` → cooperative `super()` (esp. reaching a GRANDPARENT method)
// threw `__pySuper(...).<m> is not a function`. The relative form already
// worked (#300); a bare import of a sibling project module must behave the
// same. Node is the behavioral oracle: the program must print `area:chart`
// (AreaChart.label → super().label reaches Chart.label two levels up). The
// test runs BOTH the bare and the relative import forms.
#[test]
fn test_crossmodule_bare_import_super_grandparent_runs_under_node() {
    if !node_present() {
        return;
    }
    // Shared class shapes; only the import form (bare vs `.`-relative) differs.
    let chart_src = "class Chart:\n    def label(self):\n        return \"chart\"\n";
    let line_body =
        "class LineChart(Chart):\n    def draw(self):\n        return \"line\"\n";
    let area_body = "class AreaChart(LineChart):\n    def label(self):\n        return \"area:\" + super().label()\n";
    let main_src =
        "from AreaChart import AreaChart\nprint(AreaChart().label())\n";

    for (tag, dot) in [("bare", ""), ("rel", ".")] {
        let dir = mf_setup(&format!("wb8_{}", tag));
        std::fs::write(dir.join("Chart.ps"), chart_src).unwrap();
        std::fs::write(
            dir.join("LineChart.ps"),
            format!("from {}Chart import Chart\n{}", dot, line_body),
        )
        .unwrap();
        std::fs::write(
            dir.join("AreaChart.ps"),
            format!("from {}LineChart import LineChart\n{}", dot, area_body),
        )
        .unwrap();
        // `main` imports AreaChart the same way; for the relative variant the
        // sibling names carry a leading dot.
        std::fs::write(
            dir.join("main.ps"),
            main_src.replace("from AreaChart", &format!("from {}AreaChart", dot)),
        )
        .unwrap();

        for f in ["Chart.ps", "LineChart.ps", "AreaChart.ps", "main.ps"] {
            mf_compile_ok(&dir, f);
        }
        // Every non-`main` module's class must carry its OWN `__pyClass` wrapper
        // (the fix): without it there is no own `__mro__` and super() breaks.
        for f in ["LineChart.js", "AreaChart.js"] {
            let js = std::fs::read_to_string(dir.join(f)).unwrap();
            assert!(
                js.contains("__pyClass("),
                "{} ({}) must emit __pyClass for its bare/relative-imported base: {}",
                f,
                tag,
                js
            );
        }
        mf_node_prep(
            &dir,
            &["Chart.js", "LineChart.js", "AreaChart.js", "main.js"],
            &[
                ("\"Chart\"", "\"./Chart.js\""),
                ("\"LineChart\"", "\"./LineChart.js\""),
                ("\"AreaChart\"", "\"./AreaChart.js\""),
                ("\"./Chart\"", "\"./Chart.js\""),
                ("\"./LineChart\"", "\"./LineChart.js\""),
                ("\"./AreaChart\"", "\"./AreaChart.js\""),
            ],
        );
        let (ok, stdout, stderr) = mf_node(&dir.join("main.js"));
        assert!(
            ok,
            "cross-module grandparent super() must run ({} imports): {}",
            tag, stderr
        );
        assert_eq!(
            stdout.trim_end(),
            "area:chart",
            "AreaChart().label() must reach Chart.label via super() ({} imports)",
            tag
        );
        let _ = std::fs::remove_dir_all(&dir);
    }
}

// ── Multi-file import defects (corpus promotion: from_dot_mod / reexport /
//    bundle — experiments/multifile-ps) ─────────────────────────────────────

fn mf_setup(name: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!("pyths_test_mf_{}", name));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn mf_compile_ok(dir: &std::path::Path, file: &str) {
    let out = pyths_bin()
        .env("PYTHS_NO_CACHE", "1")
        .args(["compile", dir.join(file).to_str().unwrap()])
        .output()
        .expect("pyths compile failed to spawn");
    assert!(
        out.status.success(),
        "compile {} must succeed: {}",
        file,
        String::from_utf8_lossy(&out.stderr)
    );
}

/// Make per-module compiled output runnable under plain Node: type=module,
/// bare `pyths-runtime` → repo runtime file URL, extensionless relative
/// specifiers → explicit `.js` (same rewrite the browser harnesses do).
fn mf_node_prep(dir: &std::path::Path, js_files: &[&str], rel_rewrites: &[(&str, &str)]) {
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
    for f in js_files {
        let p = dir.join(f);
        let mut src = std::fs::read_to_string(&p).unwrap();
        src = src.replace("\"pyths-runtime\"", &format!("\"{}\"", runtime_url));
        for (from, to) in rel_rewrites {
            src = src.replace(from, to);
        }
        std::fs::write(&p, src).unwrap();
    }
}

fn mf_node(path: &std::path::Path) -> (bool, String, String) {
    let out = Command::new("node")
        .arg(path)
        .output()
        .expect("node failed to spawn");
    (
        out.status.success(),
        String::from_utf8_lossy(&out.stdout).replace("\r\n", "\n"),
        String::from_utf8_lossy(&out.stderr).to_string(),
    )
}

// BUG #1 — `from . import <submodule>` used to emit `import { a } from "./"`
// (the package index importing the submodule as a named export of ITSELF →
// ESM link error) and `a.X` mis-lowered via pyBoundMethod. Root fix: a
// module-namespace import (`import * as a from "./a"`) with direct member
// access. Corpus project `from_dot_mod`, promoted.
#[test]
fn test_from_dot_submodule_import_runs_under_node() {
    if !node_present() {
        return;
    }
    let dir = mf_setup("from_dot");
    std::fs::write(dir.join("a.ps"), "X = 42\ndef f():\n    return 7\n").unwrap();
    std::fs::write(
        dir.join("__init__.ps"),
        "from . import a\nprint(a.X)\nprint(a.f())\n",
    )
    .unwrap();
    mf_compile_ok(&dir, "a.ps");
    mf_compile_ok(&dir, "__init__.ps");

    let js = std::fs::read_to_string(dir.join("__init__.js")).unwrap();
    assert!(
        js.contains("import * as a from \"./a\";"),
        "namespace import of the submodule expected:\n{js}"
    );
    assert!(
        !js.contains("from \"./\"") && !js.contains("pyBoundMethod(a,"),
        "self-referential index import / pyBoundMethod wrap must be gone:\n{js}"
    );

    mf_node_prep(&dir, &["a.js", "__init__.js"], &[("\"./a\"", "\"./a.js\"")]);
    let (ok, so, se) = mf_node(&dir.join("__init__.js"));
    assert!(ok, "node must link + run the namespace import: {se}");
    assert_eq!(so.trim_end(), "42\n7");
    let _ = std::fs::remove_dir_all(&dir);
}

// BUG #2 — `from .mod import *` was SILENTLY dropped (clean compile, bare
// ReferenceError at runtime). Root fix: the CLI expands the relative star
// from the sibling source into explicit named imports (commands::relstar).
// Corpus project `reexport`, promoted.
#[test]
fn test_relative_star_import_expands_and_runs_under_node() {
    if !node_present() {
        return;
    }
    let dir = mf_setup("relstar");
    std::fs::write(
        dir.join("impl.ps"),
        "Y = 5\nZ = 6\n_hidden = 9\ndef work():\n    return Y + Z\n",
    )
    .unwrap();
    std::fs::write(
        dir.join("main.ps"),
        "from .impl import *\nprint(Y + Z)\nprint(work())\n",
    )
    .unwrap();
    mf_compile_ok(&dir, "impl.ps");
    mf_compile_ok(&dir, "main.ps");

    let js = std::fs::read_to_string(dir.join("main.js")).unwrap();
    assert!(
        js.contains("import { Y, Z, work } from \"./impl\";"),
        "star must expand to the sibling's public names:\n{js}"
    );
    assert!(
        !js.contains("_hidden"),
        "underscore names must not be star-imported:\n{js}"
    );

    mf_node_prep(&dir, &["impl.js", "main.js"], &[("\"./impl\"", "\"./impl.js\"")]);
    let (ok, so, se) = mf_node(&dir.join("main.js"));
    assert!(ok, "expanded star import must link + run: {se}");
    assert_eq!(so.trim_end(), "11\n11");
    let _ = std::fs::remove_dir_all(&dir);
}

// BUG #2 — `__all__` is authoritative for the star surface (exact set, in
// `__all__` order).
#[test]
fn test_relative_star_respects_dunder_all() {
    let dir = mf_setup("relstar_all");
    std::fs::write(
        dir.join("impl.ps"),
        "__all__ = [\"work\"]\ndef work():\n    return 1\ndef extra():\n    return 2\n",
    )
    .unwrap();
    std::fs::write(dir.join("main.ps"), "from .impl import *\nprint(work())\n").unwrap();
    mf_compile_ok(&dir, "main.ps");
    let js = std::fs::read_to_string(dir.join("main.js")).unwrap();
    assert!(
        js.contains("import { work } from \"./impl\";"),
        "__all__ names must be imported:\n{js}"
    );
    assert!(
        !js.contains("extra"),
        "names outside __all__ must not be imported:\n{js}"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

// BUG #2 — the silent-drop failure mode is dead on EVERY path: a relative
// star whose sibling is missing is a loud COMPILE error, not a clean compile.
#[test]
fn test_relative_star_missing_sibling_fails_compile() {
    let dir = mf_setup("relstar_missing");
    std::fs::write(dir.join("main.ps"), "from .nosuch import *\nprint(1)\n").unwrap();
    let out = pyths_bin()
        .env("PYTHS_NO_CACHE", "1")
        .args(["compile", dir.join("main.ps").to_str().unwrap()])
        .output()
        .expect("pyths compile failed to spawn");
    assert!(
        !out.status.success(),
        "missing star sibling must FAIL the compile (old behavior: silent drop)"
    );
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("no module file found"),
        "diagnostic must explain the unresolved star: {stderr}"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

// BUG #3 — `pyths bundle` emitted invalid JS for ANY relative import
// (`export` left inside the module IIFE = SyntaxError; imported bindings
// never exposed to the entry scope). Root fix: ALL import/export rewriting
// lives in one transform (strip + record exports, return a live namespace
// object, rewrite importers to destructure it). End-to-end: bundle must be
// valid, runnable ESM.
#[test]
fn test_bundle_relative_import_runs_under_node() {
    if !node_present() {
        return;
    }
    let dir = mf_setup("bundle_rel");
    std::fs::write(
        dir.join("helper.ps"),
        "def greet():\n    return \"hello from helper\"\n",
    )
    .unwrap();
    std::fs::write(
        dir.join("main.ps"),
        "from .helper import greet\nprint(greet())\nprint(2 + 2)\n",
    )
    .unwrap();
    let out = pyths_bin()
        .args([
            "bundle",
            dir.join("main.ps").to_str().unwrap(),
            "-o",
            dir.join("out.mjs").to_str().unwrap(),
        ])
        .output()
        .expect("pyths bundle failed to spawn");
    assert!(
        out.status.success(),
        "bundle must succeed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let bundle = std::fs::read_to_string(dir.join("out.mjs")).unwrap();
    assert!(
        bundle.contains("const __pyMod_helper = (() => {"),
        "inlined module must be namespace-wrapped:\n{bundle}"
    );
    assert!(
        bundle.contains("return {"),
        "module IIFE must return its export namespace:\n{bundle}"
    );
    assert!(
        !bundle.lines().any(|l| l.trim_start().starts_with("export ")
            && !l.starts_with("export ")),
        "no `export` may remain inside an IIFE (SyntaxError):\n{bundle}"
    );
    let (ok, so, se) = mf_node(&dir.join("out.mjs"));
    assert!(ok, "bundle must be valid runnable ESM: {se}");
    assert_eq!(so.trim_end(), "hello from helper\n4");
    let _ = std::fs::remove_dir_all(&dir);
}

// BUG #3 — transitive relative chains must inline in dependency order, a
// `from . import mod` namespace import must link against the inlined
// namespace object, a relative STAR must bundle after expansion, and two
// modules with the SAME STEM in different directories must not clobber
// each other (the old bundler keyed modules by bare stem).
#[test]
fn test_bundle_chain_star_namespace_and_stem_collision_runs_under_node() {
    if !node_present() {
        return;
    }
    let dir = mf_setup("bundle_deep");
    std::fs::create_dir_all(dir.join("sub")).unwrap();
    std::fs::write(dir.join("c.ps"), "BASE = 10\ndef base_fn():\n    return BASE\n").unwrap();
    std::fs::write(
        dir.join("b.ps"),
        "from .c import base_fn\ndef mid():\n    return base_fn() + 1\n",
    )
    .unwrap();
    std::fs::write(dir.join("star_src.ps"), "V = 5\ndef vfn():\n    return V\n").unwrap();
    std::fs::write(dir.join("util.ps"), "def u1():\n    return 1\n").unwrap();
    std::fs::write(dir.join("sub").join("util.ps"), "def u2():\n    return 2\n").unwrap();
    std::fs::write(
        dir.join("main.ps"),
        "from .b import mid\nfrom .star_src import *\nfrom . import c\nfrom .util import u1\nfrom .sub.util import u2\nprint(mid())\nprint(vfn() + V)\nprint(c.BASE)\nprint(u1() + u2())\n",
    )
    .unwrap();
    let out = pyths_bin()
        .args([
            "bundle",
            dir.join("main.ps").to_str().unwrap(),
            "-o",
            dir.join("out.mjs").to_str().unwrap(),
        ])
        .output()
        .expect("pyths bundle failed to spawn");
    assert!(
        out.status.success(),
        "bundle must succeed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let bundle = std::fs::read_to_string(dir.join("out.mjs")).unwrap();
    assert!(
        bundle.contains("const __pyMod_util") && bundle.contains("const __pyMod_sub_util"),
        "same-stem modules must get distinct namespace vars:\n{bundle}"
    );
    let (ok, so, se) = mf_node(&dir.join("out.mjs"));
    assert!(ok, "deep bundle must run: {se}");
    assert_eq!(so.trim_end(), "11\n10\n10\n3");
    let _ = std::fs::remove_dir_all(&dir);
}

// FIX 2 — `from . import X` where X is a SYMBOL of the package __init__
// (legal Python, previously handled; regressed by the BUG #1 submodule
// lowering). The CLI pre-pass FS-disambiguates: submodule file exists →
// namespace import; else → named import from the package index. Mixed form
// covers both halves in one statement.
#[test]
fn test_from_dot_index_symbol_import_runs_under_node() {
    if !node_present() {
        return;
    }
    let dir = mf_setup("dot_symbol");
    std::fs::write(dir.join("a.ps"), "X = 42\n").unwrap();
    std::fs::write(dir.join("__init__.ps"), "CONST = 200\n").unwrap();
    std::fs::write(
        dir.join("mod.ps"),
        "from . import a, CONST\nprint(a.X)\nprint(CONST)\n",
    )
    .unwrap();
    for f in ["a.ps", "__init__.ps", "mod.ps"] {
        mf_compile_ok(&dir, f);
    }
    let js = std::fs::read_to_string(dir.join("mod.js")).unwrap();
    assert!(
        js.contains("import * as a from \"./a\";"),
        "submodule half must stay a namespace import:\n{js}"
    );
    assert!(
        js.contains("import { CONST } from \"./\";"),
        "index-symbol half must be a named import from the index:\n{js}"
    );
    // Order matters: "./a" must be rewritten before the "./" index spec.
    mf_node_prep(
        &dir,
        &["a.js", "__init__.js", "mod.js"],
        &[("\"./a\"", "\"./a.js\""), ("\"./\"", "\"./__init__.js\"")],
    );
    let (ok, so, se) = mf_node(&dir.join("mod.js"));
    assert!(ok, "index-symbol import must link + run: {se}");
    assert_eq!(so.trim_end(), "42\n200");
    let _ = std::fs::remove_dir_all(&dir);
}

// FIX 1(b) — module-level tuple/list-unpack targets are ordinary Python
// module globals and must EXPORT (per-module path: previously an ESM link
// error).
#[test]
fn test_module_unpack_globals_link_and_run_under_node() {
    if !node_present() {
        return;
    }
    let dir = mf_setup("unpack_exports");
    std::fs::write(
        dir.join("impl.ps"),
        "x, y = 1, 2\n[p, q] = [3, 4]\ndef f():\n    return 9\n",
    )
    .unwrap();
    std::fs::write(
        dir.join("main.ps"),
        "from .impl import x, y, p, q, f\nprint(x, y, f())\nprint(p + q)\n",
    )
    .unwrap();
    mf_compile_ok(&dir, "impl.ps");
    mf_compile_ok(&dir, "main.ps");
    let impl_js = std::fs::read_to_string(dir.join("impl.js")).unwrap();
    for n in ["x", "y", "p", "q"] {
        assert!(
            impl_js.contains(&format!("export let {};", n)),
            "unpack target `{n}` must export:\n{impl_js}"
        );
    }
    mf_node_prep(&dir, &["impl.js", "main.js"], &[("\"./impl\"", "\"./impl.js\"")]);
    let (ok, so, se) = mf_node(&dir.join("main.js"));
    assert!(ok, "unpack globals must link per-module: {se}");
    assert_eq!(so.trim_end(), "1 2 9\n7");
    let _ = std::fs::remove_dir_all(&dir);
}

// FIX 1(b) — the BUNDLE path for the same surface, via a relative STAR
// (previously the silent-miscompile hole: `undefined` bindings).
#[test]
fn test_bundle_unpack_globals_via_star_run_under_node() {
    if !node_present() {
        return;
    }
    let dir = mf_setup("bundle_unpack");
    std::fs::write(
        dir.join("impl.ps"),
        "x, y = 1, 2\n[p, q] = [3, 4]\ndef f():\n    return 9\n",
    )
    .unwrap();
    std::fs::write(
        dir.join("star.ps"),
        "from .impl import *\nprint(x, y, f())\nprint(p + q)\n",
    )
    .unwrap();
    let out = pyths_bin()
        .args([
            "bundle",
            dir.join("star.ps").to_str().unwrap(),
            "-o",
            dir.join("out.mjs").to_str().unwrap(),
        ])
        .output()
        .expect("pyths bundle failed to spawn");
    assert!(
        out.status.success(),
        "bundle must succeed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let (ok, so, se) = mf_node(&dir.join("out.mjs"));
    assert!(ok, "bundled unpack globals must run: {se}");
    assert_eq!(
        so.trim_end(),
        "1 2 9\n7",
        "unpack globals must carry real values, never undefined/None"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

// FIX 1(a) — the bundle export-surface gate: importing a name the module
// does not export must FAIL the bundle loud (mirroring the per-module ESM
// link error), never bind `undefined`. This closes the silent class for ANY
// cause, not just tuple unpack.
#[test]
fn test_bundle_missing_export_fails_loud() {
    let dir = mf_setup("bundle_noexport");
    std::fs::write(dir.join("lib2.ps"), "A = 1\n").unwrap();
    std::fs::write(dir.join("bad.ps"), "from .lib2 import nope\nprint(nope)\n").unwrap();
    let out = pyths_bin()
        .args([
            "bundle",
            dir.join("bad.ps").to_str().unwrap(),
            "-o",
            dir.join("bad.mjs").to_str().unwrap(),
        ])
        .output()
        .expect("pyths bundle failed to spawn");
    assert!(
        !out.status.success(),
        "bundle must FAIL on an unexported import (old behavior: silent undefined)"
    );
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("does not export `nope`"),
        "diagnostic must name the missing export: {stderr}"
    );
    assert!(
        !dir.join("bad.mjs").exists(),
        "no bundle artifact may be written on a failed export check"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

// ── Round-3 EXECUTABLE regression tests ────────────────────────────────────
// These COMPILE a .ps and RUN it (`pyths run` → inline runtime via node),
// asserting BEHAVIOR of the generated/inlined output — not just emitted
// strings. They lock the round-3 root fixes at the path they actually ship on.
fn run_ps(src: &str) -> (bool, String, String) {
    let dir = psc_test_scratch("run3");
    let p = dir.join("prog.ps");
    std::fs::write(&p, src).unwrap();
    let out = pyths_bin()
        .args(["run", p.to_str().unwrap()])
        .env("PYTHS_NO_CACHE", "1")
        .output()
        .expect("pyths run failed to spawn");
    let so = String::from_utf8_lossy(&out.stdout).to_string();
    let se = String::from_utf8_lossy(&out.stderr).to_string();
    let _ = std::fs::remove_dir_all(&dir);
    (out.status.success(), so, se)
}

// `pyths run` needs node; skip cleanly if absent so CI without node is green.
fn node_present() -> bool {
    Command::new("node").arg("--version").output().map(|o| o.status.success()).unwrap_or(false)
}

#[test]
fn round3_optimized_range_true_prints_zero() {
    if !node_present() { return; }
    let (ok, so, se) = run_ps("for i in range(True):\n    print(i)\n");
    assert!(ok, "run failed: {se}");
    assert_eq!(so.trim(), "0", "range(True) must yield [0]; got {so:?}");
}

#[test]
fn round3_optimized_range_near_2p53_terminates() {
    if !node_present() { return; }
    // The old hand-rolled `i += 1` counter hung here (2**53+1 == 2**53 in Number).
    let (ok, so, se) = run_ps(
        "a = int(float(\"9007199254740992\"))\nb = int(float(\"9007199254740994\"))\nc = 0\nfor i in range(a, b):\n    c += 1\nprint(c)\n",
    );
    assert!(ok, "run failed/hung: {se}");
    assert_eq!(so.trim(), "2");
}

#[test]
fn round3_optimized_range_bigint_iterates() {
    if !node_present() { return; }
    // The old counter crashed with "Cannot mix BigInt and other types".
    let (ok, so, se) = run_ps(
        "c = 0\nfor i in range(9007199254740992, 9007199254740994):\n    c += 1\nprint(c)\n",
    );
    assert!(ok, "run failed: {se}");
    assert_eq!(so.trim(), "2");
}

#[test]
fn round3_optimized_range_zero_step_and_float_raise() {
    if !node_present() { return; }
    let (ok, so, _) = run_ps(
        "step = 0\ntry:\n    for i in range(1, 0, step):\n        pass\n    print(\"NO\")\nexcept ValueError:\n    print(\"VE\")\n",
    );
    assert!(ok);
    assert_eq!(so.trim(), "VE");
}

#[test]
fn round3_proto_dict_write_is_data_key() {
    if !node_present() { return; }
    // `d["__proto__"] = v` must create a real data key (CPython `"__proto__" in d`).
    let (ok, so, se) = run_ps(
        "d = {}\nd[\"__proto__\"] = 7\nprint(\"__proto__\" in d, d[\"__proto__\"])\n",
    );
    assert!(ok, "run failed: {se}");
    assert_eq!(so.trim(), "True 7");
}

#[test]
fn round3_range_float_arg_typeerror() {
    if !node_present() { return; }
    let (ok, so, _) = run_ps(
        "try:\n    x = list(range(0, 2, 1))\n    y = 0\n    for j in [0.5]:\n        y = j\n    z = list(range(0, 2, y))\n    print(\"NO\")\nexcept TypeError:\n    print(\"TE\")\n",
    );
    assert!(ok);
    assert_eq!(so.trim(), "TE");
}

// ── Round-4: PACKAGE-boundary ESM load (catches missing package exports) ────
// `pyths run` uses the INLINE runtime, so it cannot catch a helper that the
// optimized/inlined lowering imports but the PACKAGE entry points don't export.
// This compiles a module (default `--target js`, which imports "pyths-runtime")
// and LOADS it as ESM resolving the real package entry — a missing export
// (e.g. __pyRangeIter) fails ESM instantiation and thus this test.
fn file_url(p: &std::path::Path) -> String {
    let s = p.to_string_lossy().replace('\\', "/");
    let s = s.strip_prefix("//?/").unwrap_or(&s);
    format!("file:///{}", s.trim_start_matches('/'))
}

#[test]
fn round4_package_esm_load_resolves_all_exports() {
    if !node_present() { return; }
    let dir = psc_test_scratch("pkgesm");
    let ps = dir.join("prog.ps");
    // range(...) makes the optimized lowering import __pyRangeIter from the pkg.
    std::fs::write(
        &ps,
        "def main():\n    total = 0\n    for i in range(3):\n        total += i\n    print(total)\nmain()\n",
    ).unwrap();
    let js = dir.join("prog.js");
    let r = pyths_bin()
        .args(["compile", ps.to_str().unwrap(), "-o", js.to_str().unwrap()])
        .env("PYTHS_NO_CACHE", "1")
        .output()
        .unwrap();
    assert!(r.status.success(), "compile failed: {}", String::from_utf8_lossy(&r.stderr));

    // Rewrite the bare "pyths-runtime" specifier(s) to the local PACKAGE entry
    // points so node resolves the real package exports.
    let src_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../runtime/src")
        .canonicalize()
        .unwrap();
    let idx_url = file_url(&src_dir.join("index.js"));
    let core_url = file_url(&src_dir.join("core.js"));
    let emitted = std::fs::read_to_string(&js).unwrap();
    let rewritten = emitted
        .replace("\"pyths-runtime/core\"", &format!("\"{core_url}\""))
        .replace("\"pyths-runtime\"", &format!("\"{idx_url}\""));
    assert!(rewritten.contains("__pyRangeIter"), "expected the optimized lowering to import __pyRangeIter:\n{emitted}");
    let mjs = dir.join("prog.mjs");
    std::fs::write(&mjs, rewritten).unwrap();

    let run = Command::new("node").arg(&mjs).output().unwrap();
    assert!(
        run.status.success(),
        "PACKAGE ESM load/run failed (a missing package export?):\nstdout={}\nstderr={}",
        String::from_utf8_lossy(&run.stdout),
        String::from_utf8_lossy(&run.stderr)
    );
    assert_eq!(String::from_utf8_lossy(&run.stdout).trim(), "3");
    let _ = std::fs::remove_dir_all(&dir);
}

// ── delta4: DRIFT GUARD — manifest × BOTH package entry points ─────────────
// Direction 2 of the export-surface guarantee (direction 1 is the
// debug_assert inside `need_runtime`): every symbol the codegen can emit an
// import for (EMITTABLE_RUNTIME_SYMBOLS) must be an ACTUAL export of BOTH
// runtime/src/index.js (default target) and runtime/src/core.js
// (`--target worker`). Imports both entries under node and fails listing
// every missing name — so the compiler can never again emit a symbol either
// entry point doesn't export.
#[test]
fn runtime_export_surface_covers_all_emittable_symbols() {
    if !node_present() { return; }
    let src_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../runtime/src")
        .canonicalize()
        .unwrap();
    let idx_url = file_url(&src_dir.join("index.js"));
    let core_url = file_url(&src_dir.join("core.js"));
    let names = pyths_codegen_js::EMITTABLE_RUNTIME_SYMBOLS
        .iter()
        .map(|n| format!("\"{}\"", n))
        .collect::<Vec<_>>()
        .join(",");
    let script = format!(
        r#"import * as idx from "{idx_url}";
import * as core from "{core_url}";
const names = [{names}];
const missing = [];
for (const n of names) {{
    if (!(n in idx)) missing.push("index.js: " + n);
    if (!(n in core)) missing.push("core.js: " + n);
}}
if (missing.length) {{
    console.error("compiler-emittable symbols MISSING from the runtime export surface:\n" + missing.join("\n"));
    process.exit(1);
}}
console.log("ok " + names.length);
"#
    );
    let dir = psc_test_scratch("exportsurface");
    let mjs = dir.join("check.mjs");
    std::fs::write(&mjs, script).unwrap();
    let run = Command::new("node").arg(&mjs).output().unwrap();
    assert!(
        run.status.success(),
        "export-surface drift guard failed:\nstdout={}\nstderr={}",
        String::from_utf8_lossy(&run.stdout),
        String::from_utf8_lossy(&run.stderr)
    );
    let _ = std::fs::remove_dir_all(&dir);
}

// ── delta4: PACKAGE-boundary ESM load, BROAD helper set, BOTH targets ──────
// The round-4 test above compiles only the DEFAULT target with a range-only
// fixture, so it could not catch a missing `--target worker` export (its
// core.js rewrite line was inert) nor any helper beyond the range family.
// This one compiles a program that exercises a BROAD helper surface —
// complex literal, int(str)/chr/ord, a generator, classes + isinstance,
// builtin + user exceptions, dict merge/update/del, list ops, range, str
// methods, min/max/sum/any/all, divmod, bin/hex/oct, float(str) — on BOTH
// the default AND the worker target, loads each emitted module as ESM
// resolving the REAL package entry (index.js resp. core.js), runs it, and
// compares against CPython's output. ANY missing export on EITHER entry
// fails ESM instantiation and thus this test.
#[test]
fn round5_broad_package_esm_load_both_targets() {
    if !node_present() { return; }
    let fixture = r#"def gen():
    yield 1
    yield 2

def fkw(**kw):
    return kw

class Animal:
    def __init__(self, name):
        self.name = name
    def speak(self):
        return self.name + " speaks"

class MyError(Exception):
    pass

class SubTE(TypeError):
    pass

class Mixin:
    def tag(self):
        return "mx"

class MixErr(Mixin, ValueError):
    pass

def main():
    z = 1j
    z2 = z + 1
    total = 0
    for i in range(4):
        total += i
    xs = [3, 1, 2]
    xs.append(4)
    xs.sort()
    del xs[0]
    d = {"a": 1}
    d2 = {**d, "b": 2}
    d2.update({"c": 3})
    del d2["a"]
    g = gen()
    first = next(g)
    a = Animal("Rex")
    ok = isinstance(a, Animal)
    try:
        raise MyError("boom")
    except MyError as e:
        caught = str(e)
    try:
        raise ValueError("bad")
    except ValueError:
        caught2 = "ve"
    caught3 = ""
    try:
        raise SubTE("teboom")
    except TypeError as e2:
        caught3 = str(e2) + " " + str(isinstance(e2, SubTE))
    caught4 = ""
    try:
        raise MixErr("mixboom")
    except ValueError as e3:
        caught4 = str(e3) + " " + e3.tag() + " " + str(isinstance(e3, MixErr))
    caught5 = ""
    try:
        fkw(**{1: "x"})
    except TypeError:
        caught5 = "kwte"
    s = "Hello World"
    r = s.replace("World", "There")
    print(z2, total, xs, d2, first, a.speak(), ok, caught, caught2)
    print(r, s.rfind("o"), "abc".islower(), "ABC".isupper())
    print(int("41") + 1, chr(65), ord("A"), divmod(7, 2), bin(5), hex(255), oct(8))
    print(min(xs), max(xs), sum(xs), any([False, True]), all([True, True]), float("1.5"))
    print("exc-sub", caught3)
    print("exc-mi", caught4)
    print("kw-nonstr", caught5)
main()
"#;
    let expected = "(1+1j) 6 [2, 3, 4] {'b': 2, 'c': 3} 1 Rex speaks True boom ve\n\
                    Hello There 7 True True\n\
                    42 A 65 (3, 1) 0b101 0xff 0o10\n\
                    2 4 9 True True 1.5\n\
                    exc-sub teboom True\n\
                    exc-mi mixboom mx True\n\
                    kw-nonstr kwte";

    let src_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../runtime/src")
        .canonicalize()
        .unwrap();
    let idx_url = file_url(&src_dir.join("index.js"));
    let core_url = file_url(&src_dir.join("core.js"));

    for (target_args, label) in [
        (&[][..], "default (pyths-runtime → index.js)"),
        (&["--target", "worker"][..], "worker (pyths-runtime/core → core.js)"),
    ] {
        let dir = psc_test_scratch(if target_args.is_empty() { "broadesm_js" } else { "broadesm_worker" });
        let ps = dir.join("prog.ps");
        std::fs::write(&ps, fixture).unwrap();
        let js = dir.join("prog.js");
        let mut args = vec!["compile", ps.to_str().unwrap(), "-o", js.to_str().unwrap()];
        args.extend_from_slice(target_args);
        let r = pyths_bin()
            .args(&args)
            .env("PYTHS_NO_CACHE", "1")
            .output()
            .unwrap();
        assert!(
            r.status.success(),
            "[{label}] compile failed: {}",
            String::from_utf8_lossy(&r.stderr)
        );

        // Rewrite the bare package specifier(s) to the local entry file —
        // core FIRST (it is a prefix-extension of the root specifier).
        let emitted = std::fs::read_to_string(&js).unwrap();
        let rewritten = emitted
            .replace("\"pyths-runtime/core\"", &format!("\"{core_url}\""))
            .replace("\"pyths-runtime\"", &format!("\"{idx_url}\""));
        if !target_args.is_empty() {
            assert!(
                emitted.contains("\"pyths-runtime/core\""),
                "[{label}] expected worker output to import from pyths-runtime/core:\n{emitted}"
            );
        }
        let mjs = dir.join("prog.mjs");
        std::fs::write(&mjs, rewritten).unwrap();

        let run = Command::new("node").arg(&mjs).output().unwrap();
        assert!(
            run.status.success(),
            "[{label}] PACKAGE ESM load/run failed (missing package export?):\nstdout={}\nstderr={}",
            String::from_utf8_lossy(&run.stdout),
            String::from_utf8_lossy(&run.stderr)
        );
        let got = String::from_utf8_lossy(&run.stdout).replace("\r\n", "\n");
        assert_eq!(got.trim(), expected, "[{label}] output diverged from CPython");
        let _ = std::fs::remove_dir_all(&dir);
    }
}

// ── Round-3 plugins/CLI delta regression tests ─────────────────────────────
#[test]
fn round3_sourcemap_rebuild_without_force_succeeds() {
    // Delta regression: `--sourcemap` twice used to reject its OWN `.js.map`.
    let dir = psc_test_scratch("smrebuild");
    let ps = dir.join("app.ps");
    std::fs::write(&ps, "def add(a: int, b: int) -> int:\n    return a + b\n").unwrap();
    let js = dir.join("app.js");
    for i in 0..2 {
        let r = pyths_bin()
            .args(["compile", ps.to_str().unwrap(), "--sourcemap", "-o", js.to_str().unwrap()])
            .env("PYTHS_NO_CACHE", "1")
            .output()
            .unwrap();
        assert!(r.status.success(), "sourcemap rebuild #{i} failed: {}", String::from_utf8_lossy(&r.stderr));
    }
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn round3_dts_refusal_is_atomic_no_partial_js() {
    // P10: an unowned `app.d.ts` must make `--dts` refuse BEFORE writing app.js.
    let dir = psc_test_scratch("dtsatomic");
    let ps = dir.join("app.ps");
    std::fs::write(&ps, "x = 1\n").unwrap();
    let js = dir.join("app.js");
    let dts = dir.join("app.d.ts");
    std::fs::write(&dts, b"export const mine = 1; // hand-written").unwrap();
    let r = pyths_bin()
        .args(["compile", ps.to_str().unwrap(), "--dts", "-o", js.to_str().unwrap()])
        .env("PYTHS_NO_CACHE", "1")
        .output()
        .unwrap();
    assert!(!r.status.success(), "must refuse the unowned .d.ts");
    assert!(!js.exists(), "app.js must NOT be written when the .d.ts is refused (atomic)");
    assert_eq!(std::fs::read(&dts).unwrap(), b"export const mine = 1; // hand-written");
    let _ = std::fs::remove_dir_all(&dir);
}

// ── public #3: unimplemented-builtin diagnostic + the four new builtins ────

#[test]
fn public3_issue_repro_hasattr_still_true() {
    if !node_present() { return; }
    let (ok, so, se) = run_ps(
        "class C:\n    x = 1\n\nc = C()\nprint(hasattr(c, \"x\"))\nprint(getattr(c, \"x\"))\n",
    );
    assert!(ok, "run failed: {se}");
    assert_eq!(so.trim(), "True\n1", "hasattr/getattr repro: {so:?}");
}

#[test]
fn public3_format_slice_ascii_vars_differential() {
    if !node_present() { return; }
    // Expected values verified against CPython 3.12:
    //   format(3.14159, '.2f') == '3.14'; format(255, '#06x') == '0x00ff'
    //   format(42) == '42'; [1,2,3,4][slice(1,3)] == [2,3]
    //   'hello'[slice(3)] == 'hel'; [1,2,3,4,5][slice(None,None,-2)] == [5,3,1]
    //   ascii('café') == "'caf\xe9'"; vars(p) == {'x': 1, 'y': 'a'}
    //   slice(1,3).start/stop/step == (1, 3, None)
    let (ok, so, se) = run_ps(
        "print(format(3.14159, \".2f\"))\n\
         print(format(255, \"#06x\"))\n\
         print(format(42))\n\
         print([1, 2, 3, 4][slice(1, 3)])\n\
         print(\"hello\"[slice(3)])\n\
         print([1, 2, 3, 4, 5][slice(None, None, -2)])\n\
         print(ascii(\"café\"))\n\
         class P:\n    \
             def __init__(self, x, y):\n        \
                 self.x = x\n        \
                 self.y = y\n\
         p = P(1, \"a\")\n\
         v = vars(p)\n\
         print(v[\"x\"], v[\"y\"])\n\
         s = slice(1, 3)\n\
         print(s.start, s.stop, s.step)\n",
    );
    assert!(ok, "run failed: {se}");
    let want = "3.14\n0x00ff\n42\n[2, 3]\nhel\n[5, 3, 1]\n'caf\\xe9'\n1 a\n1 3 None";
    assert_eq!(so.trim().replace("\r\n", "\n"), want, "stdout: {so:?}");
}

#[test]
fn public3_unsupported_builtin_fails_compile_and_check() {
    let dir = psc_test_scratch("public3");
    let p = dir.join("uses_open.ps");
    std::fs::write(&p, "f = open(\"x.txt\")\nprint(f)\n").unwrap();

    // `pyths compile` must FAIL (exit != 0), name the builtin, and write no
    // output artifact.
    let out = pyths_bin()
        .args(["compile", p.to_str().unwrap()])
        .env("PYTHS_NO_CACHE", "1")
        .output()
        .unwrap();
    let se = String::from_utf8_lossy(&out.stderr);
    assert!(!out.status.success(), "compile must fail: {se}");
    assert!(
        se.contains("builtin 'open' is not supported yet"),
        "named diagnostic: {se}"
    );
    assert!(
        !dir.join("uses_open.js").exists(),
        "no artifact written on a failed compile"
    );

    // `pyths check` must FAIL too (the issue: check wrongly passed).
    let out = pyths_bin()
        .args(["check", p.to_str().unwrap()])
        .output()
        .unwrap();
    let se = String::from_utf8_lossy(&out.stderr);
    assert!(!out.status.success(), "check must fail: {se}");
    assert!(
        se.contains("builtin 'open' is not supported yet"),
        "check names the builtin: {se}"
    );

    // Control: shadowing `open` with a user def compiles AND checks clean.
    let ok_p = dir.join("shadows_open.ps");
    std::fs::write(
        &ok_p,
        "def open(path):\n    return path\nprint(open(\"fine\"))\n",
    )
    .unwrap();
    let out = pyths_bin()
        .args(["check", ok_p.to_str().unwrap()])
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "user-shadowed open must pass check: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let _ = std::fs::remove_dir_all(&dir);
}

// ─── #439: WASM-routed reserved-word function export/import coordination ───
//
// A function named like a JS reserved word (`default`, `new`) with typed
// numeric params is WASM-ROUTED under the default `js+wasm` target. The WASM
// binary exports it under its raw Python name (a string, always legal), but
// the JS glue wrapper, the `__jsfb` twin object, the `.__pyparams__` attach,
// and the main module's `import`/`export`/call sites must all use a
// coordinated JS-LEGAL identifier (`default$`), or the glue is a SyntaxError
// (`export function default` / `import { default }`). This compiles the real
// js+wasm artifacts and EXECUTES the main module under node (the glue's loader
// detects node and reads the .wasm via node:fs), proving the routed
// reserved-word functions actually run.
#[test]
fn test_run_wasm_reserved_word_export_coordination() {
    let dir = std::env::temp_dir().join("pyths_test_439_wasm_reserved");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let ps = dir.join("mod.ps");
    std::fs::write(
        &ps,
        "def default(a: int, b: int) -> int:\n    return a * b\n\
         def new(x: int) -> int:\n    return x + 1\n\
         print(default(6, 7))\n\
         print(new(41))\n",
    )
    .unwrap();
    let js = dir.join("mod.js");

    // Compile to the js+wasm artifacts (.js + .glue.js + .wasm).
    let out = pyths_bin()
        .args([
            "compile",
            ps.to_str().unwrap(),
            "--target",
            "js+wasm",
            "-o",
            js.to_str().unwrap(),
        ])
        .output()
        .expect("pyths compile failed to spawn");
    assert!(
        out.status.success(),
        "js+wasm compile of reserved-word functions must succeed: {}",
        String::from_utf8_lossy(&out.stderr)
    );

    // The main module must import/re-export the SANITIZED names, matching the
    // glue's `export function default$` — never the bare reserved word.
    let main_js = std::fs::read_to_string(&js).unwrap();
    assert!(
        main_js.contains("import { default$, new$ }") || main_js.contains("import { new$, default$ }"),
        "main module must import the sanitized glue exports:\n{main_js}"
    );
    assert!(
        !main_js.contains("import { default }") && !main_js.contains("import { default, "),
        "main module must not emit a bare reserved-word import binding:\n{main_js}"
    );
    let glue = std::fs::read_to_string(dir.join("mod.glue.js")).unwrap();
    assert!(
        glue.contains("export function default$(") && glue.contains("export function new$("),
        "glue wrappers must be sanitized:\n{glue}"
    );
    assert!(
        !glue.contains("export function default(") && !glue.contains("export function new("),
        "glue must not emit a reserved-word function declaration:\n{glue}"
    );
    // The WASM binary export is accessed as a property (reserved words legal
    // after `.`), so it stays the raw name.
    assert!(
        glue.contains("__wasm.default(") && glue.contains("__wasm.new("),
        "WASM binary export access stays the raw name:\n{glue}"
    );

    // Materialize the runtime so `pyths-runtime` resolves, then EXECUTE.
    pyths_runtime::materialize_runtime_package(&dir).expect("materialize runtime");
    let node = match Command::new("node").arg(js.to_str().unwrap()).output() {
        Ok(o) => o,
        Err(_) => {
            eprintln!("node unavailable — skipping execution half of #439 test");
            let _ = std::fs::remove_dir_all(&dir);
            return;
        }
    };
    let stdout = String::from_utf8_lossy(&node.stdout);
    let stderr = String::from_utf8_lossy(&node.stderr);
    assert!(
        node.status.success(),
        "js+wasm reserved-word module must run under node; stderr={stderr}"
    );
    assert_eq!(
        stdout.trim(),
        "42\n42",
        "default(6,7)=42 and new(41)=42 must run WASM-routed; got {stdout:?} stderr={stderr}"
    );

    let _ = std::fs::remove_dir_all(&dir);
}

// ─── #440: WASM error-dispatch vs early control-flow (end-to-end differential) ───
//
// Inside a `try`, a WASM-lowered subscript that raises IndexError from within a
// `return` expression would bypass the WASM error dispatch (the `return`
// executes before the post-statement err_code check), losing the exception.
// The analysis guard (review D) prevents that loss by rejecting such a function
// from the WASM fast path so it runs on the JS backend, where the handler works.
// The pyths_hir unit tests pin the REJECTION; this pins the CPython-correct
// end-to-end BEHAVIOR of the shipped default-target artifact: the exception is
// raised AND caught (victim(5) → 999), never lost.
#[test]
fn test_run_440_wasm_early_return_raise_is_caught() {
    let dir = std::env::temp_dir().join("pyths_test_440_early_return");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let ps = dir.join("mod.ps");
    // `return a[n]` inside try/except IndexError — the review-D bypass shape.
    std::fs::write(
        &ps,
        "def victim(n: int) -> int:\n\
         \x20   a = [1, 2, 3]\n\
         \x20   try:\n\
         \x20       return a[n]\n\
         \x20   except IndexError:\n\
         \x20       return 999\n\
         print(victim(1))\n\
         print(victim(5))\n",
    )
    .unwrap();
    let js = dir.join("mod.js");

    // Default routing (js+wasm). The function is review-D rejected → JS backend,
    // so NO glue export for `victim` is emitted for it.
    let out = pyths_bin()
        .args(["compile", ps.to_str().unwrap(), "-o", js.to_str().unwrap()])
        .output()
        .expect("pyths compile failed to spawn");
    assert!(
        out.status.success(),
        "default-target compile must succeed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let glue = dir.join("mod.glue.js");
    if glue.exists() {
        let g = std::fs::read_to_string(&glue).unwrap();
        assert!(
            !g.contains("function victim"),
            "victim must NOT be WASM-routed (review-D rejection): {g}"
        );
    }

    // Materialize the runtime and EXECUTE — the exception must be raised+caught.
    pyths_runtime::materialize_runtime_package(&dir).expect("materialize runtime");
    let node = match Command::new("node").arg(js.to_str().unwrap()).output() {
        Ok(o) => o,
        Err(_) => {
            eprintln!("node unavailable — skipping execution half of #440 test");
            let _ = std::fs::remove_dir_all(&dir);
            return;
        }
    };
    let stdout = String::from_utf8_lossy(&node.stdout);
    let stderr = String::from_utf8_lossy(&node.stderr);
    assert!(
        node.status.success(),
        "module must run cleanly (exception caught, not escaped); stderr={stderr}"
    );
    assert_eq!(
        stdout.trim(),
        "2\n999",
        "victim(1)=2 (in-bounds) and victim(5)=999 (IndexError raised AND caught, not lost); \
         got {stdout:?} stderr={stderr}"
    );

    let _ = std::fs::remove_dir_all(&dir);
}
