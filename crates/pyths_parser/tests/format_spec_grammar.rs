//! E3 — format-spec GRAMMAR PARITY (compile-time side).
//!
//! `tests/fixtures/format_spec_grammar.json` (generated from the pinned
//! CPython oracle by scripts/gen_format_spec_grammar.py) is THE statement of
//! the mini-language grammar. The runtime parser (parseFormatSpec) is pinned
//! to it by crates/pyths_runtime/js/format_diff_test.mjs; this test pins the
//! COMPILE-TIME parser (format_spec::parse) to the same rows:
//!
//!   - a grammar-valid row (`parse` non-null) must parse to EXACTLY the
//!     canonical opts (same fields the JS parser produces, and the same
//!     object build_opts_object lowers — minus `raw`);
//!   - a grammar-invalid row (`parse` null) must return None (the f-string
//!     lowering then routes through pyFormatDynamic so the RUNTIME raises
//!     CPython's exact error — the #108 silently-ignored-spec class).
//!
//! Two parsers, one fixture: they can no longer drift apart silently.

use pyths_parser::format_spec::{parse, Align, Grouping, Sign};
use serde_json::Value;

/// Pin: number of rows in the generated fixture (update together with a
/// deliberate `scripts/gen_format_spec_grammar.py` regeneration).
const EXPECTED_ROWS: usize = 172;

fn opts_json(spec: &pyths_parser::format_spec::FormatSpec) -> Value {
    let mut m = serde_json::Map::new();
    if let Some(c) = spec.fill {
        m.insert("fill".into(), Value::String(c.to_string()));
    }
    if let Some(a) = spec.align {
        let s = match a {
            Align::Left => "<",
            Align::Right => ">",
            Align::Center => "^",
            Align::PadAfterSign => "=",
        };
        m.insert("align".into(), Value::String(s.into()));
    }
    if let Some(sg) = spec.sign {
        let s = match sg {
            Sign::Plus => "+",
            Sign::Minus => "-",
            Sign::Space => " ",
        };
        m.insert("sign".into(), Value::String(s.into()));
    }
    if spec.coerce_zero {
        m.insert("z".into(), Value::Bool(true));
    }
    if spec.alt_form {
        m.insert("alt".into(), Value::Bool(true));
    }
    if spec.zero_pad {
        m.insert("zero".into(), Value::Bool(true));
    }
    if let Some(w) = spec.width {
        m.insert("width".into(), Value::Number(w.into()));
    }
    if let Some(g) = spec.grouping {
        let s = match g {
            Grouping::Comma => ",",
            Grouping::Underscore => "_",
        };
        m.insert("grouping".into(), Value::String(s.into()));
    }
    if let Some(p) = spec.precision {
        m.insert("precision".into(), Value::Number(p.into()));
    }
    if let Some(g) = spec.frac_grouping {
        let s = match g {
            Grouping::Comma => ",",
            Grouping::Underscore => "_",
        };
        m.insert("fracGrouping".into(), Value::String(s.into()));
    }
    if let Some(t) = spec.ty {
        use pyths_parser::format_spec::FormatType::*;
        let s = match t {
            Binary => "b",
            Char => "c",
            Decimal => "d",
            ExpLower => "e",
            ExpUpper => "E",
            FixedLower => "f",
            FixedUpper => "F",
            GeneralLower => "g",
            GeneralUpper => "G",
            LocaleDecimal => "n",
            Octal => "o",
            String => "s",
            HexLower => "x",
            HexUpper => "X",
            Percent => "%",
        };
        m.insert("type".into(), Value::String(s.into()));
    }
    Value::Object(m)
}

#[test]
fn test_format_spec_grammar_compile_time_parser_matches_fixture() {
    let fixture_path = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../tests/fixtures/format_spec_grammar.json"
    );
    let text = std::fs::read_to_string(fixture_path).expect("grammar fixture readable");
    let doc: Value = serde_json::from_str(&text).expect("grammar fixture is JSON");
    let rows = doc["rows"].as_array().expect("rows array");
    // Exact pins (codex r2 should-fix): the fixture is a GENERATED artifact
    // of a specific oracle — a silently shrunken or re-oracled fixture must
    // fail loud, not drift.
    assert_eq!(
        rows.len(),
        EXPECTED_ROWS,
        "fixture row count changed — regenerate deliberately and update the pin"
    );
    let oracle = doc["oracle"].as_str().expect("oracle field");
    assert!(
        oracle.starts_with("3.14"),
        "fixture generated against oracle {oracle}, expected the pinned 3.14 line"
    );

    let mut failures = Vec::new();
    for row in rows {
        let spec_str = row["spec"].as_str().unwrap();
        let expected = &row["parse"];
        match (parse(spec_str), expected.is_null()) {
            (None, true) => {}
            (Some(got), false) => {
                let got_json = opts_json(&got);
                if &got_json != expected {
                    failures.push(format!(
                        "{spec_str:?}: opts mismatch\n  rust: {got_json}\n  canon: {expected}"
                    ));
                }
            }
            (None, false) => {
                failures.push(format!(
                    "{spec_str:?}: rust parser REJECTS a grammar-valid spec (canon: {expected})"
                ));
            }
            (Some(got), true) => {
                failures.push(format!(
                    "{spec_str:?}: rust parser ACCEPTS a grammar-invalid spec (got {:?})",
                    opts_json(&got)
                ));
            }
        }
    }
    assert!(
        failures.is_empty(),
        "{} compile-time parser divergence(s) from the grammar fixture:\n{}",
        failures.len(),
        failures.join("\n")
    );
}
