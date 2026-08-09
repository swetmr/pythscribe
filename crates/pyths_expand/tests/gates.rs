//! P0 validation gates for the `.psc` rewrite system (testing gap-closure,
//! 2026-07-10). Three gate families:
//!
//! 1. **Dictionary audit** — the reversibility of the whole `.psc` scheme
//!    rests on the `$NAME` table being conflict-free and injective. These
//!    invariants were previously enforced only by trust (the token-savings
//!    audit scripts are reports, not gates).
//! 2. **Zone-classifier property test** — the Lean model
//!    (`verification/PythExpandVerify.lean`) PROVES zone-safety for an
//!    *assumed-correct* byte-level zone partition; the classifier itself is
//!    the model's trust boundary (the "x18" of this project — see the Axon
//!    register-allocator lesson). This test ties the real classifier to the
//!    model's assumption: sigils inside string/comment/f-string zones must
//!    survive expansion byte-for-byte.
//! 3. **Model drift gate** — the tier order + dictionary domain the Lean
//!    proofs quantify over is pinned in `verification/model-manifest.txt`;
//!    any change to either fails here until the manifest AND the Lean model
//!    are updated together.

use pyths_expand::decorators::ALIASES as DECORATOR_ALIASES;
use pyths_expand::hooks::ALIASES as HOOK_ALIASES;
use pyths_expand::kwargs::ALIASES as KWARG_ALIASES;
use pyths_expand::presets::PRESETS;
use pyths_expand::strings::ALIASES;
use pyths_expand::{expand_with_config, TIER_ORDER};
use std::collections::{HashMap, HashSet};

// ---------------------------------------------------------------------
// 1. Dictionary audit
// ---------------------------------------------------------------------

#[test]
fn dict_audit_aliases_unique() {
    let mut seen = HashSet::new();
    for a in ALIASES {
        assert!(
            seen.insert(a.alias),
            "duplicate alias `${}` — expansion would be ambiguous",
            a.alias
        );
    }
}

#[test]
fn dict_audit_canonicals_unique() {
    // Injectivity: compress (canonical → alias) must be well-defined,
    // otherwise `expand ∘ compress = id` (the Lean round-trip theorem's
    // InverseConsistent hypothesis) is vacuous for the real table.
    let mut seen = HashMap::new();
    for a in ALIASES {
        if let Some(prev) = seen.insert(a.canonical, a.alias) {
            panic!(
                "canonical {} maps from two aliases (`${}` and `${}`) — compression is ambiguous",
                a.canonical, prev, a.alias
            );
        }
    }
}

#[test]
fn dict_audit_canonicals_are_quoted_string_literals() {
    for a in ALIASES {
        let c = a.canonical.as_bytes();
        assert!(
            c.len() >= 2 && c[0] == b'"' && c[c.len() - 1] == b'"',
            "canonical for `${}` is not a double-quoted literal: {}",
            a.alias,
            a.canonical
        );
    }
}

#[test]
fn dict_audit_no_alias_is_a_canonical_payload() {
    // An alias whose `$`-form appears inside another entry's canonical
    // would re-expand on a second pass, breaking idempotence.
    for a in ALIASES {
        for b in ALIASES {
            assert!(
                !b.canonical.contains(&format!("${}", a.alias)),
                "canonical of `${}` contains alias sigil `${}`",
                b.alias,
                a.alias
            );
        }
    }
}

#[test]
fn dict_audit_expand_of_each_alias_yields_canonical() {
    // End-to-end through the real pipeline: `$alias` in code position
    // expands to exactly the canonical, for every entry.
    let empty = HashMap::new();
    for a in ALIASES {
        let src = format!("x = ${}\n", a.alias);
        let out = expand_with_config(&src, &empty, &empty);
        assert_eq!(
            out,
            format!("x = {}\n", a.canonical),
            "`${}` did not expand to its canonical",
            a.alias
        );
    }
}

// ---------------------------------------------------------------------
// 2. Zone-classifier property test (sigils in protected zones)
// ---------------------------------------------------------------------

/// Deterministic xorshift64* so the corpus is reproducible in CI.
struct Rng(u64);
impl Rng {
    fn next(&mut self) -> u64 {
        let mut x = self.0;
        x ^= x >> 12;
        x ^= x << 25;
        x ^= x >> 27;
        self.0 = x;
        x.wrapping_mul(0x2545F4914F6CDD1D)
    }
    fn pick<'a, T>(&mut self, xs: &'a [T]) -> &'a T {
        &xs[(self.next() % xs.len() as u64) as usize]
    }
}

#[test]
fn zone_classifier_property_sigils_in_literals_never_expand() {
    // The Lean zone-safety theorem: protPayloads (expand c src) =
    // protPayloads src. Here: a unique marker string carrying live
    // dictionary sigils (`$pad`, `$c1`, …) and idiom sigils (`%card`)
    // is embedded in every protected-zone kind; after the FULL pipeline
    // it must appear verbatim, while the same sigil in code position
    // (the control line) must expand.
    let empty = HashMap::new();
    let idioms: HashMap<String, String> =
        [("card".to_string(), "div(cls=\"card\")".to_string())].into();
    let sigils = ["$pad", "$c1", "$p1", "$fs", "$bg", "%card", "$brr", "$mar"];
    let mut rng = Rng(0x_5EED_2026_07_10);

    for case in 0..300 {
        let s1 = *rng.pick(&sigils);
        let s2 = *rng.pick(&sigils);
        let payload = format!("Z{case}q {s1} mid {s2} Z{case}e");
        let zone_kind = rng.next() % 5;
        let protected_line = match zone_kind {
            0 => format!("a = \"{payload}\"\n"),
            1 => format!("b = '{payload}'\n"),
            2 => format!("# {payload}\n"),
            3 => format!("c = f\"{payload} {{v}}\"\n"),
            _ => format!("d = \"\"\"{payload}\"\"\"\n"),
        };
        // Control: the same sigils in code position on a separate line.
        let src = format!("{protected_line}style = {{$pad: $p1}}\n");
        let out = expand_with_config(&src, &empty, &idioms);
        assert!(
            out.contains(&payload),
            "case {case} (zone {zone_kind}): protected payload was rewritten.\n  src: {src:?}\n  out: {out:?}"
        );
        assert!(
            out.contains("\"padding\"") && out.contains("\"12px\""),
            "case {case}: control code-position sigils did not expand.\n  out: {out:?}"
        );
    }
}

// ---------------------------------------------------------------------
// 2b. Tier A IS zone-aware — the line rewrite is gated by the shared
//     classifier (fixed 2026-07-14; was a pinned defect before that)
// ---------------------------------------------------------------------

/// Tier A (`lib.rs::expand_line`, presets + decorator aliases) is the one tier
/// that rewrites whole LINES rather than scanning characters. It asks the
/// shared zone classifier (`zones::line_start_states`) which lines begin in a
/// code zone, and rewrites only those: a preset marker or decorator alias
/// sitting alone on a line INSIDE a triple-quoted string is docstring text and
/// is emitted byte-for-byte, exactly as in every other tier.
///
/// In Lean this is `tierA_zone_safety_chars` (verification/PythExpandVerify.lean)
/// — the positive analogue of `zone_safety_chars` / `kwarg_zone_safety_chars` /
/// `hook_zone_safety_chars` / `idiom_zone_safety_chars`, ranging over the same
/// executable Tier A the differential (`diff_harness.py --tier tiera`) runs
/// against this compiler. If you change Tier A's zone behaviour, this test, the
/// Lean theorem and the differential corpus must move together.
#[test]
fn tier_a_is_zone_aware_inside_docstrings() {
    let empty = HashMap::new();
    let x = |src: &str| expand_with_config(src, &empty, &empty);

    // A preset marker alone on a line inside a `"""` docstring: VERBATIM.
    let src = "doc = \"\"\"\nR*\n\"\"\"\ny = 1\n";
    assert_eq!(
        x(src),
        src,
        "Tier A rewrote a preset marker inside a docstring — the zone gate in \
         lib.rs (zones::line_start_states) is not firing."
    );

    // A decorator alias inside a `'''` docstring: VERBATIM.
    assert_eq!(x("d = '''\n@c\n'''\n"), "d = '''\n@c\n'''\n");

    // Indented marker, trailing whitespace, several markers: all VERBATIM.
    let many = "\"\"\"\n  R+  \n@d\nT*\n\"\"\"\n";
    assert_eq!(x(many), many);

    // An unterminated single-quoted string protects the lines that follow.
    assert_eq!(x("s = \"oops\nR*\n"), "s = \"oops\nR*\n");
    // …and an escaped quote does not close the zone.
    assert_eq!(x("s = \"a\\\"b\nR*\n"), "s = \"a\\\"b\nR*\n");

    // A comment line is not a marker line.
    assert_eq!(x("# R*\n# @c\n"), "# R*\n# @c\n");

    // Control 1: a marker in a REAL code position immediately after a closed
    // docstring still expands.
    assert_eq!(
        x("doc = \"\"\"\nR*\n\"\"\"\nR*\n"),
        "doc = \"\"\"\nR*\n\"\"\"\nfrom pyths.react import component, use_state, \
         use_effect, use_callback, use_memo\n"
    );

    // Control 2: a line that merely CONTAINS a quote (closed on the same line)
    // leaves the next line in code.
    assert_eq!(x("s = \"R*\"\n@c\n"), "s = \"R*\"\n@component\n");
    assert_eq!(
        x("a = \"\"\"x\"\"\"\n@d\n"),
        "a = \"\"\"x\"\"\"\n@dataclass\n"
    );

    // Control 3: Tier A's other guard is unchanged — a marker that is not alone
    // on its (trimmed) line is untouched wherever it appears.
    assert_eq!(x("s = \"R*\"\n"), "s = \"R*\"\n");
    assert_eq!(x("x = R*\n"), "x = R*\n");
}

// ---------------------------------------------------------------------
// 3. Model drift gate (Rust ↔ Lean manifest)
// ---------------------------------------------------------------------

/// FNV-1a 64 — dependency-free stable hash for the manifest.
fn fnv1a64(bytes: &[u8]) -> u64 {
    let mut h: u64 = 0xcbf29ce484222325;
    for &b in bytes {
        h ^= b as u64;
        h = h.wrapping_mul(0x100000001b3);
    }
    h
}

#[test]
fn model_manifest_matches_rust_tier_order_and_dictionary() {
    let mut dict_lines: Vec<String> = ALIASES
        .iter()
        .map(|a| format!("{}={}", a.alias, a.canonical))
        .collect();
    dict_lines.sort();
    let dict_blob = dict_lines.join("\n");

    // Tier B (kwargs.rs) is now a CONCRETE Lean tier (`expandKwargStr` over
    // `committedKwargs`, generated by gen-kwarg-data.py) with its own
    // zone-safety proof and a decided alias-table-exactness theorem. Pin the
    // table here too, so neither side can move alone: Rust-side drift fails
    // this test; Lean-side drift fails `gen-kwarg-data.py --check` in CI.
    let mut kw_lines: Vec<String> = KWARG_ALIASES
        .iter()
        .map(|a| format!("{}={}", a.alias, a.canonical))
        .collect();
    kw_lines.sort();
    let kw_blob = kw_lines.join("\n");

    // Tiers A (presets + decorators), hooks and E are now CONCRETE Lean tiers
    // too (gap-closure 2026-07-14), each generated from the Rust table below
    // by its gen-*-data.py and each with a decided exactness theorem. Pin all
    // of them, so no table can move without the model being updated.
    let mut preset_lines: Vec<String> = PRESETS
        .iter()
        .map(|p| format!("{}={}", p.marker, p.expansion))
        .collect();
    preset_lines.sort();
    let preset_blob = preset_lines.join("\n");

    let mut deco_lines: Vec<String> = DECORATOR_ALIASES
        .iter()
        .map(|d| format!("{}={}", d.alias, d.canonical))
        .collect();
    deco_lines.sort();
    let deco_blob = deco_lines.join("\n");

    let mut hook_lines: Vec<String> = HOOK_ALIASES
        .iter()
        .map(|h| format!("{}={}", h.alias, h.canonical))
        .collect();
    hook_lines.sort();
    let hook_blob = hook_lines.join("\n");

    // Tier E has NO compiler-side table — the `%NAME` map is supplied by the
    // user's pyths.toml and is empty by default. What the Lean Tier-E proofs
    // and the differential pin is the SCANNER, over a committed FIXTURE table
    // (verification/idiom-table.toml). Hash the fixture file itself so it
    // cannot be edited on either side without this gate noticing. Normalize
    // CRLF so the hash is checkout-independent.
    let fixture_path = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../verification/idiom-table.toml"
    );
    let fixture =
        std::fs::read_to_string(fixture_path).expect("verification/idiom-table.toml is missing");
    let fixture_norm = fixture.replace("\r\n", "\n");

    let expected = format!(
        "tier-order: {}\ndict-entries: {}\ndict-fnv1a64: {:016x}\n\
         kwarg-entries: {}\nkwarg-fnv1a64: {:016x}\n\
         preset-entries: {}\npreset-fnv1a64: {:016x}\n\
         decorator-entries: {}\ndecorator-fnv1a64: {:016x}\n\
         hook-entries: {}\nhook-fnv1a64: {:016x}\n\
         idiom-fixture-fnv1a64: {:016x}\n",
        TIER_ORDER.join(","),
        ALIASES.len(),
        fnv1a64(dict_blob.as_bytes()),
        KWARG_ALIASES.len(),
        fnv1a64(kw_blob.as_bytes()),
        PRESETS.len(),
        fnv1a64(preset_blob.as_bytes()),
        DECORATOR_ALIASES.len(),
        fnv1a64(deco_blob.as_bytes()),
        HOOK_ALIASES.len(),
        fnv1a64(hook_blob.as_bytes()),
        fnv1a64(fixture_norm.as_bytes())
    );

    let manifest_path = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../verification/model-manifest.txt"
    );
    let on_disk = std::fs::read_to_string(manifest_path).unwrap_or_default();
    // Ignore comment lines so the manifest can document itself.
    let on_disk_body: String = on_disk
        .lines()
        .filter(|l| !l.starts_with('#') && !l.trim().is_empty())
        .map(|l| format!("{l}\n"))
        .collect();

    assert_eq!(
        on_disk_body, expected,
        "\nThe expander's tier order, `$NAME` dictionary, or Tier-B kwarg \
         table changed, but verification/model-manifest.txt was not updated.\n\
         The Lean proofs (verification/PythExpandVerify.lean) quantify over \
         exactly this order + domain — update the model, then paste the \
         following into the manifest (below its comment header):\n\n{expected}"
    );
}
