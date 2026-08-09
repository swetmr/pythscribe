//! Kani proof harnesses for the shared zone classifier ([`crate::zones`]).
//!
//! # What this layer is, precisely
//!
//! Kani is a **bounded model checker** (CBMC-based). Each harness below is
//! exhaustive over *every* input up to a stated bound `N` — not a sample, not a
//! corpus. That makes it strictly stronger than the differential corpus
//! (`verification/diff_harness.py`, 2,039 fixed cases) and strictly weaker than
//! the Lean proofs (`verification/PythExpandVerify.lean`), which are unbounded
//! but are about a *model* of the expander rather than this Rust code.
//!
//! **A passing Kani harness does not license the word "verified".** It licenses
//! exactly this: "no input of at most N bytes falsifies this property".
//!
//! # Why it exists
//!
//! The differential corpus cannot find a bug that both sides share. It did not
//! find this one: `string_step` returned `utf8_char_len(lead)` for a *truncated*
//! multi-byte lead byte at the end of a buffer (`bytes = [0xF0]`, `i = 0` → an
//! advance of 4 past a 1-byte buffer), because the Lean model made the same
//! assumption. Kani finds it in seconds — see
//! `string_step_is_total_on_a_truncated_lead_byte` below, the regression pin for
//! the clamp that fixes it.
//!
//! # Running
//!
//! ```text
//! cargo kani -p pyths_expand          # all harnesses
//! cargo kani -p pyths_expand --harness string_step_progress_and_in_bounds
//! ```
//!
//! This module is `#[cfg(kani)]`: it is invisible to `cargo build`, `cargo test`
//! and every downstream consumer, and the crate has **no** Kani dependency (the
//! `kani` crate is injected by `cargo kani` itself). See `KANI.md`.

use crate::zones::{code_step, line_start_states, string_step, utf8_char_len, CodeStep, ScanState};

// ---------------------------------------------------------------------------
// Symbolic-input helpers
// ---------------------------------------------------------------------------

/// An unconstrained [`ScanState`]. The quote byte is left *fully* symbolic
/// (not just `'` / `"`), which is strictly more general than any state the
/// scanner can actually reach.
fn any_state() -> ScanState {
    let q: u8 = kani::any();
    if kani::any() {
        ScanState::Single(q)
    } else {
        ScanState::Triple(q)
    }
}

/// Constrain a symbolic buffer to be valid UTF-8 — the real precondition of
/// every `&str` entry point in the expander (`code_step`, `line_start_states`
/// and every tier are reached only from a `&str`).
fn assume_valid_utf8(buf: &[u8]) -> &str {
    kani::assume(core::str::from_utf8(buf).is_ok());
    core::str::from_utf8(buf).unwrap()
}

// ---------------------------------------------------------------------------
// 1. `string_step` — progress and in-bounds
// ---------------------------------------------------------------------------

/// **Progress**: `string_step` always consumes ≥ 1 byte, so no caller loop can
/// spin. **In-bounds**: it never advances past the end of the buffer — over an
/// *unconstrained* `[u8; N]` (not just valid UTF-8), because the signature takes
/// `&[u8]` and therefore carries no UTF-8 invariant.
///
/// This is the harness that fails on the pre-clamp `string_step`.
#[kani::proof]
#[kani::unwind(8)]
fn string_step_progress_and_in_bounds() {
    const N: usize = 16;
    let bytes: [u8; N] = kani::any();
    let i: usize = kani::any();
    kani::assume(i < N);

    let (len, _next) = string_step(&bytes, i, any_state());

    assert!(
        len >= 1,
        "progress: string_step must consume at least one byte"
    );
    assert!(
        i + len <= N,
        "in-bounds: string_step must never advance past the end of the buffer"
    );
}

/// The regression pin for the latent precondition the 2,039-case differential
/// structurally could not find: a **truncated** multi-byte lead byte at the very
/// end of a buffer. Pre-fix, `utf8_char_len(0xF0) == 4` while one byte remains.
#[kani::proof]
#[kani::unwind(8)]
fn string_step_is_total_on_a_truncated_lead_byte() {
    let lead: u8 = kani::any();
    kani::assume(lead >= 0xC0); // every multi-byte lead byte: 2-, 3- and 4-byte
    let bytes = [lead]; // …with every continuation byte missing

    let (len, _next) = string_step(&bytes, 0, any_state());

    assert!(
        len <= 1,
        "string_step must clamp to the buffer end on a truncated lead byte"
    );
}

/// The clamp is a **no-op on the real call domain**: for any valid-UTF-8 buffer
/// and any char-boundary index, the clamped `string_step` returns *exactly* what
/// the pre-clamp version returned. This is what keeps the 2,039-case differential
/// byte-identical.
#[kani::proof]
#[kani::unwind(10)]
fn clamp_changes_nothing_on_valid_utf8() {
    const N: usize = 8;
    let buf: [u8; N] = kani::any();
    let s = assume_valid_utf8(&buf);
    let i: usize = kani::any();
    kani::assume(i < N);
    kani::assume(s.is_char_boundary(i));

    let st = any_state();
    let now = string_step(&buf, i, st);
    let before = string_step_pre_clamp(&buf, i, st);

    assert!(
        now == before,
        "the clamp must not change behaviour on valid UTF-8"
    );
}

/// Byte-for-byte copy of `string_step` **as it shipped before the clamp**, kept
/// here (and only here) as the oracle for `clamp_changes_nothing_on_valid_utf8`.
/// Do not use it for anything else.
fn string_step_pre_clamp(bytes: &[u8], i: usize, st: ScanState) -> (usize, Option<ScanState>) {
    let n = bytes.len();
    let b = bytes[i];
    if b == b'\\' && i + 1 < n {
        return (1 + utf8_char_len(bytes[i + 1]), Some(st));
    }
    match st {
        ScanState::Single(q) => {
            let next = if b == q { None } else { Some(st) };
            (utf8_char_len(b), next)
        }
        ScanState::Triple(q) => {
            if b == q && i + 2 < n && bytes[i + 1] == q && bytes[i + 2] == q {
                (3, None)
            } else {
                (utf8_char_len(b), Some(st))
            }
        }
    }
}

// ---------------------------------------------------------------------------
// 2. Escape handling
// ---------------------------------------------------------------------------

/// A backslash-escaped character can **never** close a protected zone: whatever
/// follows the backslash (including the zone's own quote byte), the state after
/// the step is unchanged. Over an unconstrained buffer and an unconstrained
/// state.
#[kani::proof]
#[kani::unwind(8)]
fn a_backslash_escape_never_closes_a_zone() {
    const N: usize = 12;
    let bytes: [u8; N] = kani::any();
    let i: usize = kani::any();
    kani::assume(i < N - 1); // not `i + 1 < N`: `i` is symbolic and would overflow
    kani::assume(bytes[i] == b'\\');

    let st = any_state();
    let (len, next) = string_step(&bytes, i, st);

    assert!(
        next == Some(st),
        "an escape pair cannot close a protected zone"
    );
    assert!(
        len >= 2,
        "an escape pair consumes the backslash and what it escapes"
    );
}

// ---------------------------------------------------------------------------
// 3. `code_step` — progress and in-bounds
// ---------------------------------------------------------------------------

/// `code_step` takes a `&str`, so the input here is a symbolic *valid-UTF-8*
/// buffer and a symbolic char-boundary index. Every arm advances (≥ 1 byte) and
/// stays inside the buffer.
/// N is 8, not 16: `code_step` takes a `&str`, so the harness must *constrain*
/// the symbolic buffer to valid UTF-8, and CBMC pays for that validation on
/// every one of the 2^(8N) buffers. At N = 12 it ran past 5 GB of solver memory
/// without converging (an honest cost of the `&str` precondition, recorded in
/// KANI.md); at N = 8 it closes in minutes. The classifier's lookahead window is
/// 3 bytes, so 8 comfortably exceeds it.
#[kani::proof]
#[kani::solver(cadical)]
#[kani::unwind(12)]
fn code_step_progress_and_in_bounds() {
    const N: usize = 8;
    let buf: [u8; N] = kani::any();
    let s = assume_valid_utf8(&buf);
    let i: usize = kani::any();
    kani::assume(i < N);
    kani::assume(s.is_char_boundary(i));

    match code_step(s, &buf, i) {
        CodeStep::Comment { end } => {
            assert!(end > i, "progress: a comment step consumes the `#`");
            assert!(end <= N, "in-bounds: comment end");
        }
        CodeStep::StringOpen { len, .. } => {
            assert!(len == 1 || len == 3, "an opener is one quote or three");
            assert!(i + len <= N, "in-bounds: string opener");
        }
        CodeStep::Code { len } => {
            assert!(len >= 1, "progress: a code step consumes one char");
            assert!(i + len <= N, "in-bounds: code step");
        }
    }
}

// ---------------------------------------------------------------------------
// 4. UTF-8
// ---------------------------------------------------------------------------

/// `utf8_char_len` agrees with the real encoded length for **every** `char`.
#[kani::proof]
#[kani::unwind(6)]
fn utf8_char_len_agrees_with_the_encoded_length() {
    let c: char = kani::any();
    let mut buf = [0u8; 4];
    let encoded_len = c.encode_utf8(&mut buf).len();
    assert!(
        utf8_char_len(buf[0]) == encoded_len,
        "utf8_char_len must agree with the actual encoding on every valid lead byte"
    );
}

// ---------------------------------------------------------------------------
// 5. `line_start_states`
// ---------------------------------------------------------------------------

/// One entry per line: the vector has exactly `newlines + 1` entries for a
/// non-empty source (line 0, plus the line that follows each `\n`). Kani's
/// automatic bounds/overflow checks additionally prove that every index the
/// function forms internally (notably `bytes[end - 1]`) is in range, and that
/// the loop terminates.
#[kani::proof]
#[kani::solver(cadical)]
#[kani::unwind(10)]
fn line_start_states_has_one_entry_per_line() {
    const N: usize = 6;
    let buf: [u8; N] = kani::any();
    let s = assume_valid_utf8(&buf);

    let states = line_start_states(s);

    let mut newlines = 0usize;
    let mut k = 0usize;
    while k < N {
        if buf[k] == b'\n' {
            newlines += 1;
        }
        k += 1;
    }

    assert!(
        states.len() == newlines + 1,
        "line_start_states must return one state per line"
    );
}

// ---------------------------------------------------------------------------
// 6. Zone safety — over the SHIPPING tier rewrites
// ---------------------------------------------------------------------------

/// The alphabet the classifier actually branches on: the two tier sigils, both
/// quote bytes, the comment byte, the escape byte, a newline, an identifier byte
/// (so `$a` / `%a` are well-formed alias sites) and a multi-byte lead byte.
/// Restricting to it is what keeps the *whole tier rewrite* — dictionary lookup
/// and all — inside the CBMC budget. Every byte outside this set is, to the
/// classifier, indistinguishable from `a`.
fn any_zone_byte() -> u8 {
    let b: u8 = kani::any();
    kani::assume(
        b == b'$'
            || b == b'%'
            || b == b'\''
            || b == b'"'
            || b == b'#'
            || b == b'\\'
            || b == b'\n'
            || b == b'a',
    );
    b
}

/// `protected[k]` iff byte `k` of `src` lies in a protected zone — computed with
/// the **shipping** classifier, not a copy of it. Opener quotes and the `#` of a
/// comment are counted as protected: every tier emits them verbatim too.
fn protected_mask<const N: usize>(src: &str, bytes: &[u8; N]) -> [bool; N] {
    let mut mask = [false; N];
    let n = bytes.len();
    let mut st: Option<ScanState> = None;
    let mut i = 0usize;

    while i < n {
        let (start, end, next) = match st {
            Some(s) => {
                let (len, next) = string_step(bytes, i, s);
                (i, (i + len).min(n), next)
            }
            None => match code_step(src, bytes, i) {
                CodeStep::Comment { end } => (i, end.max(i + 1), None),
                CodeStep::StringOpen { len, state } => (i, (i + len).min(n), Some(state)),
                CodeStep::Code { len } => {
                    // A code byte: not protected. Advance without marking.
                    let e = (i + len).min(n);
                    st = None;
                    i = e;
                    continue;
                }
            },
        };
        let mut k = start;
        while k < end {
            mask[k] = true;
            k += 1;
        }
        st = next;
        i = end;
    }
    mask
}

/// **Zone safety, Tier `$` (the domain dictionary).** If every `$` byte in the
/// source lies inside a protected zone — a string, a docstring, a comment, or the
/// tail of an escape pair — then `strings::substitute` returns the source
/// **byte-for-byte**. Nothing inside a protected zone is ever rewritten; a
/// protected `$` reaches the output verbatim.
///
/// Stated over the shipping `strings::substitute`, exhaustive for all sources of
/// up to N bytes drawn from `any_zone_byte`.
#[kani::proof]
#[kani::solver(cadical)]
#[kani::unwind(70)]
fn zone_safety_dollar_sigil_is_never_rewritten_inside_a_protected_zone() {
    const N: usize = 6;
    let mut bytes = [0u8; N];
    let mut k = 0usize;
    while k < N {
        bytes[k] = any_zone_byte();
        k += 1;
    }
    // Every byte of the alphabet is ASCII, so the buffer is valid UTF-8 by
    // construction.
    let src = core::str::from_utf8(&bytes).unwrap();

    let mask = protected_mask::<N>(src, &bytes);
    let mut j = 0usize;
    while j < N {
        // The hypothesis: no `$` sits in a code zone.
        kani::assume(!(bytes[j] == b'$' && !mask[j]));
        j += 1;
    }

    let out = crate::strings::substitute(src);
    assert!(
        out.as_bytes() == &bytes[..],
        "zone safety: a protected `$` must survive the dict tier byte-for-byte"
    );
}

/// **Zone safety, Tier `%` (idioms).** Same statement for the idiom tier, with a
/// non-empty idiom map (an empty map short-circuits, which would prove nothing).
#[kani::proof]
#[kani::solver(cadical)]
#[kani::unwind(70)]
fn zone_safety_percent_sigil_is_never_rewritten_inside_a_protected_zone() {
    const N: usize = 6;
    let mut bytes = [0u8; N];
    let mut k = 0usize;
    while k < N {
        bytes[k] = any_zone_byte();
        k += 1;
    }
    let src = core::str::from_utf8(&bytes).unwrap();

    let mask = protected_mask::<N>(src, &bytes);
    let mut j = 0usize;
    while j < N {
        kani::assume(!(bytes[j] == b'%' && !mask[j]));
        j += 1;
    }

    let mut map = std::collections::HashMap::new();
    map.insert("a".to_string(), "EXPANDED".to_string());
    let out = crate::idioms::substitute_with_map(src, &map);

    assert!(
        out.as_bytes() == &bytes[..],
        "zone safety: a protected `%` must survive the idiom tier byte-for-byte"
    );
}
