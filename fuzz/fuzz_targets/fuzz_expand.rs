#![no_main]
//! `cargo fuzz` target for the `.psc` expander.
//!
//! Run with: `cargo +nightly fuzz run fuzz_expand -- -max_len=4096`
//!
//! Success criterion: no panics on any input. The expander is a pure
//! source-to-source pre-pass; it must accept arbitrary bytes (or fail
//! gracefully) without aborting.
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    if let Ok(s) = std::str::from_utf8(data) {
        let _ = pyths_expand::expand(s);
    }
});
