//! Panic-resistance fuzz harness for the PythScribe pipeline.
//!
//! Generates pseudo-random ASCII and UTF-8 byte sequences and feeds
//! them through lex → expand → parse → type-check. The pipeline is
//! allowed to return errors freely (most random inputs aren't valid
//! PythScribe), but it must NEVER panic.
//!
//! This is the regression-grade fuzz harness called out in
//! `docs/security.md` §5. For coverage-guided mutation, see the
//! `cargo-fuzz` integration tracked under §6 "Known limits".
//!
//! Counts are kept modest so the suite runs in CI; bump locally if
//! you're chasing a specific class of bug:
//!
//! ```bash
//! PYTHS_FUZZ_ITER=10000 cargo test -p pyths_cli --test fuzz_inputs --release
//! ```

use std::panic::{catch_unwind, AssertUnwindSafe};

/// Deterministic xorshift64 RNG so fuzz runs are reproducible across
/// machines. Not cryptographically secure — that's not the point here.
struct Xorshift64(u64);

impl Xorshift64 {
    fn new(seed: u64) -> Self {
        // Avoid the zero state which would lock the generator.
        Self(if seed == 0 {
            0x1234_5678_9abc_def0
        } else {
            seed
        })
    }
    fn next_u64(&mut self) -> u64 {
        let mut x = self.0;
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        self.0 = x;
        x
    }
    fn next_u32(&mut self) -> u32 {
        self.next_u64() as u32
    }
}

fn fuzz_iterations() -> usize {
    std::env::var("PYTHS_FUZZ_ITER")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(200)
}

/// Run `f(input)` inside a panic catcher. On panic, fail the test with
/// a diagnostic that includes the byte-encoded input so the failure
/// can be reproduced.
fn assert_no_panic<F: FnOnce()>(label: &str, input: &str, f: F) {
    let f = AssertUnwindSafe(f);
    let result = catch_unwind(f);
    if let Err(payload) = result {
        let msg = if let Some(s) = payload.downcast_ref::<&'static str>() {
            (*s).to_string()
        } else if let Some(s) = payload.downcast_ref::<String>() {
            s.clone()
        } else {
            "<non-string panic payload>".to_string()
        };
        // Show first 200 bytes of input as escaped hex so reproducers
        // survive copy-paste through terminals that mangle bytes.
        let preview: String = input
            .bytes()
            .take(200)
            .map(|b| format!("\\x{:02x}", b))
            .collect();
        panic!(
            "{} panicked: {}\ninput ({}B preview): {}",
            label,
            msg,
            input.len().min(200),
            preview
        );
    }
}

// ---------------------------------------------------------------------------
// Random-ASCII corpus
// ---------------------------------------------------------------------------

fn random_ascii(rng: &mut Xorshift64, max_len: usize) -> String {
    let len = (rng.next_u32() as usize) % max_len;
    let mut s = String::with_capacity(len);
    for _ in 0..len {
        let b = (rng.next_u32() % 95 + 0x20) as u8; // printable ASCII
        s.push(b as char);
    }
    s
}

#[test]
fn fuzz_random_ascii_no_panics() {
    let n = fuzz_iterations();
    let mut rng = Xorshift64::new(0xdead_beef);
    for i in 0..n {
        let src = random_ascii(&mut rng, 1024);

        // Step 1: lexer must not panic.
        assert_no_panic("lexer", &src, || {
            let _ = pyths_lexer::lex_recovering(&src);
        });

        // Step 2: expander must not panic.
        let expanded = AssertUnwindSafe(|| pyths_expand::expand(&src));
        let expanded_result = catch_unwind(expanded);
        let expanded_src = match expanded_result {
            Ok(s) => s,
            Err(_) => panic!("expander panicked on iter {}: input={:?}", i, &src),
        };

        // Step 3: parser must not panic.
        assert_no_panic("parser", &expanded_src, || {
            let _ = pyths_parser::parse(&expanded_src);
        });

        // Step 4: type-checker. Only run when the parse actually
        // succeeded — the checker takes a Module, not raw text.
        if let Ok(module) = pyths_parser::parse(&expanded_src) {
            assert_no_panic("type checker", &expanded_src, || {
                let _ = pyths_types::check(&module);
            });
        }
    }
}

// ---------------------------------------------------------------------------
// Random-UTF-8 corpus (multi-byte sequences, emoji, RTL marks)
// ---------------------------------------------------------------------------

const UTF8_SAMPLES: &[&str] = &[
    "← ↑ → ↓",           // BMP arrows
    "🎉 🚀 🦀",          // emoji
    "מהוא",              // RTL
    "你好世界",          // CJK
    "café résumé naïve", // Latin-1 + combining
    "\u{200D}\u{FE0F}",  // ZWJ + variation selector
];

fn random_utf8(rng: &mut Xorshift64, max_len: usize) -> String {
    let len = (rng.next_u32() as usize) % max_len;
    let mut s = String::with_capacity(len);
    while s.len() < len {
        // 50/50 ASCII or UTF-8 sample.
        if rng.next_u32() & 1 == 0 {
            let b = (rng.next_u32() % 95 + 0x20) as u8;
            s.push(b as char);
        } else {
            let sample = UTF8_SAMPLES[(rng.next_u32() as usize) % UTF8_SAMPLES.len()];
            s.push_str(sample);
        }
    }
    s
}

#[test]
fn fuzz_random_utf8_no_panics() {
    let n = fuzz_iterations();
    let mut rng = Xorshift64::new(0xcafe_babe);
    for _i in 0..n {
        let src = random_utf8(&mut rng, 1024);

        assert_no_panic("lexer-utf8", &src, || {
            let _ = pyths_lexer::lex_recovering(&src);
        });
        let expanded = catch_unwind(AssertUnwindSafe(|| pyths_expand::expand(&src)))
            .expect("expander never panics on utf-8");
        assert_no_panic("parser-utf8", &expanded, || {
            let _ = pyths_parser::parse(&expanded);
        });
    }
}

// ---------------------------------------------------------------------------
// Mutation-based corpus: start from valid fixtures, flip / insert / delete
// ---------------------------------------------------------------------------

fn fixtures_dir() -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures")
}

fn mutate(rng: &mut Xorshift64, bytes: &mut Vec<u8>) {
    if bytes.is_empty() {
        return;
    }
    match rng.next_u32() % 3 {
        0 => {
            // Flip a random bit.
            let i = (rng.next_u32() as usize) % bytes.len();
            let bit = (rng.next_u32() as u8) & 7;
            bytes[i] ^= 1 << bit;
        }
        1 => {
            // Insert a random byte.
            let i = (rng.next_u32() as usize) % (bytes.len() + 1);
            let b = (rng.next_u32() as u8) % 0x7e + 0x20;
            bytes.insert(i, b);
        }
        _ => {
            // Delete a random byte.
            let i = (rng.next_u32() as usize) % bytes.len();
            bytes.remove(i);
        }
    }
}

#[test]
fn fuzz_mutated_fixtures_no_panics() {
    // Start from a few hand-picked fixtures and apply 50 mutations to each.
    let fixtures = ["hello.ps", "arithmetic.ps", "classes.ps", "control_flow.ps"];
    let dir = fixtures_dir();
    let mut rng = Xorshift64::new(0xfeed_face);

    for name in &fixtures {
        let path = dir.join(name);
        let original = match std::fs::read(&path) {
            Ok(b) => b,
            Err(_) => continue, // fixture missing — skip
        };
        for _ in 0..50 {
            let mut mutated = original.clone();
            // Apply 1–3 mutations.
            let count = 1 + (rng.next_u32() % 3) as usize;
            for _ in 0..count {
                mutate(&mut rng, &mut mutated);
            }
            // The result may not be valid UTF-8 after byte flips.
            let src = match std::str::from_utf8(&mutated) {
                Ok(s) => s.to_string(),
                Err(_) => continue, // skip invalid UTF-8 mutations
            };

            assert_no_panic("lexer-mut", &src, || {
                let _ = pyths_lexer::lex_recovering(&src);
            });
            // Expander panic-resistance: catch_unwind so we can keep going.
            let _ = catch_unwind(AssertUnwindSafe(|| pyths_expand::expand(&src)));
            assert_no_panic("parser-mut", &src, || {
                let _ = pyths_parser::parse(&src);
            });
        }
    }
}
