//! BigInt-range guard for the `random` stdlib module — the boundary fence
//! for the #461 class ("Cannot mix BigInt and other types") inside
//! `runtime/src/stdlib/random.js`.
//!
//! Root cause fixed: `randint`/`randrange`/`uniform` (and the cumulative-
//! weight math in `choices`) hand-mixed `_next()` floats with integer
//! arguments; when PythScribe represents a large Python int as a BigInt
//! (the pre-#451 hybrid model), those expressions threw
//! (`Math.floor(BigInt)`, `float * BigInt`). Reproducer: HumanEval/39
//! `prime_fib` calling `random.randint(a, b)` over a BigInt range.
//!
//! The fix routes every integer-domain draw through ONE authority,
//! `Random.prototype._randbelow(n)`:
//!   * Number path (safe-integer n): the ORIGINAL `Math.floor(_next()*n)` —
//!     values bit-for-bit unchanged, which the pinned seeded draws below
//!     prove (captured from the PRE-fix runtime).
//!   * BigInt path: CPython `_randbelow` structure — 32-bit words from
//!     `_next()`, exact bit-length, rejection sampling; returns BigInt.
//!
//! This test drives EVERY integer-domain random API with BigInt-range
//! arguments end-to-end (compile + node via `pyths run`) so the class
//! cannot silently reopen at another site.

use std::process::Command;

fn pyths_bin() -> Command {
    Command::new(env!("CARGO_BIN_EXE_pyths"))
}

/// Compile-and-run `src` through `pyths run`, returning stdout.
fn run_ps(name: &str, src: &str) -> String {
    let dir = std::env::temp_dir().join(format!("pyths_randbig_{}", name));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let ps = dir.join(format!("{}.ps", name));
    std::fs::write(&ps, src).unwrap();
    let out = pyths_bin()
        .args(["run", ps.to_str().unwrap()])
        .output()
        .expect("pyths run failed to spawn");
    let stdout = String::from_utf8_lossy(&out.stdout).replace("\r\n", "\n");
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        out.status.success(),
        "pyths run {name} should exit 0; stderr={stderr}\nstdout={stdout}"
    );
    let _ = std::fs::remove_dir_all(&dir);
    stdout
}

/// The full BigInt-range matrix: every integer-domain API must accept a
/// range around 10**30 without throwing, return an in-range value of the
/// correct Python type, and the float-domain `uniform` must return a float.
#[test]
fn bigint_range_matrix_no_throw_in_range() {
    let out = run_ps(
        "matrix",
        r#"
import random

B = 10 ** 30

v = random.randint(B, B + 5)
print("randint", B <= v and v <= B + 5, isinstance(v, int))

w = random.randrange(B)
print("randrange1", 0 <= w and w < B, isinstance(w, int))

x = random.randrange(B, B + 10, 2)
print("randrange3", B <= x and x < B + 10 and (x - B) % 2 == 0, isinstance(x, int))

u = random.uniform(B, B + 1)
print("uniform", isinstance(u, float))

m = random.randint(1, B)
print("mixed", 1 <= m and m <= B, isinstance(m, int))

c = random.choices([1, 2], weights=[1, B], k=3)
print("choices", len(c) == 3, all(e in [1, 2] for e in c))
"#,
    );
    assert_eq!(
        out,
        "randint True True\n\
         randrange1 True True\n\
         randrange3 True True\n\
         uniform True\n\
         mixed True True\n\
         choices True True\n",
        "every BigInt-range integer draw must succeed, land in range, and \
         keep the Python type (int in -> int out; uniform -> float)"
    );
}

/// The Number path is UNCHANGED: pinned draws captured by running the
/// PRE-fix runtime (`new Random(42)` under node, before `_randbelow`
/// existed). If any of these move, the small path stopped being the
/// original `Math.floor(_next() * n)` algorithm — a hard regression for
/// seeded reproducibility.
#[test]
fn seeded_number_path_values_unchanged() {
    let out = run_ps(
        "pinned",
        r#"
import random

r = random.Random(42)
print(r.randint(1, 6))
print(r.randint(1, 6))
print(r.randint(1, 6))
print(r.randint(1, 6))
print(r.randint(1, 6))

r2 = random.Random(42)
print(r2.randrange(0, 100, 7))
"#,
    );
    // Pre-fix pinned values (mulberry32, seed 42): randint(1,6) x5 then
    // randrange(0,100,7).
    assert_eq!(
        out, "3\n6\n5\n4\n5\n49\n",
        "seeded Number-path sequences must be bit-for-bit unchanged by the \
         BigInt fix"
    );
}

/// Sequence APIs over normal lists keep working (they index with native
/// `.length` Numbers — the safe sites the audit left untouched).
#[test]
fn sequence_apis_still_work() {
    let out = run_ps(
        "seqs",
        r#"
import random

items = [1, 2, 3, 4, 5]
c = random.choice(items)
print("choice", c in items)

s = random.sample(items, 3)
print("sample", len(s) == 3)

random.shuffle(items)
print("shuffle", len(items) == 5, sorted(items) == [1, 2, 3, 4, 5])
"#,
    );
    assert_eq!(
        out,
        "choice True\nsample True\nshuffle True True\n",
        "choice/sample/shuffle over a normal list must keep working"
    );
}
