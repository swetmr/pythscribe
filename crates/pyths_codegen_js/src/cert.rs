//! Credible-compilation certificate for the SUBSCRIPT-ROUTING pass
//! (testing gap-closure §7.2, 2026-07-10 — the Axon move: validate *this*
//! compilation, don't re-prove the pass forever).
//!
//! `route()` is the single decision procedure for how a subscript
//! expression lowers to JS. `emit.rs` CONSULTS it (one source of truth),
//! codegen can record every decision into a [`Certificate`], and
//! [`check_certificate`] independently re-applies the rules to the
//! recorded evidence and cross-checks the emitted JS. A Lean twin of
//! `route` (verification/PythExpandVerify.lean, `RouteModel` section)
//! proves the safety invariant and generates
//! `verification/route-table.txt`, which a test in this crate compares
//! against the Rust table — so the model and the implementation cannot
//! drift silently.
//!
//! Honest trust boundary: the receiver-type EVIDENCE comes from
//! `emit.rs::infer_type` and is trusted (re-inferring independently
//! would be a second type checker). What the certificate establishes is
//! that (a) the routing RULES were applied correctly to that evidence at
//! every site, (b) the emitted JS artifact is consistent with the
//! recorded decisions (helper-call counts match), and (c) the
//! `provably_inbounds` verdict is JUSTIFIED — no longer trusted. The
//! checker re-derives the bound `0 <= index < list_len` from the
//! recorded [`InboundsEvidence`] and rejects any site whose
//! `provably_inbounds` bit is not exactly the re-derived value (the
//! CompCert §4.2 validate-a-posteriori move: check the analysis result,
//! don't trust the analysis). This closes the Gap-2 hole — a wrong
//! in-bounds verdict would otherwise route `native`, the certificate
//! would accept, and every gate would stay green while the read
//! silently went out of bounds (the #363 shape one level over). What
//! stays trusted is the *provenance* of `list_len`/`index` (that they
//! are the real receiver length and index at that span) — the same
//! artifact-consistency trust as the helper-call counts. Rule
//! application + the bound predicate are machine-checked in Lean; the
//! inference stays covered by the differential/test layers.

use std::fmt::Write as _;

/// Receiver type as inferred at the subscript site (the evidence).
/// Mirrors `emit.rs::JsInferredType` (kept separate so the certificate
/// is a stable, serializable surface).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RecvTy {
    Primitive,
    Float,
    List,
    Dict,
    Set,
    Tuple,
    Unknown,
}

impl RecvTy {
    pub const ALL: [RecvTy; 7] = [
        RecvTy::Primitive,
        RecvTy::Float,
        RecvTy::List,
        RecvTy::Dict,
        RecvTy::Set,
        RecvTy::Tuple,
        RecvTy::Unknown,
    ];
    pub fn name(self) -> &'static str {
        match self {
            RecvTy::Primitive => "primitive",
            RecvTy::Float => "float",
            RecvTy::List => "list",
            RecvTy::Dict => "dict",
            RecvTy::Set => "set",
            RecvTy::Tuple => "tuple",
            RecvTy::Unknown => "unknown",
        }
    }
}

/// The static evidence that justifies a `provably_inbounds` verdict:
/// a list-literal receiver of length `list_len` indexed by the integer
/// literal `index`. Recorded so [`check_certificate`] can *re-derive*
/// the bound `0 <= index < list_len` a posteriori instead of trusting
/// the `provably_inbounds` bit (CompCert §4.2 — validate the analysis
/// result, don't trust the analysis). `index` is `i128` to match
/// `ExprKind::IntLiteral`; a value outside `usize` is simply not in
/// bounds.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct InboundsEvidence {
    pub list_len: usize,
    pub index: i128,
}

impl InboundsEvidence {
    /// The decidable side condition: is the recorded index a valid,
    /// non-negative index into a list of the recorded length?
    pub fn is_inbounds(self) -> bool {
        self.index >= 0 && (self.index as u128) < self.list_len as u128
    }
}

/// Everything the routing decision depends on, at one subscript site.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SiteInput {
    pub is_slice: bool,
    pub is_lhs: bool,
    pub is_optional: bool,
    /// List-literal receiver + int-literal index statically in bounds
    /// (only ever true when `!is_lhs && !is_optional && !is_slice`).
    /// This bit drives `route()`, but it is NOT trusted by the
    /// certificate checker: `inbounds_evidence` must justify it, and
    /// [`check_certificate`] re-derives the bound decidably.
    pub provably_inbounds: bool,
    /// The static evidence for `provably_inbounds`. `Some` exactly when
    /// codegen took the list-literal/int-literal fast path; `None`
    /// otherwise (and then `provably_inbounds` must be `false`).
    pub inbounds_evidence: Option<InboundsEvidence>,
    pub recv_ty: RecvTy,
}

/// The four lowerings a subscript can take.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Route {
    /// `pySlice(value, lo, hi, step)`
    PySlice,
    /// Native `x[i]` — provably in-bounds literal fast path.
    NativeInbounds,
    /// `pyGetItem(value, index)` — Python-semantics read.
    Helper,
    /// Native `x[i]` / `x?.[i]` — LHS target, optional chain, or a
    /// receiver type outside the helper set.
    Native,
}

impl Route {
    pub fn name(self) -> &'static str {
        match self {
            Route::PySlice => "pySlice",
            Route::NativeInbounds => "native-inbounds",
            Route::Helper => "pyGetItem",
            Route::Native => "native",
        }
    }
}

/// THE decision procedure — the exact rule order of `emit.rs`'s
/// Subscript arm. Total by construction (every input gets exactly one
/// route). The Lean twin proves the read-safety invariant: no
/// Python-typed read (list/dict/tuple/primitive/unknown, non-LHS,
/// non-optional, non-slice, not provably in-bounds) ever routes native.
pub fn route(s: SiteInput) -> Route {
    if s.is_slice {
        return Route::PySlice;
    }
    if s.provably_inbounds {
        return Route::NativeInbounds;
    }
    // EVERY receiver type routes through pyGetItem for a plain read: the
    // container types get Python lookup semantics; the non-subscriptable
    // types (Float, Set, and the int/bool inside Primitive) get a Python
    // TypeError from the helper instead of a silent JS `undefined`. Float/Set
    // were previously EXCLUDED and lowered native (`3.5[0]` → undefined) — a
    // read-safety gap the lattice C4 shipping-binding surfaced (now closed).
    // Mirrors `helperTy` in verification/PythExpandVerify.lean.
    let helper_ty = matches!(
        s.recv_ty,
        RecvTy::List
            | RecvTy::Dict
            | RecvTy::Tuple
            | RecvTy::Primitive
            | RecvTy::Unknown
            | RecvTy::Float
            | RecvTy::Set
    );
    if !s.is_lhs && !s.is_optional && helper_ty {
        Route::Helper
    } else {
        Route::Native
    }
}

/// One recorded decision (a certificate entry).
#[derive(Debug, Clone)]
pub struct SiteRecord {
    /// Byte-offset span of the subscript expression in the .ps source.
    pub start: usize,
    pub end: usize,
    pub input: SiteInput,
    pub route: Route,
    /// BODY-relative byte offset (into `emit.rs`'s `self.output`, i.e. the
    /// codegen body BEFORE the runtime/React prelude is prepended) of the
    /// first byte of the emitted lowering for this site. `Some` when
    /// recorded during real codegen; `None` for hand-built certificates in
    /// unit tests (the positional cross-check is then skipped for the site).
    pub js_start: Option<usize>,
    /// BODY-relative byte offset just past the last byte of the emitted
    /// lowering for this site (exclusive). Pairs with [`SiteRecord::js_start`].
    pub js_end: Option<usize>,
}

/// The per-module certificate.
#[derive(Debug, Default, Clone)]
pub struct Certificate {
    pub sites: Vec<SiteRecord>,
    /// Maps a BODY-relative offset `o` (>= [`Certificate::directive_len`]) to
    /// its offset in the FINAL emitted `js`: `final = body_shift + o`. Set by
    /// `finish_certified` / `codegen_certified` once the final layout (prelude
    /// + hoisted directive) is known. `body_shift = P - D` where `P` is the
    ///   length of `result` at the moment the post-directive body is appended
    ///   and `D` is [`Certificate::directive_len`]. Zero on a default cert
    ///   (fine — positional check only runs when `js_start`/`js_end` are `Some`).
    pub body_shift: usize,
    /// Bytes of a leading `"use client";` / `"use server";` directive that
    /// `finish` hoisted off the front of the body. BODY offsets below this are
    /// inside the relocated directive and are NOT remapped by `body_shift`
    /// (the positional check skips them defensively).
    pub directive_len: usize,
}

impl Certificate {
    /// Serialize as JSON (dependency-free, hand-rolled — the format is
    /// flat and fully under our control).
    pub fn to_json(&self) -> String {
        let mut out = String::from("{\n  \"pass\": \"subscript-routing\",\n  \"sites\": [\n");
        for (i, s) in self.sites.iter().enumerate() {
            let (ev_n, ev_i) = match s.input.inbounds_evidence {
                Some(e) => (e.list_len as i128, e.index),
                None => (-1, -1),
            };
            let _ = writeln!(
                out,
                "    {{\"start\": {}, \"end\": {}, \"slice\": {}, \"lhs\": {}, \"optional\": {}, \"inbounds\": {}, \"ev_len\": {}, \"ev_idx\": {}, \"ty\": \"{}\", \"route\": \"{}\"}}{}",
                s.start,
                s.end,
                s.input.is_slice,
                s.input.is_lhs,
                s.input.is_optional,
                s.input.provably_inbounds,
                ev_n,
                ev_i,
                s.input.recv_ty.name(),
                s.route.name(),
                if i + 1 == self.sites.len() { "" } else { "," }
            );
        }
        out.push_str("  ]\n}\n");
        out
    }
}

/// Independent validation of a certificate against the emitted JS.
///
/// 1. **Rule check** — re-applies `route()` to every entry's recorded
///    evidence; any mismatch means codegen did not follow the rules.
/// 2. **Artifact consistency** — the number of `pyGetItem(` /
///    `pySlice(` call sites the certificate promises must match the
///    emitted JS (runtime-helper DEFINITIONS in inline mode are
///    excluded by counting call sites of the form `name(` minus
///    `function name(`).
///
/// Returns a list of human-readable violations (empty = certificate
/// accepted).
pub fn check_certificate(cert: &Certificate, js: &str) -> Vec<String> {
    let mut violations = Vec::new();

    for s in &cert.sites {
        let expected = route(s.input);
        if expected != s.route {
            violations.push(format!(
                "site @{}..{} — recorded route `{}` but the rules give `{}` for {:?}",
                s.start,
                s.end,
                s.route.name(),
                expected.name(),
                s.input
            ));
        }

        // Side condition (validate-a-posteriori): the `provably_inbounds`
        // bit is not trusted — re-derive it from the recorded evidence and
        // require an exact match. Absent evidence justifies only `false`.
        // This is what makes a `native-inbounds` route safe rather than a
        // trusted claim (Gap-2 / the #363 shape).
        let derived_inbounds = s
            .input
            .inbounds_evidence
            .is_some_and(InboundsEvidence::is_inbounds);
        if s.input.provably_inbounds != derived_inbounds {
            violations.push(format!(
                "site @{}..{} — provably_inbounds={} is not justified by its evidence {:?} \
                 (the bound 0 <= index < list_len re-derives to {})",
                s.start,
                s.end,
                s.input.provably_inbounds,
                s.input.inbounds_evidence,
                derived_inbounds
            ));
        }

        // POSITIONAL cross-check (route-swap detector). The count check above
        // cannot catch a *swap* — site A promises Helper but emits native, site
        // B promises native but emits a helper: the per-route counts stay equal
        // and the swap slips through. Here we look at the ACTUAL emitted JS at
        // this site's offset and require the lowering to match the promised
        // route. This is audit-strengthening only: EVERY uncertainty (absent
        // offsets, an offset inside the hoisted directive, an out-of-range or
        // non-char-boundary slice) SKIPS silently and is never a violation, so
        // the check can only ADD true detections and can never break a
        // currently-passing compile (false positives hard-fail the shipping
        // `--emit-cert` path, so they are unacceptable).
        if let (Some(js_start), Some(js_end)) = (s.js_start, s.js_end) {
            // A body offset below the hoisted-directive length is not remapped
            // by `body_shift` — skip defensively rather than mis-slice.
            if js_start >= cert.directive_len && js_start <= js_end {
                let sfin = cert.body_shift + js_start;
                let efin = cert.body_shift + js_end;
                if efin <= js.len() && js.is_char_boundary(sfin) && js.is_char_boundary(efin) {
                    let snippet = &js[sfin..efin];
                    let starts_slice = snippet.starts_with("pySlice(");
                    let starts_helper = snippet.starts_with("pyGetItem(");
                    let ok = match s.route {
                        Route::PySlice => starts_slice,
                        Route::Helper => starts_helper,
                        // A native `x[i]` / `x?.[i]` begins with the value
                        // expression, never with a helper-call token.
                        Route::Native | Route::NativeInbounds => !starts_slice && !starts_helper,
                    };
                    if !ok {
                        let peek: String = snippet.chars().take(24).collect();
                        violations.push(format!(
                            "site @{}..{} — route {} but emitted JS at [{}..{}] is {:?}",
                            s.start,
                            s.end,
                            s.route.name(),
                            sfin,
                            efin,
                            peek
                        ));
                    }
                }
            }
        }
    }

    let count_calls = |name: &str| -> usize {
        let call = format!("{name}(");
        let def = format!("function {name}(");
        let total = js.matches(&call).count();
        let defs = js.matches(&def).count();
        total - defs
    };
    let cert_helper = cert
        .sites
        .iter()
        .filter(|s| s.route == Route::Helper)
        .count();
    let cert_slice = cert
        .sites
        .iter()
        .filter(|s| s.route == Route::PySlice)
        .count();
    let js_helper = count_calls("pyGetItem");
    let js_slice = count_calls("pySlice");
    // Other lowerings may legitimately introduce these helpers (e.g.
    // augmented subscript assignment uses pyGetItem read-modify-write,
    // method lowerings use pySlice) — the JS can carry MORE call sites
    // than the subscript certificate, never fewer.
    if js_helper < cert_helper {
        violations.push(format!(
            "JS has {js_helper} pyGetItem call sites but the certificate promises {cert_helper}"
        ));
    }
    if js_slice < cert_slice {
        violations.push(format!(
            "JS has {js_slice} pySlice call sites but the certificate promises {cert_slice}"
        ));
    }

    violations
}

/// The full decision table over every flag/type combination — compared
/// against the Lean model's generated `verification/route-table.txt` by
/// `tests/route_table.rs`, binding model and implementation.
pub fn decision_table() -> String {
    let mut out = String::new();
    for is_slice in [false, true] {
        for is_lhs in [false, true] {
            for is_optional in [false, true] {
                for inbounds in [false, true] {
                    for ty in RecvTy::ALL {
                        // Evidence that justifies the `inbounds` bit for
                        // this table row — `route()` ignores it, but keeping
                        // it consistent means every enumerated row is a
                        // certificate the checker would also accept.
                        let inbounds_evidence = if inbounds {
                            Some(InboundsEvidence {
                                list_len: 1,
                                index: 0,
                            })
                        } else {
                            None
                        };
                        let input = SiteInput {
                            is_slice,
                            is_lhs,
                            is_optional,
                            provably_inbounds: inbounds,
                            inbounds_evidence,
                            recv_ty: ty,
                        };
                        let _ = writeln!(
                            out,
                            "{} {} {} {} {} -> {}",
                            is_slice as u8,
                            is_lhs as u8,
                            is_optional as u8,
                            inbounds as u8,
                            ty.name(),
                            route(input).name()
                        );
                    }
                }
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn site(
        is_slice: bool,
        is_lhs: bool,
        is_optional: bool,
        inbounds: bool,
        ty: RecvTy,
    ) -> SiteInput {
        SiteInput {
            is_slice,
            is_lhs,
            is_optional,
            provably_inbounds: inbounds,
            // keep evidence consistent with the bit so route() tests never
            // trip the check_certificate side condition
            inbounds_evidence: if inbounds {
                Some(InboundsEvidence {
                    list_len: 1,
                    index: 0,
                })
            } else {
                None
            },
            recv_ty: ty,
        }
    }

    #[test]
    fn slice_always_wins() {
        for ty in RecvTy::ALL {
            assert_eq!(route(site(true, false, false, false, ty)), Route::PySlice);
        }
    }

    #[test]
    fn inbounds_literal_is_native() {
        assert_eq!(
            route(site(false, false, false, true, RecvTy::List)),
            Route::NativeInbounds
        );
    }

    #[test]
    fn python_typed_reads_take_the_helper() {
        // EVERY receiver type (incl. Float/Set — non-subscriptable, the helper
        // raises TypeError) takes the helper for a plain read. Float/Set were
        // moved out of Native here after the lattice C4 binding found `3.5[0]`
        // / set-subscript silently lowering native to `undefined`.
        for ty in RecvTy::ALL {
            assert_eq!(route(site(false, false, false, false, ty)), Route::Helper);
        }
    }

    #[test]
    fn lhs_and_optional_stay_native() {
        // Only LHS targets and optional chains stay native now (they are not
        // plain reads); the receiver TYPE no longer forces native.
        assert_eq!(
            route(site(false, true, false, false, RecvTy::Dict)),
            Route::Native
        );
        assert_eq!(
            route(site(false, false, true, false, RecvTy::List)),
            Route::Native
        );
        assert_eq!(
            route(site(false, true, false, false, RecvTy::Float)),
            Route::Native
        );
        assert_eq!(
            route(site(false, false, true, false, RecvTy::Set)),
            Route::Native
        );
    }

    #[test]
    fn tampered_certificate_is_rejected() {
        let mut cert = Certificate::default();
        cert.sites.push(SiteRecord {
            start: 8,
            end: 12,
            input: site(false, false, false, false, RecvTy::Dict),
            route: Route::Native, // tampered: rules say Helper
            js_start: None,
            js_end: None,
        });
        let violations = check_certificate(&cert, "let x = d[k];");
        assert!(
            violations.iter().any(|v| v.contains("recorded route")),
            "tampering must be rejected: {violations:?}"
        );
    }

    #[test]
    fn unjustified_inbounds_verdict_is_rejected() {
        // The Gap-2 hole: a site claims provably_inbounds=true but the
        // recorded evidence does NOT satisfy 0 <= index < list_len. Before
        // the side condition this routed native-inbounds and the cert
        // accepted (silent OOB); now the checker must reject it.
        let mk = |ev: Option<InboundsEvidence>, bit: bool| {
            let mut cert = Certificate::default();
            cert.sites.push(SiteRecord {
                start: 0,
                end: 4,
                input: SiteInput {
                    is_slice: false,
                    is_lhs: false,
                    is_optional: false,
                    provably_inbounds: bit,
                    inbounds_evidence: ev,
                    recv_ty: RecvTy::List,
                },
                route: if bit {
                    Route::NativeInbounds
                } else {
                    Route::Helper
                },
                js_start: None,
                js_end: None,
            });
            cert
        };
        let says = |c: &Certificate, js: &str| {
            check_certificate(c, js)
                .iter()
                .any(|v| v.contains("provably_inbounds"))
        };
        // index == len: out of bounds, bit true → rejected
        assert!(says(
            &mk(
                Some(InboundsEvidence {
                    list_len: 3,
                    index: 3
                }),
                true
            ),
            "let x = a[3];"
        ));
        // negative index, bit true → rejected
        assert!(says(
            &mk(
                Some(InboundsEvidence {
                    list_len: 3,
                    index: -1
                }),
                true
            ),
            "let x = a[-1];"
        ));
        // bit true but NO evidence → rejected (unjustified)
        assert!(says(&mk(None, true), "let x = a[0];"));
        // genuinely in bounds, bit true, correct route → accepted
        assert!(!says(
            &mk(
                Some(InboundsEvidence {
                    list_len: 3,
                    index: 2
                }),
                true
            ),
            "let x = [10, 20, 30][2];"
        ));
        // bit false with no evidence (the common case) → accepted
        assert!(!says(&mk(None, false), "let x = pyGetItem(a, k);"));
    }

    #[test]
    fn missing_helper_in_js_is_rejected() {
        let mut cert = Certificate::default();
        cert.sites.push(SiteRecord {
            start: 8,
            end: 12,
            input: site(false, false, false, false, RecvTy::Dict),
            route: Route::Helper,
            js_start: None,
            js_end: None,
        });
        // Miscompiled JS: helper promised but a bare subscript emitted.
        let violations = check_certificate(&cert, "let x = d[k];");
        assert!(
            violations.iter().any(|v| v.contains("promises")),
            "helper-count mismatch must be rejected: {violations:?}"
        );
        // Correct JS accepted.
        assert!(check_certificate(&cert, "let x = pyGetItem(d, k);").is_empty());
    }

    // ---- mutation-testing follow-up (cargo-mutants, 2026-07-29): survivors
    // clustered in the SLICE under-emission check, `to_json` serialization, the
    // `total - defs` call count, and the positional guards. These kill them. ----

    fn helper_site() -> SiteRecord {
        SiteRecord {
            start: 0,
            end: 4,
            input: site(false, false, false, false, RecvTy::Dict),
            route: Route::Helper,
            js_start: None,
            js_end: None,
        }
    }

    fn helper_site_at(js_start: usize, js_end: usize) -> SiteRecord {
        SiteRecord {
            js_start: Some(js_start),
            js_end: Some(js_end),
            ..helper_site()
        }
    }

    #[test]
    fn missing_slice_in_js_is_rejected() {
        let mut cert = Certificate::default();
        cert.sites.push(SiteRecord {
            start: 0,
            end: 6,
            input: site(true, false, false, false, RecvTy::List), // is_slice → PySlice
            route: Route::PySlice,
            js_start: None,
            js_end: None,
        });
        // slice promised but no pySlice( emitted — the `js_slice < cert_slice`
        // safety check must fire (the counterpart of the helper check above).
        let v = check_certificate(&cert, "let x = a.slice(1, 2);");
        assert!(
            v.iter().any(|s| s.contains("pySlice")),
            "slice under-emission must be rejected: {v:?}"
        );
        assert!(check_certificate(&cert, "let x = pySlice(a, 1, 2, null);").is_empty());
    }

    #[test]
    fn to_json_serializes_sites_and_no_evidence_sentinels() {
        let mut cert = Certificate::default();
        cert.sites.push(SiteRecord {
            start: 3,
            end: 7,
            ..helper_site()
        });
        let json = cert.to_json();
        // to_json must produce the real structure (not empty / garbage)…
        assert!(
            json.contains("\"pass\": \"subscript-routing\""),
            "to_json shape: {json}"
        );
        assert!(
            json.contains("\"route\": \"pyGetItem\""),
            "route serialized: {json}"
        );
        assert!(
            json.contains("\"start\": 3") && json.contains("\"end\": 7"),
            "span serialized: {json}"
        );
        // …and a no-evidence site serializes the -1/-1 sentinel (not 1/1).
        assert!(
            json.contains("\"ev_len\": -1") && json.contains("\"ev_idx\": -1"),
            "sentinel: {json}"
        );
    }

    #[test]
    fn helper_count_excludes_the_helper_definition() {
        let mut cert = Certificate::default();
        cert.sites.push(helper_site());
        cert.sites.push(helper_site()); // promise TWO helper call sites
                                        // JS defines pyGetItem (inline runtime) and calls it ONCE. `total - defs`
                                        // must exclude the definition → only 1 real call < 2 promised → rejected.
        let js = "function pyGetItem(a, b) { return a; }\nlet x = pyGetItem(d, k);";
        assert!(
            check_certificate(&cert, js)
                .iter()
                .any(|s| s.contains("promises")),
            "the helper DEFINITION must not count as a call site"
        );
    }

    #[test]
    fn native_site_that_emits_a_helper_is_rejected_positionally() {
        let mut cert = Certificate::default();
        cert.sites.push(SiteRecord {
            start: 0,
            end: 4,
            input: site(false, true, false, false, RecvTy::Dict), // is_lhs → Native
            route: Route::Native,
            js_start: Some(0),
            js_end: Some(10),
        });
        // a Native site whose emitted lowering starts with a helper token is a
        // miscompile — the `Route::Native => !starts_slice && !starts_helper`
        // arm must flag it.
        let v = check_certificate(&cert, "pyGetItem(d)");
        assert!(
            v.iter().any(|s| s.contains("emitted JS at")),
            "native site emitting a helper must be caught positionally: {v:?}"
        );
    }

    #[test]
    fn positional_check_skips_offsets_inside_the_hoisted_directive() {
        let mut cert = Certificate::default();
        cert.directive_len = 100; // a large directive floor
        cert.sites.push(helper_site_at(2, 12));
        // js_start (2) < directive_len (100) → the `js_start >= directive_len`
        // guard must SKIP positional remapping. A pyGetItem call elsewhere keeps
        // the count check happy, so a correct run yields NO violation.
        let v = check_certificate(&cert, "pyGetItem(d); let y = 1;");
        assert!(
            v.is_empty(),
            "below-directive offset must skip positional, not flag: {v:?}"
        );
    }

    #[test]
    fn positional_check_skips_out_of_range_offsets() {
        let mut cert = Certificate::default();
        cert.sites.push(SiteRecord {
            start: 0,
            end: 4,
            input: site(false, true, false, false, RecvTy::Dict), // Native (no count check)
            route: Route::Native,
            js_start: Some(0),
            js_end: Some(100), // efin far past the JS length
        });
        // efin > js.len() → the `efin <= js.len()` guard must SKIP (a wrongly
        // entered slice would panic). Correct run: no violation.
        assert!(
            check_certificate(&cert, "short").is_empty(),
            "out-of-range offset must skip"
        );
    }

    #[test]
    fn positional_check_skips_non_char_boundary_offsets() {
        let mut cert = Certificate::default();
        cert.sites.push(SiteRecord {
            start: 0,
            end: 4,
            input: site(false, true, false, false, RecvTy::Dict), // Native
            route: Route::Native,
            js_start: Some(0),
            js_end: Some(1), // mid a 2-byte 'π'
        });
        // efin (1) is not a char boundary of "π" (2 bytes) → the
        // `is_char_boundary(efin)` guard must SKIP (a wrongly entered slice
        // would panic on the mid-codepoint index).
        assert!(
            check_certificate(&cert, "π").is_empty(),
            "non-char-boundary offset must skip"
        );
    }
}
