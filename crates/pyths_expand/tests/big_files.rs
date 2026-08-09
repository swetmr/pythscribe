//! Integration tests against the two big benchmark fixtures —
//! `dashboard_500.psc` and `app_1000.psc`. These files exercise every
//! Tier A substitution at scale (multi-class, multi-component, multi-import)
//! and are the showcase examples for Phase 2 in the design doc.

use std::path::PathBuf;

use pyths_expand::expand;

fn sample(name: &str) -> String {
    let mut path = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    path.pop(); // crates/pyths_expand -> crates
    path.pop(); // crates -> workspace root
    path.push("examples");
    path.push("cloudflare-bench");
    path.push("large-samples");
    path.push("pythscribe");
    path.push(name);
    std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("failed to read {}: {}", path.display(), e))
}

// ---------------------------------------------------------------------------
// dashboard_500
// ---------------------------------------------------------------------------

#[test]
fn dashboard_500_psc_expands_react_preset() {
    let expanded = expand(&sample("dashboard_500.psc"));
    assert!(
        expanded.contains(
            "from pyths.react import component, use_state, use_effect, use_callback, use_memo"
        ),
        "expanded output is missing the React import line"
    );
}

#[test]
fn dashboard_500_psc_expands_dataclass_preset() {
    let expanded = expand(&sample("dashboard_500.psc"));
    assert!(
        expanded.contains("from dataclasses import dataclass"),
        "expanded output is missing the dataclass import"
    );
}

#[test]
fn dashboard_500_psc_expands_all_component_decorators() {
    let expanded = expand(&sample("dashboard_500.psc"));
    let count = expanded.matches("@component").count();
    assert_eq!(
        count, 10,
        "expected 10 @component decorators after expansion, got {}",
        count
    );
}

#[test]
fn dashboard_500_psc_expands_all_dataclass_decorators() {
    let expanded = expand(&sample("dashboard_500.psc"));
    let count = expanded.matches("@dataclass").count();
    assert_eq!(
        count, 3,
        "expected 3 @dataclass decorators after expansion, got {}",
        count
    );
}

#[test]
fn dashboard_500_psc_leaves_no_aliases_unexpanded() {
    let expanded = expand(&sample("dashboard_500.psc"));
    for line in expanded.lines() {
        let trimmed = line.trim();
        assert!(
            trimmed != "@c" && trimmed != "@d",
            "found unexpanded alias line: {:?}",
            line
        );
        assert!(
            trimmed != "R*" && trimmed != "T*",
            "found unexpanded preset line: {:?}",
            line
        );
    }
    // Tier B: no kwarg aliases should survive expansion either.
    for alias in [
        "st=", "cl=", "cn=", "oc=", "oh=", "os=", "oa=", "ph=", "dis=",
    ] {
        assert!(
            !expanded.contains(alias),
            "found unexpanded Tier B alias {:?} in dashboard_500",
            alias
        );
    }
}

#[test]
fn dashboard_500_psc_expands_tier_b_kwargs() {
    let expanded = expand(&sample("dashboard_500.psc"));
    // Each canonical kwarg appears at least once in the expanded output.
    for canonical in [
        "style=",
        "className=",
        "on_click=",
        "on_change=",
        "on_acknowledge=",
        "placeholder=",
        "disabled=",
    ] {
        assert!(
            expanded.contains(canonical),
            "expanded output is missing canonical kwarg {:?}",
            canonical
        );
    }
}

#[test]
fn dashboard_500_psc_expansion_is_lexable() {
    let expanded = expand(&sample("dashboard_500.psc"));
    let result = pyths_lexer::lex_recovering(&expanded);
    assert!(
        result.errors.is_empty(),
        "expanded dashboard_500 fails lexer with {} error(s): {:?}",
        result.errors.len(),
        result.errors
    );
}

#[test]
fn dashboard_500_psc_is_smaller_than_expanded() {
    // Byte-size proxy for "compression actually compresses something".
    let psc = sample("dashboard_500.psc");
    let expanded = expand(&psc);
    assert!(
        psc.len() < expanded.len(),
        "compressed .psc ({} bytes) should be smaller than expanded form ({} bytes)",
        psc.len(),
        expanded.len()
    );
}

#[test]
fn dashboard_500_psc_is_smaller_than_ps() {
    // The .psc fixture must be at least slightly smaller than the
    // hand-written .ps fixture; if not, the format isn't earning its
    // keep on this file.
    let psc = sample("dashboard_500.psc");
    let ps = sample("dashboard_500.ps");
    assert!(
        psc.len() < ps.len(),
        ".psc ({} bytes) should be smaller than .ps ({} bytes)",
        psc.len(),
        ps.len()
    );
}

// ---------------------------------------------------------------------------
// app_1000
// ---------------------------------------------------------------------------

#[test]
fn app_1000_psc_expands_all_three_presets() {
    let expanded = expand(&sample("app_1000.psc"));
    assert!(
        expanded.contains(
            "from pyths.react import component, use_state, use_effect, use_callback, use_memo"
        ),
        "missing React import"
    );
    assert!(
        expanded.contains("from pyths.asyncio import gather, sleep"),
        "missing asyncio import"
    );
    assert!(
        expanded.contains("from dataclasses import dataclass, Field"),
        "missing dataclass+Field import"
    );
}

#[test]
fn app_1000_psc_expands_all_component_decorators() {
    let expanded = expand(&sample("app_1000.psc"));
    let count = expanded.matches("@component").count();
    assert_eq!(
        count, 17,
        "expected 17 @component decorators after expansion, got {}",
        count
    );
}

#[test]
fn app_1000_psc_expands_all_dataclass_decorators() {
    let expanded = expand(&sample("app_1000.psc"));
    let count = expanded.matches("@dataclass").count();
    assert_eq!(
        count, 5,
        "expected 5 @dataclass decorators after expansion, got {}",
        count
    );
}

#[test]
fn app_1000_psc_leaves_no_aliases_unexpanded() {
    let expanded = expand(&sample("app_1000.psc"));
    for line in expanded.lines() {
        let trimmed = line.trim();
        assert!(
            trimmed != "@c" && trimmed != "@d",
            "found unexpanded alias line: {:?}",
            line
        );
        assert!(
            trimmed != "R*" && trimmed != "A*" && trimmed != "T+" && trimmed != "T*",
            "found unexpanded preset line: {:?}",
            line
        );
    }
    // Tier B aliases must all be expanded.
    for alias in [
        "st=", "cl=", "cn=", "oc=", "oh=", "os=", "oa=", "ph=", "dis=",
    ] {
        assert!(
            !expanded.contains(alias),
            "found unexpanded Tier B alias {:?} in app_1000",
            alias
        );
    }
}

#[test]
fn app_1000_psc_expands_tier_b_kwargs() {
    let expanded = expand(&sample("app_1000.psc"));
    // The CRM app uses `style=`, `on_click=`, `on_change=`, `placeholder=`
    // — verify each appears in the expanded output.
    for canonical in ["style=", "on_click=", "on_change=", "placeholder="] {
        assert!(
            expanded.contains(canonical),
            "expanded output is missing canonical kwarg {:?}",
            canonical
        );
    }
}

#[test]
fn app_1000_psc_expansion_is_lexable() {
    let expanded = expand(&sample("app_1000.psc"));
    let result = pyths_lexer::lex_recovering(&expanded);
    assert!(
        result.errors.is_empty(),
        "expanded app_1000 fails lexer with {} error(s): {:?}",
        result.errors.len(),
        result.errors
    );
}

#[test]
fn app_1000_psc_is_smaller_than_ps() {
    let psc = sample("app_1000.psc");
    let ps = sample("app_1000.ps");
    assert!(
        psc.len() < ps.len(),
        ".psc ({} bytes) should be smaller than .ps ({} bytes)",
        psc.len(),
        ps.len()
    );
}

// ---------------------------------------------------------------------------
// Cross-file properties
// ---------------------------------------------------------------------------

#[test]
fn expand_is_idempotent_on_expanded_output() {
    for name in ["dashboard_500.psc", "app_1000.psc"] {
        let expanded_once = expand(&sample(name));
        let expanded_twice = expand(&expanded_once);
        assert_eq!(
            expanded_once, expanded_twice,
            "expand() is not idempotent on {}",
            name
        );
    }
}

#[test]
fn expanded_hook_counts_match_original_ps() {
    for name in ["dashboard_500", "app_1000"] {
        let expanded = expand(&sample(&format!("{}.psc", name)));
        let ps = sample(&format!("{}.ps", name));
        for canonical in ["use_state(", "use_effect("] {
            let expanded_count = expanded.matches(canonical).count();
            let ps_count = ps.matches(canonical).count();
            assert_eq!(
                expanded_count, ps_count,
                "{}: count of {:?} in expanded ({}) doesn't match .ps ({})",
                name, canonical, expanded_count, ps_count
            );
        }
    }
}

#[test]
fn no_hook_aliases_survive_in_expanded_files() {
    for name in ["dashboard_500", "app_1000"] {
        let expanded = expand(&sample(&format!("{}.psc", name)));
        // A bare `us(` / `ue(` / etc. in expanded output would mean
        // the substitution missed.
        for alias in ["us(", "ue(", "um(", "uc(", "ur(", "ux("] {
            // Allow these as substrings in the middle of longer
            // identifiers (e.g., `bus(` would match `us(`); use
            // word-boundary check.
            for line in expanded.lines() {
                let mut idx = 0;
                while let Some(found) = line[idx..].find(alias) {
                    let pos = idx + found;
                    let before_is_ident = pos > 0 && {
                        let prev = line.as_bytes()[pos - 1];
                        prev.is_ascii_alphanumeric() || prev == b'_'
                    };
                    assert!(
                        before_is_ident,
                        "{}: hook alias {:?} survives expansion in line: {:?}",
                        name, alias, line
                    );
                    idx = pos + alias.len();
                }
            }
        }
    }
}

#[test]
fn no_dictionary_aliases_survive_in_expanded_files() {
    // Phase 2.10: every `$NAME` token in `.psc` must expand. We
    // can't just assert `!expanded.contains('$')` because `$` is a
    // valid character inside string literals (e.g. `f"${price}"`
    // for currency formatting). Instead, run a tiny scanner that
    // checks for `$` only outside strings/comments.
    for name in ["dashboard_500", "app_1000"] {
        let expanded = expand(&sample(&format!("{}.psc", name)));
        let bytes = expanded.as_bytes();
        let mut in_string: Option<u8> = None;
        let mut in_comment = false;
        for (i, &b) in bytes.iter().enumerate() {
            if in_comment {
                if b == b'\n' {
                    in_comment = false;
                }
                continue;
            }
            if let Some(quote) = in_string {
                if b == b'\\' {
                    continue;
                }
                if b == quote {
                    in_string = None;
                }
                continue;
            }
            if b == b'#' {
                in_comment = true;
                continue;
            }
            if b == b'\'' || b == b'"' {
                in_string = Some(b);
                continue;
            }
            assert!(
                b != b'$',
                "{}: stray `$` at byte {} outside strings/comments — \
                 alias expansion missed something",
                name,
                i
            );
        }
    }
}

#[test]
fn expanded_dictionary_strings_match_original_ps() {
    // Verify that dictionary aliases round-trip: the canonical hex
    // and px-value strings appear in the expanded output at the same
    // count they appear in the original `.ps` file.
    for name in ["dashboard_500", "app_1000"] {
        let expanded = expand(&sample(&format!("{}.psc", name)));
        let ps = sample(&format!("{}.ps", name));
        for canonical in [
            "\"#9ca3af\"",
            "\"#ffffff\"",
            "\"#6b7280\"",
            "\"#3b82f6\"",
            "\"#dc2626\"",
            "\"12px\"",
            "\"10px\"",
            "\"16px\"",
            "\"1px solid #d1d5db\"",
        ] {
            let exp_count = expanded.matches(canonical).count();
            let ps_count = ps.matches(canonical).count();
            assert_eq!(
                exp_count, ps_count,
                "{}: canonical {:?} count {} doesn't match .ps count {}",
                name, canonical, exp_count, ps_count
            );
        }
    }
}

#[test]
fn expanded_kwarg_counts_match_original_ps() {
    // Sanity check: after expansion, every kwarg-canonical form occurs
    // at the same count as in the hand-written `.ps`. If we ever drop
    // a substitution, this catches it.
    for name in ["dashboard_500", "app_1000"] {
        let expanded = expand(&sample(&format!("{}.psc", name)));
        let ps = sample(&format!("{}.ps", name));
        for canonical in [
            "style=",
            "className=",
            "on_click=",
            "on_change=",
            "placeholder=",
            "disabled=",
            "on_acknowledge=",
        ] {
            let expanded_count = expanded.matches(canonical).count();
            let ps_count = ps.matches(canonical).count();
            assert_eq!(
                expanded_count, ps_count,
                "{}: count of {:?} in expanded ({}) doesn't match .ps ({})",
                name, canonical, expanded_count, ps_count
            );
        }
    }
}

#[test]
fn tier_b_delivers_meaningful_byte_savings() {
    // With Tier B aliases applied, the .psc must be at least 3% smaller
    // than the .ps. This guards against accidental regression of the
    // kwarg-substitution pass.
    for name in ["dashboard_500", "app_1000"] {
        let psc = sample(&format!("{}.psc", name));
        let ps = sample(&format!("{}.ps", name));
        let saved_pct = 100.0 * (ps.len() as f64 - psc.len() as f64) / ps.len() as f64;
        assert!(
            saved_pct >= 3.0,
            "{}: Tier B savings dropped below 3% (got {:.2}%)",
            name,
            saved_pct
        );
    }
}

#[test]
fn expanded_size_within_reasonable_growth_factor() {
    // Tier A expansion should not blow up output by more than ~25%
    // (preset macros expand small markers into longer lines, but
    // most of the file is body code that passes through).
    for name in ["dashboard_500.psc", "app_1000.psc"] {
        let psc = sample(name);
        let expanded = expand(&psc);
        let growth = expanded.len() as f64 / psc.len() as f64;
        assert!(
            growth < 1.25,
            "{}: expansion grew by {:.2}x, which exceeds the 1.25x guard",
            name,
            growth
        );
    }
}
