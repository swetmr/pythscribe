//! The **one** zone classifier every tier shares.
//!
//! A `.psc` source is partitioned into *code* and *protected* zones.
//! Protected = inside a `'…'` / `"…"` string, inside a `'''…'''` /
//! `"""…"""` triple-quoted string (docstring), or after a `#` up to the
//! end of the line. Every tier rewrites **only** in code zones and emits
//! protected zones byte-for-byte.
//!
//! Before this module existed, the state machine was copy-pasted into
//! `strings.rs`, `kwargs.rs`, `hooks.rs` and `idioms.rs`, and Tier A
//! (`lib.rs::expand_line`) had **no** zone state at all — which is how a
//! preset marker alone on a line inside a docstring came to be rewritten.
//! There is now a single definition; the tiers differ only in what they do
//! in a *code* zone.
//!
//! The Lean model (`verification/PythExpandVerify.lean`, `expandDictChars`
//! and its companion classifier `protChars`) mirrors these transitions
//! exactly; `zone_safety_chars`, `kwarg_zone_safety_chars`,
//! `hook_zone_safety_chars`, `idiom_zone_safety_chars` and
//! `tierA_zone_safety_chars` are all proved against that one classifier,
//! and the model-vs-implementation differential (`diff_harness.py`) binds
//! it to this code. Change the transitions here and every one of those
//! must move with it.

/// The protected-zone states. Code is represented as `None`.
///
/// A `#` comment is not a state: it always ends at the next `\n`, so it is
/// consumed in one step by [`code_step`] and can never be live at a line
/// boundary.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum ScanState {
    /// Inside `'…'` or `"…"` — the byte closes it.
    Single(u8),
    /// Inside `'''…'''` or `"""…"""` — three of the byte close it.
    Triple(u8),
}

/// What the classifier finds at `i` while in a **code** zone.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum CodeStep {
    /// A `#` comment: bytes `i..end` are protected (`end` excludes the
    /// terminating `\n`, which is consumed back in code state).
    Comment { end: usize },
    /// A string opener of `len` bytes (1 = single-quote, 3 = triple-quote)
    /// that puts the classifier into `state`.
    StringOpen { len: usize, state: ScanState },
    /// Not a zone boundary — `len` bytes of ordinary code (one UTF-8 char).
    Code { len: usize },
}

/// One step of the classifier **inside a protected string zone**.
///
/// Returns the number of bytes consumed and the state that follows them. A
/// backslash escapes the next character in both single and triple zones, so
/// `\"` does not close a `"…"`.
///
/// # Postcondition (total, unconditional)
///
/// `1 <= len` and `i + len <= bytes.len()` for **every** `&[u8]` and every
/// `i < bytes.len()` — no UTF-8 precondition. The `.min(n - i)` clamp is what
/// makes that unconditional: `utf8_char_len` reports the length a lead byte
/// *claims*, which for a **truncated** trailing character (e.g. `bytes =
/// [0xF0]`, `i = 0` → claims 4, has 1) overruns the buffer. Real callers pass
/// `&str` bytes, where the clamp is provably a no-op
/// (`kani_proofs::clamp_changes_nothing_on_valid_utf8`), so this costs nothing
/// and the differential corpus stays byte-identical — but `&[u8]` does not
/// carry the UTF-8 invariant, and a function whose postcondition depends on an
/// invariant its signature cannot express is a trap for the next caller.
/// Progress + in-bounds are model-checked over unconstrained buffers by
/// `kani_proofs::string_step_progress_and_in_bounds`.
#[must_use]
pub fn string_step(bytes: &[u8], i: usize, st: ScanState) -> (usize, Option<ScanState>) {
    let n = bytes.len();
    let b = bytes[i];
    // Bytes still available at `i` — the ceiling on any advance.
    let avail = n - i;

    // Escape — the backslash and the character it escapes are consumed as a
    // unit and cannot close the zone.
    if b == b'\\' && i + 1 < n {
        return ((1 + utf8_char_len(bytes[i + 1])).min(avail), Some(st));
    }

    match st {
        ScanState::Single(q) => {
            let next = if b == q { None } else { Some(st) };
            (utf8_char_len(b).min(avail), next)
        }
        ScanState::Triple(q) => {
            if b == q && i + 2 < n && bytes[i + 1] == q && bytes[i + 2] == q {
                (3, None)
            } else {
                (utf8_char_len(b).min(avail), Some(st))
            }
        }
    }
}

/// One step of the classifier **inside a code zone**: does the byte at `i`
/// open a protected zone?
///
/// `src` and `bytes` must be the same buffer (`bytes = src.as_bytes()`).
#[must_use]
pub fn code_step(src: &str, bytes: &[u8], i: usize) -> CodeStep {
    let n = bytes.len();
    let b = bytes[i];

    if b == b'#' {
        let end = src[i..].find('\n').map(|x| i + x).unwrap_or(n);
        return CodeStep::Comment { end };
    }

    if b == b'\'' || b == b'"' {
        if i + 2 < n && bytes[i + 1] == b && bytes[i + 2] == b {
            return CodeStep::StringOpen {
                len: 3,
                state: ScanState::Triple(b),
            };
        }
        return CodeStep::StringOpen {
            len: 1,
            state: ScanState::Single(b),
        };
    }

    CodeStep::Code {
        len: utf8_char_len(b),
    }
}

/// The zone state at the start of every line of `src`.
///
/// Lines are the pieces `lib.rs::split_lines` produces (split after each
/// `\n`, terminator kept). Entry `k` is the classifier state at the first
/// byte of line `k`: `None` means the line begins in a code zone,
/// `Some(_)` means it begins inside an unterminated string or docstring.
///
/// This is what makes **Tier A** zone-aware: a per-line rewrite is legal
/// exactly when the line starts in code. A `#` comment never survives a
/// newline, so a comment state can never appear here.
///
/// The returned vector may carry one extra trailing entry (the state after
/// a final `\n`, for which there is no line); callers index defensively.
#[must_use]
pub fn line_start_states(src: &str) -> Vec<Option<ScanState>> {
    let bytes = src.as_bytes();
    let n = bytes.len();
    let mut states: Vec<Option<ScanState>> = Vec::new();
    if n == 0 {
        return states;
    }
    // Line 0 always starts in code.
    states.push(None);

    let mut st: Option<ScanState> = None;
    let mut i = 0;
    while i < n {
        let (len, next) = match st {
            Some(s) => string_step(bytes, i, s),
            None => match code_step(src, bytes, i) {
                CodeStep::Comment { end } => (end.max(i + 1) - i, None),
                CodeStep::StringOpen { len, state } => (len, Some(state)),
                CodeStep::Code { len } => (len, None),
            },
        };
        let end = (i + len).min(n);
        // A `\n` can only ever be the LAST byte of a step: a comment step
        // stops before it, a triple-quote closer is three quotes, and an
        // escape step is `\` + one character. So checking the final byte is
        // exhaustive.
        if bytes[end - 1] == b'\n' {
            states.push(next);
        }
        st = next;
        i = end;
    }

    states
}

/// Byte-length of a UTF-8 character given its leading byte. A stray
/// continuation byte counts as 1 so that malformed input still advances.
#[must_use]
// clippy: the first two branches share the length 1 but are DISTINCT byte
// classes — ASCII (`b < 0x80`) vs a stray UTF-8 continuation byte
// (`0x80..=0xBF`, counted as 1 so malformed input still advances). Kept
// separate deliberately to mirror the byte-class table; not a copy-paste bug.
#[allow(clippy::if_same_then_else)]
pub fn utf8_char_len(b: u8) -> usize {
    if b < 0x80 {
        1
    } else if b < 0xC0 {
        1
    } else if b < 0xE0 {
        2
    } else if b < 0xF0 {
        3
    } else {
        4
    }
}

/// Copy `len` bytes of `source` starting at `i` into `out`, clamped to the
/// end of the buffer.
pub fn emit_chars(out: &mut String, source: &str, i: usize, len: usize) {
    let end = (i + len).min(source.len());
    out.push_str(&source[i..end]);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn states(src: &str) -> Vec<Option<ScanState>> {
        line_start_states(src)
    }

    #[test]
    fn empty_source_has_no_lines() {
        assert!(states("").is_empty());
    }

    #[test]
    fn plain_code_lines_all_start_in_code() {
        let s = states("x = 1\ny = 2\n");
        assert_eq!(s[0], None);
        assert_eq!(s[1], None);
    }

    #[test]
    fn lines_inside_triple_double_quote_are_protected() {
        // line 0: code | line 1: inside """ | line 2: inside """ (the closer
        // line itself starts protected) | line 3: code again
        let s = states("doc = \"\"\"\nR*\n\"\"\"\ny = 1\n");
        assert_eq!(s[0], None);
        assert_eq!(s[1], Some(ScanState::Triple(b'"')));
        assert_eq!(s[2], Some(ScanState::Triple(b'"')));
        assert_eq!(s[3], None);
    }

    #[test]
    fn lines_inside_triple_single_quote_are_protected() {
        let s = states("d = '''\n@c\n'''\nz = 2\n");
        assert_eq!(s[0], None);
        assert_eq!(s[1], Some(ScanState::Triple(b'\'')));
        assert_eq!(s[3], None);
    }

    #[test]
    fn comment_never_survives_the_newline() {
        let s = states("# R*\nR*\n");
        assert_eq!(s[0], None);
        assert_eq!(s[1], None);
    }

    #[test]
    fn closed_string_on_a_line_leaves_the_next_line_in_code() {
        let s = states("s = \"R*\"\nR*\n");
        assert_eq!(s[1], None);
    }

    #[test]
    fn escaped_quote_does_not_close_the_zone() {
        // The `\"` must not close the string, so line 1 is still protected.
        let s = states("s = \"a\\\"b\nR*\n");
        assert_eq!(s[1], Some(ScanState::Single(b'"')));
    }

    #[test]
    fn adjacent_triple_quotes_close_and_reopen() {
        let s = states("a = \"\"\"x\"\"\" + \"\"\"y\nR*\n");
        assert_eq!(s[1], Some(ScanState::Triple(b'"')));
    }

    #[test]
    fn multibyte_chars_do_not_desync_line_indices() {
        let s = states("s = \"← ✓ 🎉\"\nR*\n");
        assert_eq!(s[0], None);
        assert_eq!(s[1], None);
    }

    #[test]
    fn crlf_line_endings_are_tracked() {
        let s = states("doc = \"\"\"\r\nR*\r\n\"\"\"\r\n");
        assert_eq!(s[1], Some(ScanState::Triple(b'"')));
    }
}
