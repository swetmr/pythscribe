#![no_main]
//! `cargo fuzz` target for the type checker.
//!
//! Run with: `cargo +nightly fuzz run fuzz_check -- -max_len=4096`
//!
//! Success criterion: no panics. Only runs the checker on inputs that
//! pass the parser — otherwise the harness exercises lexer/parser
//! paths that are covered by the other targets. Type errors are
//! expected outputs, not failures.
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    if let Ok(s) = std::str::from_utf8(data) {
        if let Ok(module) = pyths_parser::parse(s) {
            let _ = pyths_types::check(&module);
        }
    }
});
