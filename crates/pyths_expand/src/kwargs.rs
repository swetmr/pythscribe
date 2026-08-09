//! Tier B kwarg-position substitution.
//!
//! Replaces short aliases (`st=`, `oc=`, `oh=`, …) with their canonical
//! kwarg names (`style=`, `on_click=`, `on_change=`, …) — but **only**
//! when the alias is in argument position (immediately preceded by `(`
//! or `,`, possibly with whitespace, and immediately followed by `=`
//! that is not part of `==`). Substitutions are skipped inside strings
//! and `#`-comments.
//!
//! Multi-line argument lists are handled: an alias on a continuation
//! line still substitutes because the `at_arg_start` flag persists
//! across whitespace (including newlines).
//!
//! Limitations (acceptable for v1):
//!   * Substitution does not recurse into f-string interpolations
//!     (`f"… {x} …"`) — the entire f-string is treated as opaque.

use crate::zones::{self, emit_chars, CodeStep, ScanState};

pub struct KwargAlias {
    pub alias: &'static str,
    pub canonical: &'static str,
}

pub const ALIASES: &[KwargAlias] = &[
    KwargAlias {
        alias: "st",
        canonical: "style",
    },
    KwargAlias {
        alias: "cn",
        canonical: "class_name",
    },
    KwargAlias {
        alias: "cl",
        canonical: "className",
    },
    KwargAlias {
        alias: "oc",
        canonical: "on_click",
    },
    KwargAlias {
        alias: "oh",
        canonical: "on_change",
    },
    KwargAlias {
        alias: "os",
        canonical: "on_submit",
    },
    KwargAlias {
        alias: "oa",
        canonical: "on_acknowledge",
    },
    KwargAlias {
        alias: "ph",
        canonical: "placeholder",
    },
    KwargAlias {
        alias: "dis",
        canonical: "disabled",
    },
];

pub fn lookup(alias: &str) -> Option<&'static str> {
    ALIASES
        .iter()
        .find(|a| a.alias == alias)
        .map(|a| a.canonical)
}

/// Apply kwarg-position substitution across an entire source string.
///
/// Operates on the post-preset-expansion text. Preserves all bytes
/// outside of substitution sites exactly (including multi-byte UTF-8
/// characters in strings, comments, and identifiers — though the
/// alias table itself is ASCII-only).
pub fn substitute(source: &str) -> String {
    let bytes = source.as_bytes();
    let n = bytes.len();
    let mut out = String::with_capacity(n);
    let mut i = 0;
    // Zone state from the SHARED classifier (`zones.rs`).
    let mut in_string: Option<ScanState> = None;
    let mut at_arg_start = false;

    while i < n {
        // Inside a string — preserve everything until we close.
        if let Some(state) = in_string {
            let (len, next) = zones::string_step(bytes, i, state);
            emit_chars(&mut out, source, i, len);
            i += len;
            in_string = next;
            continue;
        }

        match zones::code_step(source, bytes, i) {
            // Line comment — pass through to the end of the line.
            CodeStep::Comment { end } => {
                out.push_str(&source[i..end]);
                i = end;
                at_arg_start = false;
            }
            // String start (`'`, `"`, `'''`, `"""`).
            CodeStep::StringOpen { len, state } => {
                out.push_str(&source[i..i + len]);
                i += len;
                in_string = Some(state);
                at_arg_start = false;
            }
            CodeStep::Code { len } => {
                let b = bytes[i];

                // Whitespace and newlines — preserve `at_arg_start` so a kwarg
                // can appear on a continuation line.
                if b == b' ' || b == b'\t' || b == b'\r' || b == b'\n' {
                    out.push(b as char);
                    i += 1;
                    continue;
                }

                // Argument-start delimiters.
                if b == b'(' || b == b',' {
                    out.push(b as char);
                    i += 1;
                    at_arg_start = true;
                    continue;
                }

                // Try to match an alias at an arg-start position.
                if at_arg_start && is_identifier_start(b) {
                    let id_start = i;
                    let mut id_end = i;
                    while id_end < n && is_identifier_continue(bytes[id_end]) {
                        id_end += 1;
                    }
                    let id = &source[id_start..id_end];

                    // Followed by `=` (and NOT `==`).
                    let is_kwarg_eq =
                        id_end < n && bytes[id_end] == b'=' && bytes.get(id_end + 1) != Some(&b'=');

                    if is_kwarg_eq {
                        if let Some(canonical) = lookup(id) {
                            out.push_str(canonical);
                            out.push('=');
                            i = id_end + 1;
                            at_arg_start = false;
                            continue;
                        }
                    }

                    // No alias — emit the identifier as-is and clear the flag.
                    out.push_str(id);
                    i = id_end;
                    at_arg_start = false;
                    continue;
                }

                // Anything else (including multi-byte UTF-8 leading bytes)
                // closes the arg-start window and passes through verbatim.
                emit_chars(&mut out, source, i, len);
                i += len;
                at_arg_start = false;
            }
        }
    }

    out
}

fn is_identifier_start(b: u8) -> bool {
    b.is_ascii_alphabetic() || b == b'_'
}

fn is_identifier_continue(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'_'
}
