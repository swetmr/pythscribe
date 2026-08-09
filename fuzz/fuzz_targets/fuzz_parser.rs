#![no_main]
//! `cargo fuzz` target for the parser.
//!
//! Run with: `cargo +nightly fuzz run fuzz_parser -- -max_len=4096`
//!
//! Success criterion: no panics. The parser returns `Result<Module,
//! Vec<ParseError>>` — error cases are part of normal operation.
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    if let Ok(s) = std::str::from_utf8(data) {
        let _ = pyths_parser::parse(s);
    }
});
