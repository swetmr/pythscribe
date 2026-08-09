//! Corpus-wide credible-compilation gate (§7.2): every parseable `.ps`
//! under `examples/` compiles with a subscript-routing certificate that
//! the independent checker ACCEPTS against the emitted JS. Plus the
//! decision-table drift gate binding the Rust rules to the Lean model
//! (`verification/route-table.txt`, checked from the Lean side by
//! `lake exe expanddiff --check-route-table`).

use std::path::{Path, PathBuf};

fn ps_files(dir: &Path, out: &mut Vec<PathBuf>) {
    if !dir.exists() {
        return;
    }
    for e in std::fs::read_dir(dir).unwrap() {
        let p = e.unwrap().path();
        if p.is_dir() {
            ps_files(&p, out);
        } else if p.extension().is_some_and(|x| x == "ps") {
            out.push(p);
        }
    }
}

#[test]
fn certificate_accepted_on_entire_examples_corpus() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let mut files = Vec::new();
    ps_files(&root.join("examples"), &mut files);
    assert!(
        files.len() > 20,
        "corpus unexpectedly small: {}",
        files.len()
    );

    let mut checked = 0usize;
    let mut sites = 0usize;
    for f in &files {
        let src = std::fs::read_to_string(f).unwrap();
        // Negative fixtures / compressed content in .ps don't parse — the
        // certificate only exists for compilable modules.
        let Ok(module) = pyths_parser::parse(&src) else {
            continue;
        };
        let opts = pyths_codegen_js::CodegenOptions::default();
        let certified = pyths_codegen_js::codegen_certified(&module, &opts);
        let violations =
            pyths_codegen_js::cert::check_certificate(&certified.certificate, &certified.js);
        assert!(
            violations.is_empty(),
            "certificate REJECTED for {}:\n{}",
            f.display(),
            violations.join("\n")
        );
        checked += 1;
        sites += certified.certificate.sites.len();
    }
    println!("certificate accepted on {checked} modules, {sites} subscript sites");
    assert!(checked > 10, "too few compilable modules: {checked}");
}

/// False-positive guard + byte-identity proof over the whole corpus, with NO
/// compile cache in the loop (unlike the CLI): the certified path
/// (`finish_certified`) must emit EXACTLY the same JS bytes as the plain path
/// (`finish`) — recording offsets must not perturb a single emitted byte — and
/// `check_certificate` (now including the positional cross-check) must accept
/// every real program. A single false positive here would hard-fail the
/// shipping `--emit-cert` compile.
#[test]
fn certified_js_is_byte_identical_to_plain_and_never_false_positives() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let mut files = Vec::new();
    ps_files(&root.join("examples"), &mut files);

    let mut checked = 0usize;
    for f in &files {
        let src = std::fs::read_to_string(f).unwrap();
        let Ok(module) = pyths_parser::parse(&src) else {
            continue;
        };
        let opts = pyths_codegen_js::CodegenOptions::default();
        let plain = pyths_codegen_js::codegen_with_options(&module, &opts);
        let certified = pyths_codegen_js::codegen_certified(&module, &opts);
        assert_eq!(
            plain,
            certified.js,
            "certified JS diverged from plain JS for {} — recording offsets \
             must not change emitted bytes",
            f.display()
        );
        let violations =
            pyths_codegen_js::cert::check_certificate(&certified.certificate, &certified.js);
        assert!(
            violations.is_empty(),
            "FALSE POSITIVE for {}:\n{}",
            f.display(),
            violations.join("\n")
        );
        checked += 1;
    }
    assert!(checked > 10, "too few compilable modules: {checked}");
    println!("byte-identical + accepted on {checked} real modules");
}

/// Proves the BODY→FINAL offset mapping (`body_shift`) is correct AND that
/// the positional cross-check is ACTIVE (not silently skipping every site).
/// The program triggers a runtime-import prelude (`pyGetItem`) so
/// `body_shift > 0` — if the mapping were off by the prelude length the
/// asserted token would not be found at the mapped offset.
#[test]
fn positional_check_is_active_and_mapping_is_correct() {
    use pyths_codegen_js::cert::Route;
    // `d[k]` on an unannotated param routes Helper (pyGetItem) → runtime
    // import → a real prelude is prepended, so BODY offsets are shifted.
    let src = "def f(d, k):\n    return d[k]\n";
    let module = pyths_parser::parse(src).expect("parses");
    let opts = pyths_codegen_js::CodegenOptions::default();
    let certified = pyths_codegen_js::codegen_certified(&module, &opts);
    let cert = &certified.certificate;
    let js = &certified.js;

    // Baseline: the real emitted output must be accepted.
    assert!(
        pyths_codegen_js::cert::check_certificate(cert, js).is_empty(),
        "baseline certified output must be accepted"
    );

    // The prelude actually shifted the body — otherwise this test would not
    // be exercising the shift arithmetic at all.
    assert!(
        cert.body_shift > 0,
        "expected a runtime-import prelude to shift the body (body_shift={})",
        cert.body_shift
    );

    // At least one site, and every recorded positional window maps to JS that
    // matches its promised route. This is the exact slice the checker inspects
    // — asserting the token here proves the mapping is right and the check
    // fires rather than skipping.
    let mut active = 0usize;
    for s in &cert.sites {
        let (Some(js_start), Some(js_end)) = (s.js_start, s.js_end) else {
            continue;
        };
        if js_start < cert.directive_len {
            continue;
        }
        let a = cert.body_shift + js_start;
        let b = cert.body_shift + js_end;
        assert!(b <= js.len(), "mapped window out of range");
        let snippet = &js[a..b];
        match s.route {
            Route::Helper => assert!(
                snippet.starts_with("pyGetItem("),
                "Helper site JS {snippet:?} at [{a}..{b}] should start with pyGetItem("
            ),
            Route::PySlice => assert!(
                snippet.starts_with("pySlice("),
                "PySlice site JS {snippet:?} should start with pySlice("
            ),
            Route::Native | Route::NativeInbounds => assert!(
                !snippet.starts_with("pyGetItem(") && !snippet.starts_with("pySlice("),
                "native site JS {snippet:?} must not start with a helper token"
            ),
        }
        active += 1;
    }
    assert!(active >= 1, "expected >= 1 positionally-checked site");
}

/// The route-SWAP the whole change exists to catch: flip one Helper site to
/// Native and one Native site to Helper. Per-route COUNTS stay balanced, so
/// the count check alone still accepts — but the POSITIONAL check must reject,
/// because the emitted JS at each site no longer matches its (swapped) route.
#[test]
fn balanced_route_swap_is_caught_by_positional_check() {
    use pyths_codegen_js::cert::Route;
    // `d[k]` → Helper (pyGetItem); `s?.[k]` → Native (optional chain). Two
    // sites with different routes, both in the same module.
    let src = "def f(d, k, s):\n    x = d[k]\n    y = s?.[k]\n    return [x, y]\n";
    let module = pyths_parser::parse(src).expect("parses");
    let opts = pyths_codegen_js::CodegenOptions::default();
    let certified = pyths_codegen_js::codegen_certified(&module, &opts);
    let js = &certified.js;

    // Sanity: the honest certificate is accepted, and it really contains one
    // Helper and one Native site to swap.
    assert!(
        pyths_codegen_js::cert::check_certificate(&certified.certificate, js).is_empty(),
        "baseline must be accepted"
    );
    let helper_idx = certified
        .certificate
        .sites
        .iter()
        .position(|s| s.route == Route::Helper)
        .expect("a Helper site");
    let native_idx = certified
        .certificate
        .sites
        .iter()
        .position(|s| s.route == Route::Native)
        .expect("a Native site");

    // Perform the balanced swap on a clone (routes only; JS untouched).
    let mut swapped = certified.certificate.clone();
    swapped.sites[helper_idx].route = Route::Native;
    swapped.sites[native_idx].route = Route::Helper;

    let violations = pyths_codegen_js::cert::check_certificate(&swapped, js);

    // The count check is blind to this swap (pyGetItem count unchanged): prove
    // no violation mentions the helper/slice COUNT.
    assert!(
        !violations.iter().any(|v| v.contains("promises")),
        "the count check should NOT be what catches this swap: {violations:?}"
    );
    // The positional check MUST catch it — a route-mismatch violation exists.
    assert!(
        violations.iter().any(|v| v.contains("but emitted JS at")),
        "positional check must catch the balanced route swap: {violations:?}"
    );
}

#[test]
fn route_table_matches_committed_fixture() {
    // The same fixture is independently regenerated and compared by the
    // Lean model (`lake exe expanddiff --check-route-table`) — if either
    // side changes its rules, its own gate fails until model, fixture,
    // and implementation agree again.
    let fixture = include_str!("../../../verification/route-table.txt");
    let ours = pyths_codegen_js::cert::decision_table();
    assert_eq!(
        fixture.replace("\r\n", "\n"),
        ours,
        "\nsubscript-routing rules changed — update verification/route-table.txt \
         AND the Lean RouteModel together (regenerate with `lake exe expanddiff --print-route-table`)"
    );
}
