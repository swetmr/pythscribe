//! Corpus-wide credible-compilation gate for WASM auto-routing admission:
//! every parseable `.ps` under `examples/` builds an admission certificate
//! that the independent checker ACCEPTS against the emitted `.wasm`. Plus the
//! decision-table drift gate binding the Rust boundary rules
//! (`is_wasm_eligible` + `to_wasm_type`) to the Lean `WasmAdmission` model
//! (`verification/wasm-admission-table.txt`, checked from the Lean side by
//! `expanddiff --check-wasm-admission-table`).

use std::path::{Path, PathBuf};

use pyths_codegen_wasm::cert::{build_certificate, check_certificate};
use pyths_codegen_wasm::codegen_wasm;

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
fn admission_certificate_accepted_on_entire_examples_corpus() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let mut files = Vec::new();
    ps_files(&root.join("examples"), &mut files);
    assert!(
        files.len() > 20,
        "corpus unexpectedly small: {}",
        files.len()
    );

    let mut checked = 0usize;
    let mut admitted = 0usize;
    for f in &files {
        let src = std::fs::read_to_string(f).unwrap();
        // Negative fixtures / compressed `.ps` don't parse — skip them.
        let Ok(module) = pyths_parser::parse(&src) else {
            continue;
        };
        let cert = build_certificate(&module);
        let out = codegen_wasm(&module);
        let violations = check_certificate(&cert, &out.wasm, &module);
        assert!(
            violations.is_empty(),
            "WASM-admission certificate REJECTED for {}:\n{}",
            f.display(),
            violations.join("\n")
        );
        checked += 1;
        admitted += cert.admitted_names().len();
    }
    println!(
        "wasm-admission certificate accepted on {checked} modules, {admitted} admitted functions"
    );
    assert!(checked > 10, "too few compilable modules: {checked}");
}

#[test]
fn wasm_admission_table_matches_committed_fixture() {
    // The same fixture is independently regenerated and compared by the Lean
    // model (`expanddiff --check-wasm-admission-table`) — if either side
    // changes a boundary rule (`is_wasm_eligible` or `to_wasm_type`), its own
    // gate fails until model, fixture, and implementation agree again. Every
    // row also finitely witnesses `wasm_admission_sound` (no elig=1,lower=0).
    let fixture = include_str!("../../../verification/wasm-admission-table.txt");
    let ours = pyths_codegen_wasm::cert::admission_table();
    assert_eq!(
        fixture.replace("\r\n", "\n"),
        ours,
        "\nWASM boundary admission/lowering rules changed — update \
         verification/wasm-admission-table.txt AND the Lean WasmAdmission model \
         together (regenerate with `lake exe expanddiff --print-wasm-admission-table`)"
    );
}
