//! Tier E-fixed — sigil-idiom expander.
//!
//! Recognises `%NAME` patterns (sentinel `%` is not a valid Python token
//! character in identifier position, so collision with user identifiers is
//! structurally impossible) and expands them to canonical **code fragments**
//! from an `[expand.idioms]` table in `pyths.toml`.
//!
//! ## Why `%` as the sentinel
//!
//! `%` is the Python modulo operator but **never** starts an identifier.
//! So any `%NAME` (where `NAME` is `[A-Za-z0-9_]+`) in a `.psc` source is
//! unambiguously a Tier E idiom sigil — no collision with user identifiers.
//! Note: idiom names beginning with a digit (e.g., `%10`) are accepted by the
//! scanner but discouraged, because a map entry like `"10"` would silently
//! intercept a Python modulo expression such as `x % 10`.
//! Unlike `$NAME` (which expands to a string *literal*), `%NAME` expands to
//! an already-canonical **code fragment** that may span multiple lines.
//!
//! ## Pipeline placement
//!
//! Idiom substitution runs **FIRST** in `expand_with_config` — before PSX,
//! Tier A presets, Tier B kwargs, hooks, and domain-dictionary passes.
//! Rationale: idiom values are already-canonical code fragments. Expanding
//! them first lets any aliases they happen to contain (e.g. `$c1`, kwarg
//! shorthands, hook aliases) flow through the later passes harmlessly. Running
//! idioms last would silently suppress those inner substitutions.
//!
//! ## Safety constraints (same scanner family as `kwargs.rs` / `hooks.rs`)
//!
//! * Skip inside `'…'`, `"…"`, `f"…"`, `f'…'` single-quoted strings.
//! * Skip inside `'''…'''` and `"""…"""` triple-quoted strings (docstrings).
//! * Skip after `#` line comments.
//! * Match `%` only when followed by an alphanumeric or `_` (the alias name);
//!   a bare `%` or `%` followed by an operator/whitespace passes through
//!   untouched.
//! * Unknown `%NAME` (not in the config map) passes through unchanged so the
//!   downstream lexer produces a clear error, consistent with `$NAME`
//!   passthrough.

use crate::zones::{self, emit_chars, CodeStep, ScanState};
use std::collections::HashMap;

/// Apply idiom substitution across the entire source string.
///
/// Scans for `%NAME` patterns outside string/comment zones and replaces them
/// with the mapped canonical code fragment. Unknown names pass through
/// verbatim.
pub fn substitute_with_map(src: &str, map: &HashMap<String, String>) -> String {
    if map.is_empty() {
        return src.to_string();
    }

    let bytes = src.as_bytes();
    let n = bytes.len();
    let mut out = String::with_capacity(n + n / 4);
    let mut i = 0;

    // Zone state from the SHARED classifier (`zones.rs`).
    let mut in_string: Option<ScanState> = None;

    while i < n {
        // ── Inside a string: preserve verbatim until the close sequence ──
        if let Some(state) = in_string {
            let (len, next) = zones::string_step(bytes, i, state);
            emit_chars(&mut out, src, i, len);
            i += len;
            in_string = next;
            continue;
        }

        match zones::code_step(src, bytes, i) {
            // ── Line comment — pass through to end-of-line ──
            CodeStep::Comment { end } => {
                out.push_str(&src[i..end]);
                i = end;
            }
            // ── String start (string prefixes `f`/`b`/`r` are ordinary code
            //    bytes; the opening quote is what enters the zone) ──
            CodeStep::StringOpen { len, state } => {
                out.push_str(&src[i..i + len]);
                i += len;
                in_string = Some(state);
            }
            CodeStep::Code { len } => {
                let b = bytes[i];
                // ── Percent-sign sentinel: try to match an idiom name ──
                if b == b'%'
                    && i + 1 < n
                    && (bytes[i + 1].is_ascii_alphanumeric() || bytes[i + 1] == b'_')
                {
                    let name_start = i + 1;
                    let mut name_end = name_start;
                    while name_end < n
                        && (bytes[name_end].is_ascii_alphanumeric() || bytes[name_end] == b'_')
                    {
                        name_end += 1;
                    }
                    let name = &src[name_start..name_end];
                    if let Some(fragment) = map.get(name) {
                        out.push_str(fragment);
                        i = name_end;
                        continue;
                    }
                    // Unknown idiom — emit `%NAME` unchanged (downstream lexer
                    // will surface a clear error on the stray `%`, consistent
                    // with `$NAME`).
                    out.push_str(&src[i..name_end]);
                    i = name_end;
                    continue;
                }

                // ── Default passthrough (UTF-8 aware) ──
                emit_chars(&mut out, src, i, len);
                i += len;
            }
        }
    }

    out
}

// ── Test helpers ──────────────────────────────────────────────────────────

/// Small bundled test map used across unit tests.
#[cfg(test)]
pub fn test_map() -> HashMap<String, String> {
    let mut h = HashMap::new();
    // The A.1 flagship idiom in canonical form.
    h.insert(
        "HTTPCHECK".into(),
        "if not response.ok:\n    raise Exception(f\"HTTP {response.status}\")\nreturn await response.json()".into(),
    );
    // A second idiom for compose/multi-idiom tests.
    h.insert("LOG".into(), "print(f\"[debug] {__name__}\")".into());
    h
}

// ── Unit tests ────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    // ── basic expansion ──────────────────────────────────────────────────

    #[test]
    fn basic_expansion_on_own_line() {
        let m = test_map();
        let src = "%HTTPCHECK\n";
        let out = substitute_with_map(src, &m);
        assert_eq!(
            out,
            "if not response.ok:\n    raise Exception(f\"HTTP {response.status}\")\nreturn await response.json()\n"
        );
    }

    #[test]
    fn unknown_sigil_passes_through_verbatim() {
        let m = test_map();
        let src = "%ZZ\n";
        let out = substitute_with_map(src, &m);
        assert_eq!(out, "%ZZ\n");
    }

    #[test]
    fn bare_percent_passes_through() {
        // `%` not followed by an alphanumeric/_ is not an idiom sigil.
        let m = test_map();
        let src = "x = 10 % 3\n";
        let out = substitute_with_map(src, &m);
        assert_eq!(out, "x = 10 % 3\n");
    }

    #[test]
    fn percent_followed_by_space_passes_through() {
        let m = test_map();
        let src = "x % 3\n";
        assert_eq!(substitute_with_map(src, &m), "x % 3\n");
    }

    // ── protected zones: string literals ────────────────────────────────

    #[test]
    fn percent_inside_double_quoted_string_not_expanded() {
        let m = test_map();
        let src = "x = \"%HTTPCHECK\"\n";
        let out = substitute_with_map(src, &m);
        assert_eq!(out, "x = \"%HTTPCHECK\"\n");
    }

    #[test]
    fn percent_inside_single_quoted_string_not_expanded() {
        let m = test_map();
        let src = "x = '%HTTPCHECK'\n";
        let out = substitute_with_map(src, &m);
        assert_eq!(out, "x = '%HTTPCHECK'\n");
    }

    #[test]
    fn percent_inside_fstring_not_expanded() {
        let m = test_map();
        let src = "log(f\"%HTTPCHECK result: {r}\")\n";
        let out = substitute_with_map(src, &m);
        assert_eq!(out, "log(f\"%HTTPCHECK result: {r}\")\n");
    }

    #[test]
    fn percent_inside_triple_double_quoted_docstring_not_expanded() {
        let m = test_map();
        let src = "\"\"\"\n  Use %HTTPCHECK to check.\n\"\"\"\n%LOG\n";
        let out = substitute_with_map(src, &m);
        // %LOG after the docstring SHOULD expand; %HTTPCHECK inside must NOT.
        assert!(out.starts_with("\"\"\"\n  Use %HTTPCHECK to check.\n\"\"\"\n"));
        assert!(out.ends_with("print(f\"[debug] {__name__}\")\n"));
    }

    #[test]
    fn percent_inside_triple_single_quoted_docstring_not_expanded() {
        let m = test_map();
        let src = "'''\n  %HTTPCHECK here\n'''\n%LOG\n";
        let out = substitute_with_map(src, &m);
        assert!(out.starts_with("'''\n  %HTTPCHECK here\n'''\n"));
        assert!(out.ends_with("print(f\"[debug] {__name__}\")\n"));
    }

    // ── protected zones: comments ────────────────────────────────────────

    #[test]
    fn percent_inside_comment_not_expanded() {
        let m = test_map();
        let src = "x = 1  # use %HTTPCHECK for http calls\n%LOG\n";
        let out = substitute_with_map(src, &m);
        assert!(out.contains("# use %HTTPCHECK for http calls"));
        assert!(out.ends_with("print(f\"[debug] {__name__}\")\n"));
    }

    // ── empty config ──────────────────────────────────────────────────────

    #[test]
    fn empty_map_returns_source_unchanged() {
        let m = HashMap::new();
        let src = "%HTTPCHECK\n%ZZ\n";
        let out = substitute_with_map(src, &m);
        assert_eq!(out, src);
    }

    // ── UTF-8 / CRLF / edge cases ─────────────────────────────────────────

    #[test]
    fn multibyte_utf8_neighbours_preserved() {
        let m = test_map();
        // Multi-byte char near the sigil must survive byte-exact.
        let src = "# ← %HTTPCHECK → end\n";
        let out = substitute_with_map(src, &m);
        // The whole line is a comment — nothing should expand.
        assert_eq!(out, src);
    }

    #[test]
    fn multibyte_utf8_outside_comment_preserved() {
        let mut m = HashMap::new();
        m.insert("X".into(), "pass".into());
        let src = "result = \"← arrow\"\n%X\n";
        let out = substitute_with_map(src, &m);
        assert_eq!(out, "result = \"← arrow\"\npass\n");
    }

    #[test]
    fn crlf_line_endings_preserved() {
        let m = test_map();
        let src = "%LOG\r\n";
        let out = substitute_with_map(src, &m);
        // The expansion itself has no newline; the CRLF from the source
        // remains after the expanded fragment.
        assert_eq!(out, "print(f\"[debug] {__name__}\")\r\n");
    }

    #[test]
    fn final_line_without_newline_preserved() {
        let m = test_map();
        let src = "%LOG";
        let out = substitute_with_map(src, &m);
        assert_eq!(out, "print(f\"[debug] {__name__}\")");
    }

    #[test]
    fn idempotent_under_double_expansion() {
        let m = test_map();
        let src = "%LOG\n";
        let first = substitute_with_map(src, &m);
        let second = substitute_with_map(&first, &m);
        // The expanded fragment contains no `%NAME` patterns → stable.
        assert_eq!(first, second);
    }

    // ── composition with $NAME and kwarg/hook passes ──────────────────────
    // (Full end-to-end is tested in lib.rs via expand_with_config; this is
    // a targeted unit test of the idiom + dict interaction in isolation.)

    #[test]
    fn idiom_value_can_contain_dollar_alias_passthrough() {
        // If an idiom fragment contains a `$NAME`, after idiom expansion
        // that `$NAME` is visible to the downstream strings pass.
        // Here we verify the idiom expander itself doesn't corrupt it.
        let mut m = HashMap::new();
        m.insert("STYLED".into(), "color = $c1".into());
        let src = "%STYLED\n";
        let out = substitute_with_map(src, &m);
        assert_eq!(out, "color = $c1\n");
    }
}
