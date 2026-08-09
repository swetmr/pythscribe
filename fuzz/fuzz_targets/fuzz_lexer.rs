#![no_main]
//! `cargo fuzz` target for the lexer.
//!
//! Run with: `cargo +nightly fuzz run fuzz_lexer -- -max_len=4096`
//!
//! Success criterion: no panics. The lexer is recovering by design; it
//! returns a `LexResult` with diagnostics rather than aborting. Any
//! panic discovered here is a bug to fix.
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    if let Ok(s) = std::str::from_utf8(data) {
        let _ = pyths_lexer::lex_recovering(s);
    }
});
