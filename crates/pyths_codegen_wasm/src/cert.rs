//! Credible-compilation certificate for WASM AUTO-ROUTING ADMISSION
//! (verification extension, 2026-07-16 — the same Axon move as the
//! subscript-routing cert: validate *this* compilation, don't re-prove the
//! pass forever).
//!
//! `analyze_module` (crates/pyths_hir) is the single decision procedure for
//! which functions the compiler admits to WASM lowering under
//! `--target js+wasm`. A function is admitted iff (a) its boundary types are
//! WASM-representable (`is_wasm_eligible` on every param + the return) and
//! (b) its body lies in the WASM-compatible fragment (`check_body`). This
//! module records every admission into a [`Certificate`] and
//! [`check_certificate`] independently re-derives admission and cross-checks
//! the emitted WASM artifact.
//!
//! ## What this certificate establishes (SOUNDNESS, not completeness)
//!
//! For every function the compiler admitted:
//!   0. **Admission re-derivation** — `check_certificate` independently re-runs
//!      the REAL decision procedure (`analyze_module`) on the module and
//!      confirms the certificate's admitted/rejected verdicts — INCLUDING
//!      body-fragment membership — match. A record that claims `admitted: true`
//!      for a function whose body is NOT in the WASM fragment (or whose
//!      boundary the rule rejects) is caught here. This is the validate-a-
//!      posteriori move (CompCert §4.2) applied to the WHOLE admission verdict,
//!      not just the boundary half — see "trust boundary" below for exactly
//!      what this does and does not buy.
//!   1. **Boundary re-check** — re-applies `is_wasm_eligible` to the recorded
//!      param/return types; any admitted type the rule rejects is a
//!      violation (catches over-admission at the boundary).
//!   2. **Lowering soundness** — every admitted boundary type must have a
//!      concrete WASM lowering (`to_wasm_type(ty).is_some()`). This is the
//!      invariant whose violation (admitting `Optional`, which has no
//!      lowering) previously PANICKED codegen at `emit.rs`'s
//!      `to_wasm_type(ty).unwrap()`. Its general form is proved in Lean as
//!      `wasm_admission_sound`.
//!   3. **Artifact consistency** — the emitted `.wasm` must export a function
//!      for every admitted name, and must NOT export any function the
//!      certificate records as rejected. Binds the decision to the real
//!      artifact (mirrors the subscript cert's JS call-site counts).
//!
//! This is **NOT**: completeness (we do not claim we admit everything
//! admissible), nor value-equivalence (we do not claim the JS and WASM
//! lowerings compute the same value). The point is narrow and honest:
//! **nothing outside the admitted fragment, and nothing that lacks a WASM
//! boundary representation, ever gets admitted.**
//!
//! ## What this certificate does NOT catch — and where each IS caught
//!
//! A clean admission certificate is emphatically NOT a proof that the emitted
//! WASM body computes the right value. The value/safety defect classes flagged
//! against the WASM backend (the "CRIT-A6" cluster) are OUT OF SCOPE here and
//! are covered by other, named layers — do not read a green cert as covering
//! them:
//!   - **Out-of-bounds list reads** (`get([7], 1)` with no `IndexError`) —
//!     covered by the Lean bounds proofs `slice_walk_inbounds` /
//!     `getIndex_inbounds` (TRUST.md "Slice-family clamping totality" /
//!     "Scalar index-normalization safety") and the per-compilation
//!     `scratch_non_interference` `debug_assert`.
//!   - **Raw `f64.div` for `/` / `//` (`1.0/0.0` → `inf`, truncating floor)** —
//!     covered by the wave-8 (`preservationWasm`) and wave-12
//!     (`wasmFdiv_floor_correct` / `wasmMod_floor_correct`) VALUE theorems and
//!     the Livermore three-backend differential.
//!   - **`i64` → `Number` dict precision loss / missing-key → 0** — covered by
//!     the return-type scalar restriction (#364: non-scalar returns stay on JS)
//!     and the differential corpus.
//!   - **Undefined marshallers for dict/tuple/callable boundaries** — made
//!     impossible at admission by the boundary rule (those shapes are
//!     `is_wasm_eligible = false`, witnessed for every shape in
//!     `admission_table`) and by the #364 scalar-return restriction.
//!
//! In short: value correctness is the differential + Livermore-WASM oracle
//! layer's job and the WASM VALUE preservation theorems' job; THIS certificate
//! is the admission-boundary gate only.
//!
//! ## Honest trust boundary
//!
//! Body-fragment MEMBERSHIP is now RE-DERIVED per compilation (point 0), so the
//! recorded `body_in_fragment` flag is checked, not trusted: a tampered or
//! drifted certificate cannot admit a function whose body the analyzer rejects.
//! What remains trusted is (a) the `check_body` IMPLEMENTATION itself —
//! re-running the same analyzer catches certificate tampering and
//! build-vs-codegen drift, but not a bug SHARED by both runs (a genuinely
//! independent second fragment checker would be a re-implementation, deferred);
//! and (b), explicitly, the VALUE-equivalence of the lowering, which stays with
//! the differential + Livermore-WASM oracle layers per the section above. The
//! boundary rules are machine-checked in Lean (`WasmAdmission`); the two Rust
//! functions (`is_wasm_eligible`, `to_wasm_type`) are bound to their Lean twins
//! by `verification/wasm-admission-table.txt`.
//!
//! Paper note: this result (WASM-admission soundness certificate + the Lean
//! `wasm_admission_sound` theorem) extends the machine-checked-routing story
//! past subscript lowering and slots into the verified-compilation preprint
//! (Paper C, `preprints/verified-compilation-draft.tex` in the reference-app
//! repo) — do NOT edit that repo from here; this is only a pointer.

use std::fmt::Write as _;

use pyths_hir::{analyze_module, is_wasm_eligible};
use pyths_syntax::ast::Module;
use pyths_types::types::Type;

use crate::types::to_wasm_type;

/// One recorded admission decision (a certificate entry).
#[derive(Debug, Clone)]
pub struct AdmissionRecord {
    pub name: String,
    /// Boundary parameter types (the evidence the boundary rule ran on).
    pub params: Vec<(String, Type)>,
    /// Declared return type (`Void` when the annotation was absent).
    pub return_type: Type,
    /// Was the function admitted to WASM lowering?
    pub admitted: bool,
    /// Body-fragment evidence from `analyze_module::check_body` (trusted).
    /// Always `true` for an admitted function (admission requires it); kept
    /// on rejected records as `false` for the artifact cross-check.
    pub body_in_fragment: bool,
}

/// The per-module WASM-admission certificate.
#[derive(Debug, Default, Clone)]
pub struct Certificate {
    pub funcs: Vec<AdmissionRecord>,
}

impl Certificate {
    /// The names admitted to WASM.
    pub fn admitted_names(&self) -> Vec<&str> {
        self.funcs
            .iter()
            .filter(|f| f.admitted)
            .map(|f| f.name.as_str())
            .collect()
    }

    /// Serialize as JSON (dependency-free, hand-rolled — the format is flat
    /// and fully under our control, exactly like the subscript cert).
    pub fn to_json(&self) -> String {
        let mut out = String::from("{\n  \"pass\": \"wasm-admission\",\n  \"funcs\": [\n");
        for (i, f) in self.funcs.iter().enumerate() {
            let params: Vec<String> = f
                .params
                .iter()
                .map(|(n, t)| format!("{{\"name\": \"{}\", \"ty\": \"{}\"}}", n, t))
                .collect();
            let _ = writeln!(
                out,
                "    {{\"name\": \"{}\", \"admitted\": {}, \"body_in_fragment\": {}, \"return\": \"{}\", \"params\": [{}]}}{}",
                f.name,
                f.admitted,
                f.body_in_fragment,
                f.return_type,
                params.join(", "),
                if i + 1 == self.funcs.len() { "" } else { "," }
            );
        }
        out.push_str("  ]\n}\n");
        out
    }
}

/// Build the admission certificate for a module by running the REAL emit-path
/// decision procedure (`analyze_module`) — the same analysis `codegen_wasm`
/// consults, so the certificate describes the actual compilation.
pub fn build_certificate(module: &Module) -> Certificate {
    let analysis = analyze_module(module);
    let mut funcs = Vec::new();

    // Admitted functions carry their boundary types + a proven-in-fragment body.
    let mut ordered: Vec<_> = analysis.eligible.values().cloned().collect();
    ordered.sort_by(|a, b| a.name.cmp(&b.name));
    for info in ordered {
        funcs.push(AdmissionRecord {
            name: info.name,
            params: info.params,
            return_type: info.return_type,
            admitted: true,
            body_in_fragment: true,
        });
    }

    // Rejected functions: name only (types not needed for the "not exported"
    // cross-check). Sorted for deterministic output.
    let mut rejected: Vec<String> = analysis.rejected.into_iter().map(|(n, _)| n).collect();
    rejected.sort();
    for name in rejected {
        funcs.push(AdmissionRecord {
            name,
            params: Vec::new(),
            return_type: Type::Void,
            admitted: false,
            body_in_fragment: false,
        });
    }

    Certificate { funcs }
}

/// Return type is representable iff it lowers OR is a no-value return.
fn return_representable(ty: &Type) -> bool {
    matches!(ty, Type::NoneType | Type::Void) || to_wasm_type(ty).is_some()
}

/// Independent validation of a certificate against the module and the emitted
/// WASM binary.
///
/// Returns a list of human-readable violations (empty = accepted).
pub fn check_certificate(cert: &Certificate, wasm: &[u8], module: &Module) -> Vec<String> {
    let mut violations = Vec::new();

    // (0) Admission re-derivation — validate-a-posteriori over the WHOLE verdict
    // (boundary + body-fragment membership), not just the boundary half. Re-run
    // the SAME decision procedure `codegen_wasm` consults and confirm the
    // certificate's admitted/rejected sets match. This is what moves
    // `body_in_fragment` out of the trust boundary (see module header): a
    // certificate that records `admitted: true` for a body the analyzer rejects
    // is caught here. Trusted still: the `check_body` implementation itself (a
    // shared-code bug survives), and lowering VALUE-equivalence (differential's
    // job).
    let analysis = analyze_module(module);
    for f in &cert.funcs {
        let rederived_admit = analysis.eligible.contains_key(&f.name);
        if f.admitted && !rederived_admit {
            violations.push(format!(
                "function `{}`: certificate records ADMITTED but re-derivation from the \
                 module does not admit it (boundary rejected or body not in the WASM \
                 fragment) — body-in-fragment evidence is checked, not trusted",
                f.name
            ));
        }
        if !f.admitted && rederived_admit {
            violations.push(format!(
                "function `{}`: certificate records REJECTED but re-derivation admits it",
                f.name
            ));
        }
    }
    // The certificate must not silently OMIT an admitted function.
    for name in analysis.eligible.keys() {
        if !cert.funcs.iter().any(|f| &f.name == name && f.admitted) {
            violations.push(format!(
                "function `{}`: re-derivation admits it but the certificate does not record \
                 it as admitted",
                name
            ));
        }
    }

    for f in &cert.funcs {
        if !f.admitted {
            continue;
        }
        // (1) Boundary re-check + (2) lowering soundness for each param.
        for (pname, ty) in &f.params {
            if !is_wasm_eligible(ty) {
                violations.push(format!(
                    "function `{}`: admitted but param `{}` has boundary type `{}` \
                     which the admission rule rejects",
                    f.name, pname, ty
                ));
            }
            if to_wasm_type(ty).is_none() {
                violations.push(format!(
                    "function `{}`: admitted but param `{}` boundary type `{}` has \
                     NO WASM lowering (to_wasm_type = None) — codegen would panic",
                    f.name, pname, ty
                ));
            }
        }
        // Return type.
        if !return_representable(&f.return_type) {
            violations.push(format!(
                "function `{}`: admitted but return type `{}` has no WASM lowering",
                f.name, f.return_type
            ));
        }
        // (evidence consistency) an admitted function must carry the body flag.
        if !f.body_in_fragment {
            violations.push(format!(
                "function `{}`: admitted without body-in-fragment evidence",
                f.name
            ));
        }
    }

    // (3) Artifact consistency. An empty WASM binary means nothing was
    // admitted (codegen emits no module) — nothing to cross-check.
    if wasm.is_empty() {
        if !cert.admitted_names().is_empty() {
            violations.push(format!(
                "certificate admits {} function(s) but the emitted WASM is empty",
                cert.admitted_names().len()
            ));
        }
        return violations;
    }

    match parse_wasm_func_exports(wasm) {
        Some(exports) => {
            for f in &cert.funcs {
                if f.admitted && !exports.iter().any(|e| e == &f.name) {
                    violations.push(format!(
                        "function `{}`: admitted but absent from the emitted WASM exports",
                        f.name
                    ));
                }
                if !f.admitted && exports.iter().any(|e| e == &f.name) {
                    violations.push(format!(
                        "function `{}`: recorded as REJECTED but present in the emitted WASM exports",
                        f.name
                    ));
                }
            }
        }
        None => violations
            .push("could not parse the WASM export section from the emitted binary".to_string()),
    }

    violations
}

// ---------------------------------------------------------------------------
// Minimal, dependency-free WASM export-section reader.
//
// The cert module ships in `src/` where `wasmparser` is a dev-only dependency,
// so we hand-roll the tiny scan we need (mirrors cert.rs's "dependency-free,
// hand-rolled" philosophy). Returns the names of Func-kind exports, or `None`
// on any malformation (the checker then reports it rather than trusting a
// partial parse).
// ---------------------------------------------------------------------------

/// Decode an unsigned LEB128 at `*pos`, advancing it. `None` on overrun.
fn read_uleb(bytes: &[u8], pos: &mut usize) -> Option<u64> {
    let mut result: u64 = 0;
    let mut shift = 0u32;
    loop {
        let b = *bytes.get(*pos)?;
        *pos += 1;
        result |= u64::from(b & 0x7f) << shift;
        if b & 0x80 == 0 {
            return Some(result);
        }
        shift += 7;
        if shift >= 64 {
            return None;
        }
    }
}

/// Parse the names of all Func-kind (`0x00`) exports from a WASM module.
pub fn parse_wasm_func_exports(bytes: &[u8]) -> Option<Vec<String>> {
    // Header: magic "\0asm" (0x00 0x61 0x73 0x6d) + version 1.
    if bytes.len() < 8 || &bytes[0..4] != b"\0asm" {
        return None;
    }
    let mut pos = 8usize;
    while pos < bytes.len() {
        let section_id = *bytes.get(pos)?;
        pos += 1;
        let section_len = read_uleb(bytes, &mut pos)? as usize;
        let section_end = pos.checked_add(section_len)?;
        if section_end > bytes.len() {
            return None;
        }
        if section_id == 7 {
            // Export section: vec(export). export = name:byte-vec, kind:u8, idx:uleb.
            let mut names = Vec::new();
            let count = read_uleb(bytes, &mut pos)?;
            for _ in 0..count {
                let name_len = read_uleb(bytes, &mut pos)? as usize;
                let name_end = pos.checked_add(name_len)?;
                if name_end > section_end {
                    return None;
                }
                let name = std::str::from_utf8(&bytes[pos..name_end]).ok()?.to_string();
                pos = name_end;
                let kind = *bytes.get(pos)?;
                pos += 1;
                let _idx = read_uleb(bytes, &mut pos)?;
                if kind == 0x00 {
                    names.push(name);
                }
            }
            return Some(names);
        }
        pos = section_end;
    }
    // No export section at all — a module with no exports.
    Some(Vec::new())
}

// ---------------------------------------------------------------------------
// Decision table — the Rust<->Lean binding surface (mirrors cert.rs's
// `decision_table` + verification/route-table.txt). Enumerates BOTH shipping
// boundary functions — `is_wasm_eligible` and `to_wasm_type(_).is_some()` —
// over every boundary-type SHAPE up to a bounded nesting depth, and prints
// `shape -> <elig-bit> <lower-bit>`. The committed fixture
// verification/wasm-admission-table.txt is compared against this from Rust
// (`cargo test wasm_admission_table_matches_committed_fixture`) AND from Lean
// (`expanddiff --check-wasm-admission-table`); either side changing a rule
// breaks its own gate. Every row also WITNESSES the soundness invariant
// finitely: no row has elig=1, lower=0.
// ---------------------------------------------------------------------------

/// The leaf-type alphabet, in table order. Chosen to hit every match arm of
/// `is_wasm_eligible` / `to_wasm_type`: eligible primitives, the top-level
/// `any`/`none`/`void`/`named` rejects, and (as a collection element) the
/// `any`-accepted-inside path.
fn base_alphabet() -> Vec<(&'static str, Type)> {
    vec![
        ("int", Type::Int),
        ("float", Type::Float),
        ("bool", Type::Bool),
        ("str", Type::Str),
        ("none", Type::NoneType),
        ("any", Type::Any),
        ("void", Type::Void),
        ("named", Type::Named("T".to_string())),
    ]
}

fn bit(b: bool) -> char {
    if b {
        '1'
    } else {
        '0'
    }
}

/// Emit `<name> -> <elig> <lower>\n` for a concrete boundary type.
fn row(out: &mut String, name: &str, ty: &Type) {
    let elig = is_wasm_eligible(ty);
    let lower = to_wasm_type(ty).is_some();
    let _ = writeln!(out, "{} -> {} {}", name, bit(elig), bit(lower));
}

/// The full boundary-admission decision table. Byte-identical to the Lean
/// `wasmAdmissionTable` (same loop order, same shape names).
pub fn admission_table() -> String {
    let base = base_alphabet();
    let mut out = String::new();

    // 1. Leaves.
    for (an, at) in &base {
        row(&mut out, an, at);
    }
    // 2. list<a>
    for (an, at) in &base {
        row(
            &mut out,
            &format!("list<{an}>"),
            &Type::List(Box::new(at.clone())),
        );
    }
    // 3. set<a>
    for (an, at) in &base {
        row(
            &mut out,
            &format!("set<{an}>"),
            &Type::Set(Box::new(at.clone())),
        );
    }
    // 4. opt<a>  (Optional — witnesses elig=0, lower=0 for every a post-fix)
    for (an, at) in &base {
        row(
            &mut out,
            &format!("opt<{an}>"),
            &Type::Optional(Box::new(at.clone())),
        );
    }
    // 5. dict<a,b>
    for (an, at) in &base {
        for (bn, bt) in &base {
            row(
                &mut out,
                &format!("dict<{an},{bn}>"),
                &Type::Dict(Box::new(at.clone()), Box::new(bt.clone())),
            );
        }
    }
    // 6. tuple<a,b>
    for (an, at) in &base {
        for (bn, bt) in &base {
            row(
                &mut out,
                &format!("tuple<{an},{bn}>"),
                &Type::Tuple(vec![at.clone(), bt.clone()]),
            );
        }
    }
    // 7. callable<a,b>  (one param a, return b)
    for (an, at) in &base {
        for (bn, bt) in &base {
            row(
                &mut out,
                &format!("callable<{an},{bn}>"),
                &Type::Callable(vec![at.clone()], Box::new(bt.clone())),
            );
        }
    }
    // 8. list<list<a>>  (depth-2 recursion through the _inner path)
    for (an, at) in &base {
        row(
            &mut out,
            &format!("list<list<{an}>>"),
            &Type::List(Box::new(Type::List(Box::new(at.clone())))),
        );
    }

    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn analyze(src: &str) -> Certificate {
        let module = pyths_parser::parse(src).expect("parse");
        build_certificate(&module)
    }

    #[test]
    fn admitted_numeric_function_certified() {
        let cert = analyze("def add(a: int, b: int) -> int:\n    return a + b\n");
        assert!(cert.admitted_names().contains(&"add"));
        let module =
            pyths_parser::parse("def add(a: int, b: int) -> int:\n    return a + b\n").unwrap();
        let out = crate::codegen_wasm(&module);
        assert!(check_certificate(&cert, &out.wasm, &module).is_empty());
    }

    #[test]
    fn optional_boundary_is_not_admitted() {
        // The regression the Lean soundness theorem generalizes: Optional has
        // no WASM lowering, so it must never be admitted.
        let cert = analyze(
            "def f(x: Optional[int]) -> int:\n    return 1\ndef g(a: int) -> int:\n    return a\n",
        );
        assert!(!cert.admitted_names().contains(&"f"));
        assert!(cert.admitted_names().contains(&"g"));
    }

    #[test]
    fn tampered_admission_is_rejected() {
        // Hand-forge a certificate that admits an Optional boundary type; the
        // checker must reject it on both the boundary rule AND lowering.
        let cert = Certificate {
            funcs: vec![AdmissionRecord {
                name: "bad".to_string(),
                params: vec![("x".to_string(), Type::Optional(Box::new(Type::Int)))],
                return_type: Type::Int,
                admitted: true,
                body_in_fragment: true,
            }],
        };
        // Non-empty but export-less fake module → also flags "absent from exports".
        let fake = b"\0asm\x01\x00\x00\x00".to_vec();
        // `bad` does not exist in this (empty) module, so re-derivation also
        // rejects it — but the boundary + lowering violations still fire.
        let empty = pyths_parser::parse("").unwrap();
        let v = check_certificate(&cert, &fake, &empty);
        assert!(
            v.iter().any(|m| m.contains("admission rule rejects")),
            "{v:?}"
        );
        assert!(v.iter().any(|m| m.contains("NO WASM lowering")), "{v:?}");
    }

    #[test]
    fn forged_body_rejected_admission_is_caught_by_rederivation() {
        // A function with an ELIGIBLE boundary (int -> int) whose BODY is not in
        // the WASM fragment (calls `print`, not a whitelisted numeric builtin).
        // The real analysis REJECTS it; a forged certificate that records it as
        // admitted must be caught by the admission re-derivation — this is the
        // gap D1/CRIT-20 flagged (body-in-fragment was previously trusted).
        let src = "def f(a: int) -> int:\n    print(a)\n    return a\n";
        let module = pyths_parser::parse(src).unwrap();
        // Sanity: the honest analysis does NOT admit `f`.
        let honest = build_certificate(&module);
        assert!(
            !honest.admitted_names().contains(&"f"),
            "expected `f` to be body-rejected"
        );
        // Forge a cert claiming `f` is admitted with an eligible boundary.
        let forged = Certificate {
            funcs: vec![AdmissionRecord {
                name: "f".to_string(),
                params: vec![("a".to_string(), Type::Int)],
                return_type: Type::Int,
                admitted: true,
                body_in_fragment: true,
            }],
        };
        let out = crate::codegen_wasm(&module);
        let v = check_certificate(&forged, &out.wasm, &module);
        assert!(
            v.iter()
                .any(|m| m.contains("re-derivation from the module does not admit it")),
            "re-derivation should reject the forged body-rejected admission: {v:?}"
        );
    }

    #[test]
    fn rejected_function_must_not_be_exported() {
        // A certificate claiming `g` was rejected, but the artifact exports it.
        let module = pyths_parser::parse("def g(a: int) -> int:\n    return a\n").unwrap();
        let out = crate::codegen_wasm(&module);
        let cert = Certificate {
            funcs: vec![AdmissionRecord {
                name: "g".to_string(),
                params: Vec::new(),
                return_type: Type::Void,
                admitted: false,
                body_in_fragment: false,
            }],
        };
        let v = check_certificate(&cert, &out.wasm, &module);
        assert!(
            v.iter()
                .any(|m| m.contains("present in the emitted WASM exports")),
            "{v:?}"
        );
    }

    #[test]
    fn export_parser_reads_names() {
        let module =
            pyths_parser::parse("def add(a: int, b: int) -> int:\n    return a + b\n").unwrap();
        let out = crate::codegen_wasm(&module);
        let names = parse_wasm_func_exports(&out.wasm).expect("parse exports");
        assert!(names.iter().any(|n| n == "add"), "exports: {names:?}");
    }

    #[test]
    fn admission_table_is_sound_on_every_row() {
        // Finite witness of `wasm_admission_sound`: no row admits (elig=1)
        // a type with no lowering (lower=0).
        for line in admission_table().lines() {
            let bits: Vec<&str> = line.rsplitn(2, "-> ").collect();
            let rhs = bits[0];
            let mut it = rhs.split_whitespace();
            let elig = it.next().unwrap();
            let lower = it.next().unwrap();
            assert!(
                !(elig == "1" && lower == "0"),
                "UNSOUND admission row (admitted but unlowerable): {line}"
            );
        }
    }
}
