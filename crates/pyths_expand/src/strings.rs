//! Tier C+ — domain-dictionary substitution for common string literals.
//!
//! Recognises `$NAME` patterns (sentinel `$` is invalid Python, so
//! collision with user identifiers is structurally impossible) and
//! expands them to canonical string literals from a small registry
//! seeded from the actual frequency of strings in the React+CRM
//! benchmark corpus.
//!
//! ## Why `$` as the sentinel
//!
//! `$` is **not a valid Python token character** — so any `$NAME` in
//! the source is unambiguously a `.psc` alias. There's no risk of
//! confusing it with a user variable. If the expander finds an
//! unknown `$NAME`, it leaves it unchanged so the downstream lexer
//! produces a clear error.
//!
//! ## Categories (initial registry)
//!
//! * **Colors** (`$c1` … `$cE`): hex codes from the benchmark corpus,
//!   ordered by frequency. Most common is `$c1` → `"#9ca3af"`
//!   (gray-400, 11 occurrences).
//! * **Pixel sizes** (`$p1` … `$pA`): CSS px values from the corpus.
//! * **Other CSS** (`$ff` → `"system-ui, sans-serif"`, etc.): a few
//!   long stereotyped strings that recur.
//!
//! Safety constraints (the shared classifier in [`crate::zones`], as used
//! by every other tier):
//!   * Skip inside `'…'`, `"…"`, `'''…'''`, `"""…"""` strings.
//!   * Skip after `#` line comments.
//!   * Match `$` only when followed by alphanumeric (the alias name);
//!     a bare `$` passes through untouched.

use crate::zones::{self, emit_chars, CodeStep, ScanState};

pub struct StringAlias {
    pub alias: &'static str,
    pub canonical: &'static str,
}

/// Initial dictionary seeded from the corpus frequency analysis.
/// Run `bench/audit_strings.py` (future) to refresh this from
/// arbitrary corpora.
pub const ALIASES: &[StringAlias] = &[
    // ---- Hex colors (ordered by corpus frequency) ----
    StringAlias {
        alias: "c1",
        canonical: "\"#9ca3af\"",
    }, // gray-400, 11x
    StringAlias {
        alias: "c2",
        canonical: "\"#ffffff\"",
    }, // white, 7x
    StringAlias {
        alias: "c3",
        canonical: "\"#6b7280\"",
    }, // gray-500, 7x
    StringAlias {
        alias: "c4",
        canonical: "\"#3b82f6\"",
    }, // blue-500, 5x
    StringAlias {
        alias: "c5",
        canonical: "\"#f9fafb\"",
    }, // gray-50, 4x
    StringAlias {
        alias: "c6",
        canonical: "\"#dc2626\"",
    }, // red-600, 4x
    StringAlias {
        alias: "c7",
        canonical: "\"#374151\"",
    }, // gray-700, 3x
    StringAlias {
        alias: "c8",
        canonical: "\"#f59e0b\"",
    }, // amber-500, 2x
    StringAlias {
        alias: "c9",
        canonical: "\"#16a34a\"",
    }, // green-600, 2x
    StringAlias {
        alias: "cA",
        canonical: "\"#fffbeb\"",
    }, // amber-50
    StringAlias {
        alias: "cB",
        canonical: "\"#fef2f2\"",
    }, // red-50
    StringAlias {
        alias: "cC",
        canonical: "\"#f3f4f6\"",
    }, // gray-100
    StringAlias {
        alias: "cD",
        canonical: "\"#eff6ff\"",
    }, // blue-50
    StringAlias {
        alias: "cE",
        canonical: "\"#7c3aed\"",
    }, // purple-600
    StringAlias {
        alias: "cF",
        canonical: "\"#e5e7eb\"",
    }, // gray-200
    StringAlias {
        alias: "cG",
        canonical: "\"#d1d5db\"",
    }, // gray-300
    // ---- Pixel sizes (corpus-frequency ordered) ----
    StringAlias {
        alias: "p1",
        canonical: "\"12px\"",
    }, // 17x
    StringAlias {
        alias: "p2",
        canonical: "\"10px\"",
    }, // 14x
    StringAlias {
        alias: "p3",
        canonical: "\"11px\"",
    }, // 9x
    StringAlias {
        alias: "p4",
        canonical: "\"16px\"",
    }, // 8x
    StringAlias {
        alias: "p5",
        canonical: "\"6px\"",
    }, // 7x
    StringAlias {
        alias: "p6",
        canonical: "\"8px\"",
    }, // 6x
    StringAlias {
        alias: "p7",
        canonical: "\"24px\"",
    }, // 6x
    StringAlias {
        alias: "p8",
        canonical: "\"14px\"",
    }, // 5x
    StringAlias {
        alias: "p9",
        canonical: "\"4px\"",
    }, // 3x
    StringAlias {
        alias: "pA",
        canonical: "\"20px\"",
    }, // 2x
    // ---- Other stereotyped strings ----
    StringAlias {
        alias: "ff",
        canonical: "\"system-ui, sans-serif\"",
    },
    StringAlias {
        alias: "br1",
        canonical: "\"1px solid #d1d5db\"",
    }, // 7x in corpus
    StringAlias {
        alias: "br2",
        canonical: "\"1px solid #e5e7eb\"",
    }, // 6x
    StringAlias {
        alias: "br3",
        canonical: "\"1px solid #f3f4f6\"",
    }, // 1x
    // ---- CSS property KEYS (frequency-ordered; Phase 2.11) ----
    // These appear as dict keys inside style={...} — e.g. {$pad: $p4}.
    // Multi-token canonical names (snake_case) compress best on cl100k.
    StringAlias {
        alias: "pad",
        canonical: "\"padding\"",
    }, // 38x
    StringAlias {
        alias: "fs",
        canonical: "\"font_size\"",
    }, // 24x
    StringAlias {
        alias: "col",
        canonical: "\"color\"",
    }, // 23x
    StringAlias {
        alias: "dsp",
        canonical: "\"display\"",
    }, // 14x
    StringAlias {
        alias: "brr",
        canonical: "\"border_radius\"",
    }, // 11x
    StringAlias {
        alias: "bd",
        canonical: "\"border\"",
    }, // 11x
    StringAlias {
        alias: "bg",
        canonical: "\"background\"",
    }, // 11x
    StringAlias {
        alias: "mb",
        canonical: "\"margin_bottom\"",
    }, // 10x
    StringAlias {
        alias: "mar",
        canonical: "\"margin\"",
    }, // 9x
    StringAlias {
        alias: "ta",
        canonical: "\"text_align\"",
    }, // 6x
    StringAlias {
        alias: "wd",
        canonical: "\"width\"",
    }, // 6x
    StringAlias {
        alias: "gtc",
        canonical: "\"grid_template_columns\"",
    }, // 4x; very long canonical
    StringAlias {
        alias: "bb",
        canonical: "\"border_bottom\"",
    }, // 4x
    StringAlias {
        alias: "mh",
        canonical: "\"min_height\"",
    }, // 3x
    StringAlias {
        alias: "fw",
        canonical: "\"font_weight\"",
    }, // 3x
    StringAlias {
        alias: "cur",
        canonical: "\"cursor\"",
    }, // 3x
    StringAlias {
        alias: "ai",
        canonical: "\"align_items\"",
    }, // 3x
    StringAlias {
        alias: "jc",
        canonical: "\"justify_content\"",
    }, // 2x
    StringAlias {
        alias: "mw",
        canonical: "\"max_width\"",
    }, // 2x
    StringAlias {
        alias: "mt",
        canonical: "\"margin_top\"",
    }, // 2x
    StringAlias {
        alias: "blt",
        canonical: "\"border_left\"",
    }, // 1x but long
    StringAlias {
        alias: "ff_k",
        canonical: "\"font_family\"",
    }, // 1x
    StringAlias {
        alias: "ws",
        canonical: "\"white_space\"",
    }, // 1x but long
    StringAlias {
        alias: "ls",
        canonical: "\"list_style\"",
    }, // 1x
    // ---- CSS property VALUES (multi-token canonicals only) ----
    StringAlias {
        alias: "sbw",
        canonical: "\"space-between\"",
    }, // 2x; multi-token via hyphen
    StringAlias {
        alias: "ib",
        canonical: "\"inline-block\"",
    }, // 2x; hyphen
    StringAlias {
        alias: "pwr",
        canonical: "\"pre-wrap\"",
    }, // 1x; hyphen
    StringAlias {
        alias: "pct",
        canonical: "\"100%\"",
    }, // 6x
    StringAlias {
        alias: "vh",
        canonical: "\"100vh\"",
    }, // 2x
    StringAlias {
        alias: "w6",
        canonical: "\"600\"",
    }, // 2x (font-weight string)
    StringAlias {
        alias: "w7",
        canonical: "\"700\"",
    }, // 1x
];

pub fn lookup(alias: &str) -> Option<&'static str> {
    ALIASES
        .iter()
        .find(|a| a.alias == alias)
        .map(|a| a.canonical)
}

/// Look up an alias, preferring `user_dict` over the bundled table.
///
/// User entries may be plain strings (without quotes) or already-quoted
/// literals. We accept either: if the value starts and ends with the same
/// quote character, it's emitted verbatim; otherwise it's wrapped in
/// double quotes. This keeps `pyths.toml` ergonomic while still allowing
/// users to opt into single-quoted output or escape sequences.
fn lookup_with_user<'a>(
    alias: &str,
    user_dict: &'a std::collections::HashMap<String, String>,
    scratch: &'a mut String,
) -> Option<&'a str> {
    if let Some(raw) = user_dict.get(alias) {
        let bytes = raw.as_bytes();
        if bytes.len() >= 2
            && (bytes[0] == b'"' || bytes[0] == b'\'')
            && bytes[bytes.len() - 1] == bytes[0]
        {
            *scratch = raw.clone();
        } else {
            scratch.clear();
            scratch.push('"');
            scratch.push_str(raw);
            scratch.push('"');
        }
        return Some(scratch.as_str());
    }
    lookup(alias)
}

pub fn substitute(source: &str) -> String {
    substitute_with_dict(source, &std::collections::HashMap::new())
}

pub fn substitute_with_dict(
    source: &str,
    user_dict: &std::collections::HashMap<String, String>,
) -> String {
    let bytes = source.as_bytes();
    let n = bytes.len();
    let mut out = String::with_capacity(n);
    let mut i = 0;
    // Zone state comes from the SHARED classifier (`zones.rs`) — the same
    // one Tier A, Tier B, hooks and idioms use, and the one the Lean
    // `protChars` companion mirrors.
    let mut in_string: Option<ScanState> = None;

    while i < n {
        // Inside a string — preserve verbatim until the zone closes.
        if let Some(state) = in_string {
            let (len, next) = zones::string_step(bytes, i, state);
            emit_chars(&mut out, source, i, len);
            i += len;
            in_string = next;
            continue;
        }

        match zones::code_step(source, bytes, i) {
            // Line comment — protected through the end of the line.
            CodeStep::Comment { end } => {
                out.push_str(&source[i..end]);
                i = end;
            }
            // String opener (`'`, `"`, `'''`, `"""`).
            CodeStep::StringOpen { len, state } => {
                out.push_str(&source[i..i + len]);
                i += len;
                in_string = Some(state);
            }
            // Code zone — the only place a `$NAME` alias may expand.
            CodeStep::Code { len } => {
                let b = bytes[i];
                if b == b'$'
                    && i + 1 < n
                    && (bytes[i + 1].is_ascii_alphanumeric() || bytes[i + 1] == b'_')
                {
                    let id_start = i + 1;
                    let mut id_end = id_start;
                    while id_end < n
                        && (bytes[id_end].is_ascii_alphanumeric() || bytes[id_end] == b'_')
                    {
                        id_end += 1;
                    }
                    let alias = &source[id_start..id_end];
                    let mut scratch = String::new();
                    if let Some(canonical) = lookup_with_user(alias, user_dict, &mut scratch) {
                        out.push_str(canonical);
                        i = id_end;
                        continue;
                    }
                    // Unknown alias — leave `$NAME` literal so the downstream
                    // lexer produces a clear "unexpected $" diagnostic. Emit
                    // the `$` byte and continue (the identifier bytes get
                    // emitted by subsequent iterations as default passthrough).
                    out.push('$');
                    i += 1;
                    continue;
                }

                // Default passthrough (UTF-8 aware).
                emit_chars(&mut out, source, i, len);
                i += len;
            }
        }
    }

    out
}
