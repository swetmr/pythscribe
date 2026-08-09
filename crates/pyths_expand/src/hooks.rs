//! Tier C v0 — hook-call shorthand.
//!
//! Aliases short identifiers (`us`, `ue`, `um`, `uc`, `ur`, `ux`) to
//! their canonical React hook names (`use_state`, `use_effect`, …),
//! but **only** when the alias is in identifier-call position —
//! i.e. a free-standing identifier immediately followed by `(`.
//!
//! Safety constraints (the shared classifier in [`crate::zones`]):
//!   * Skips inside `'…'`, `"…"` strings.
//!   * Skips after `#` line comments.
//!   * Only fires when the alias is at identifier start — the
//!     preceding byte is not an identifier-continuation character
//!     (alphanumeric or `_`).
//!   * Only fires when the alias is followed immediately by `(`.
//!     Variable references like `us` on their own pass through.
//!
//! Aliases are reserved identifier names in the `.psc` surface. Don't
//! introduce a local variable called `us` in `.psc` — it will be
//! rewritten to `use_state` if used as a function call. The audit
//! script flags zero-saving aliases — `use_ref` and `use_context` are
//! kept for symmetry even though their cl100k savings are small.

use crate::zones::{self, emit_chars, CodeStep, ScanState};

pub struct HookAlias {
    pub alias: &'static str,
    pub canonical: &'static str,
}

pub const ALIASES: &[HookAlias] = &[
    HookAlias {
        alias: "us",
        canonical: "use_state",
    },
    HookAlias {
        alias: "ue",
        canonical: "use_effect",
    },
    HookAlias {
        alias: "um",
        canonical: "use_memo",
    },
    HookAlias {
        alias: "uc",
        canonical: "use_callback",
    },
    HookAlias {
        alias: "ur",
        canonical: "use_ref",
    },
    HookAlias {
        alias: "ux",
        canonical: "use_context",
    },
];

pub fn lookup(alias: &str) -> Option<&'static str> {
    ALIASES
        .iter()
        .find(|a| a.alias == alias)
        .map(|a| a.canonical)
}

pub fn substitute(source: &str) -> String {
    let bytes = source.as_bytes();
    let n = bytes.len();
    let mut out = String::with_capacity(n);
    let mut i = 0;
    // Zone state from the SHARED classifier (`zones.rs`).
    let mut in_string: Option<ScanState> = None;
    // True when the previous emitted byte was an identifier-continue
    // character — used to gate alias matches to identifier start.
    let mut prev_ident_cont = false;
    // True when the most recent non-whitespace byte was `.` — the
    // next identifier is therefore an attribute access (`obj.us(…)`)
    // and must NOT be substituted, since `us` here is a method on
    // `obj`, not a free identifier referencing the hook.
    let mut prev_was_dot = false;

    while i < n {
        // Inside a string — preserve verbatim until the zone closes.
        if let Some(state) = in_string {
            let (len, next) = zones::string_step(bytes, i, state);
            emit_chars(&mut out, source, i, len);
            i += len;
            in_string = next;
            prev_ident_cont = false;
            prev_was_dot = false;
            continue;
        }

        match zones::code_step(source, bytes, i) {
            // Line comment.
            CodeStep::Comment { end } => {
                out.push_str(&source[i..end]);
                i = end;
                prev_ident_cont = false;
                prev_was_dot = false;
            }
            // String start — single or triple quoted.
            CodeStep::StringOpen { len, state } => {
                out.push_str(&source[i..i + len]);
                i += len;
                in_string = Some(state);
                prev_ident_cont = false;
                prev_was_dot = false;
            }
            CodeStep::Code { len } => {
                let b = bytes[i];

                // Identifier-start match for an alias.
                // Inhibit substitution when preceded by `.` (attribute access).
                if !prev_ident_cont && !prev_was_dot && is_identifier_start(b) {
                    let id_start = i;
                    let mut id_end = i;
                    while id_end < n && is_identifier_continue(bytes[id_end]) {
                        id_end += 1;
                    }
                    let id = &source[id_start..id_end];

                    // Hook substitution requires the identifier to be
                    // immediately followed by `(`.
                    let is_call = id_end < n && bytes[id_end] == b'(';

                    if is_call {
                        if let Some(canonical) = lookup(id) {
                            out.push_str(canonical);
                            i = id_end;
                            prev_ident_cont = false;
                            prev_was_dot = false;
                            continue;
                        }
                    }

                    // Pass identifier through.
                    out.push_str(id);
                    i = id_end;
                    prev_ident_cont = true;
                    prev_was_dot = false;
                    continue;
                }

                // Default: pass-through. Track flags for the next iteration.
                emit_chars(&mut out, source, i, len);
                i += len;
                prev_ident_cont = is_identifier_continue(b);
                if b == b'.' {
                    prev_was_dot = true;
                } else if b != b' ' && b != b'\t' {
                    // Whitespace (space, tab) preserves `prev_was_dot` so
                    // `obj . us(…)` still inhibits the substitution. Any
                    // other character clears it.
                    prev_was_dot = false;
                }
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
