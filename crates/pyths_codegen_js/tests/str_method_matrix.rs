//! E3 — the str-method INLINE ≡ RUNTIME parity matrix.
//!
//! Generated FROM `method_table::TABLE` (the ONE lowering catalog): every
//! str-receiver method row must carry at least one value case here
//! (completeness assertion), and each case is compiled twice —
//!
//!   * INLINE path: a string-literal receiver (provably str ⇒ the Inline/
//!     Hybrid-inline/Rename lowering fires);
//!   * RUNTIME path: the receiver is wrapped in an identity function
//!     (`__opq("...")`) whose call result the type-inferencer cannot see
//!     through (`JsInferredType::Unknown`, and not a simple receiver), so
//!     Hybrid rows GENUINELY take their runtime helper and Runtime rows
//!     dispatch through a runtime-typed receiver. (E3 r3: the r2 receiver
//!     `("W" + "oRld")` inferred Primitive — str+str is provably str — so
//!     the Hybrid inlines still fired on BOTH paths and a false-world
//!     mutation like `pyStrIsalpha = () => false` stayed green. The
//!     structural assertion below now PROVES the runtime program calls
//!     every runtime helper, so that vacuity cannot silently return.)
//!
//! E3 r2 (codex): the guard COMPARES ALL THREE — inline path, runtime path,
//! and the LIVE CPython oracle. A guard that only checked the two compiled
//! paths against each other could go green while both drifted from Python
//! (the hollow-guard failure E7 documented); every case line is executed by
//! the oracle interpreter and both compiled outputs must match it
//! byte-for-byte.
//!
//! E3 r3 oracle policy (codex r2 blockers): PYTHS_ORACLE_PYTHON is
//! whitespace-split into argv (the documented `PYTHS_ORACLE_PYTHON="py
//! -3.14"` Windows form works — r2 passed the whole string as one
//! executable name); the resolved interpreter must be EXACTLY CPython
//! 3.14.x (3.12/3.13 can no longer masquerade as the pinned 3.14.7
//! oracle); and there is NO skip switch — a missing oracle FAILS the test.
//! A skipped oracle is not a green run (the release-gate lesson: a gate
//! that can pass on skipped checks is not a gate).

use std::collections::BTreeSet;
use std::process::Command;

use pyths_codegen_js::method_table::{ReceiverKind, Strategy, TABLE};

/// (method, receiver, args-source) — receiver is the LITERAL string content;
/// args are `.ps` source text.
const CASES: &[(&str, &str, &str)] = &[
    ("capitalize", "woRld \u{df}", ""),
    ("capitalize", "\u{1c6}ab", ""),
    ("casefold", "Stra\u{df}e", ""),
    ("casefold", "\u{130}X", ""),
    ("center", "ab", "7"),
    ("center", "ab", "8, \"*\""),
    ("count", "hello", "\"l\""),
    ("count", "hello", "\"l\", 1, 4"),
    ("count", "hello", "\"\""),
    ("encode", "h\u{e9}\u{df}", ""),
    ("encode", "h\u{e9}", "\"latin-1\""),
    ("encode", "h\u{e9}", "\"ascii\", \"replace\""),
    ("endswith", "hello", "\"lo\""),
    ("endswith", "hello", "(\"x\", \"lo\")"),
    ("endswith", "hello", "\"ll\", 1, 4"),
    ("expandtabs", "a\tb", ""),
    ("expandtabs", "a\tb", "4"),
    ("find", "hello", "\"l\""),
    ("find", "hello", "\"l\", 3"),
    ("find", "hello", "\"z\""),
    ("format", "{} {a}", "1, a=2"),
    ("format_map", "{a}", "{\"a\": 1}"),
    ("index", "hello", "\"l\""),
    ("isalnum", "ab12", ""),
    ("isalnum", "ab ", ""),
    ("isalpha", "a\u{f1}b", ""),
    ("isalpha", "ab1", ""),
    ("isascii", "abc", ""),
    ("isascii", "\u{e9}", ""),
    ("isdecimal", "123", ""),
    ("isdecimal", "\u{b2}", ""),
    ("isdigit", "\u{b2}3", ""),
    ("isdigit", "12x", ""),
    ("isidentifier", "a_b1", ""),
    ("isidentifier", "1ab", ""),
    ("islower", "ab1", ""),
    ("islower", "aB", ""),
    ("isnumeric", "\u{bd}\u{5341}", ""),
    ("isnumeric", "12x", ""),
    ("isprintable", "ab c", ""),
    ("isprintable", "a\u{a0}b", ""),
    ("isspace", " \t\u{a0}", ""),
    ("isspace", " x ", ""),
    ("istitle", "It'S Ok", ""),
    ("istitle", "IT", ""),
    ("isupper", "AB1", ""),
    ("isupper", "Ab", ""),
    ("join", "-", "[\"a\", \"b\"]"),
    ("ljust", "ab", "5"),
    ("ljust", "ab", "5, \"*\""),
    ("lower", "AbC\u{130}", ""),
    ("lstrip", "  ab ", ""),
    ("lstrip", "xxabx", "\"x\""),
    ("partition", "a-b-c", "\"-\""),
    ("removeprefix", "TestHook", "\"Test\""),
    ("removeprefix", "TestHook", "\"Xest\""),
    ("removesuffix", "MiscTests", "\"Tests\""),
    ("removesuffix", "MiscTests", "\"Xests\""),
    ("replace", "banana", "\"an\", \"AN\""),
    ("replace", "banana", "\"an\", \"AN\", 1"),
    ("replace", "banana", "\"\", \"-\", 3"),
    ("rfind", "banana", "\"na\""),
    ("rindex", "banana", "\"na\""),
    ("rjust", "ab", "5"),
    ("rpartition", "a-b-c", "\"-\""),
    ("rstrip", " ab\u{a0} ", ""),
    ("rstrip", "xabxx", "\"x\""),
    ("rsplit", "a b  c", ""),
    ("rsplit", "a-b-c", "\"-\", 1"),
    ("split", "a b  c", ""),
    ("split", "a-b-c", "\"-\""),
    ("split", "a-b-c", "\"-\", 1"),
    ("splitlines", "a\nb\u{b}c", ""),
    ("splitlines", "a\nb", "keepends=True"),
    ("startswith", "hello", "\"he\""),
    ("startswith", "hello", "(\"x\", \"he\")"),
    ("strip", " \u{a0}ab\t ", ""),
    ("strip", "xxabx", "\"x\""),
    ("swapcase", "AbC\u{df}", ""),
    ("title", "it's \u{1c6}e", ""),
    ("upper", "ab\u{df}", ""),
    ("zfill", "-5", "6"),
    ("zfill", "42", "5"),
    // maketrans/translate: exercised as a PAIR (translate needs a table).
    ("translate", "abc", "{97: \"A\", 98: None}"),
];

/// Rows whose parity is exercised elsewhere (with the reason).
const EXEMPT: &[(&str, &str)] = &[
    // str.maketrans is a STATIC method — no instance-receiver form to split
    // into inline/runtime paths; corpus.d/e3_str_format.json covers it
    // against CPython (and `translate` above consumes its table shape).
    ("maketrans", "static method; corpus-covered"),
    // format is also covered above; nothing exempted besides maketrans.
];

/// E3 r3 (codex r2 blocker): `ReceiverKind::Multi` rows that ARE str
/// methods — the completeness ratchet covers these too (r2 filtered on
/// `ReceiverKind::Str` only, so deleting the find/index/count cases left
/// the matrix green). Kept in sync with TABLE by the assertion below.
const STR_MULTI: &[&str] = &["count", "find", "index"];

fn esc_ps(s: &str) -> String {
    let mut out = String::new();
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\t' => out.push_str("\\t"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            c if (c as u32) < 0x20 => out.push_str(&format!("\\x{:02x}", c as u32)),
            c => out.push(c),
        }
    }
    out
}

fn build_program(inline_path: bool) -> String {
    let mut src = String::new();
    if !inline_path {
        // The identity wrapper's CALL RESULT is opaque to the inferencer
        // (JsInferredType::Unknown; not a simple receiver), so this leg
        // reaches every runtime helper for real.
        src.push_str("def __opq(x):\n    return x\n");
    }
    // The probe prints repr() so tuples/lists/bytes compare structurally.
    for (m, recv, args) in CASES {
        let recv_expr = if inline_path {
            format!("\"{}\"", esc_ps(recv))
        } else {
            format!("__opq(\"{}\")", esc_ps(recv))
        };
        src.push_str(&format!("print(repr({recv_expr}.{m}({args})))\n"));
    }
    src
}

/// Resolve the pinned CPython 3.14 oracle. PYTHS_ORACLE_PYTHON is
/// whitespace-split into argv; whatever resolves must be EXACTLY 3.14.x;
/// no oracle ⇒ the test FAILS (there is deliberately no skip switch).
fn resolve_oracle() -> (String, Vec<String>) {
    let is_314 = |bin: &str, args: &[String]| -> bool {
        Command::new(bin)
            .args(args)
            .arg("-c")
            .arg("import sys; raise SystemExit(0 if sys.version_info[:2] == (3, 14) else 1)")
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false)
    };
    if let Ok(spec) = std::env::var("PYTHS_ORACLE_PYTHON") {
        let mut parts = spec.split_whitespace().map(String::from);
        let bin = parts.next().expect("PYTHS_ORACLE_PYTHON is set but empty");
        let args: Vec<String> = parts.collect();
        assert!(
            is_314(&bin, &args),
            "PYTHS_ORACLE_PYTHON={spec:?} did not resolve to CPython 3.14.x — \
             the matrix pins the 3.14.7 oracle (a 3.12/3.13 interpreter must \
             not masquerade as it)"
        );
        return (bin, args);
    }
    let candidates: [(&str, &[&str]); 3] = [("py", &["-3.14"]), ("python3", &[]), ("python", &[])];
    for (bin, pre) in candidates {
        let args: Vec<String> = pre.iter().map(|s| s.to_string()).collect();
        if is_314(bin, &args) {
            return (bin.to_string(), args);
        }
    }
    panic!(
        "no CPython 3.14 oracle found (PYTHS_ORACLE_PYTHON / py -3.14 / \
         python3 / python). The matrix REQUIRES the live-oracle leg; a \
         skipped oracle is NOT a passing run."
    );
}

#[test]
fn test_str_method_matrix_paths_match_cpython() {
    // ---- completeness: every str-receiver TABLE row has a case or
    //      exemption. E3 r3: `ReceiverKind::Multi` rows that are str
    //      methods (STR_MULTI) are ratcheted too, and STR_MULTI itself is
    //      checked against TABLE so the two lists cannot drift.
    let covered: BTreeSet<&str> = CASES.iter().map(|(m, _, _)| *m).collect();
    let exempt: BTreeSet<&str> = EXEMPT.iter().map(|(m, _)| *m).collect();
    let multi_rows: BTreeSet<&str> = TABLE
        .iter()
        .filter(|e| matches!(e.receiver, ReceiverKind::Multi))
        .map(|e| e.name)
        .collect();
    for m in STR_MULTI {
        assert!(
            multi_rows.contains(m),
            "STR_MULTI lists {m:?} but TABLE has no ReceiverKind::Multi row \
             for it — keep the two in sync"
        );
    }
    let mut missing = Vec::new();
    for e in TABLE {
        let is_str_row = matches!(e.receiver, ReceiverKind::Str)
            || (matches!(e.receiver, ReceiverKind::Multi) && STR_MULTI.contains(&e.name));
        if is_str_row && !covered.contains(e.name) && !exempt.contains(e.name) {
            missing.push(e.name);
        }
    }
    assert!(
        missing.is_empty(),
        "str-method TABLE rows without a parity-matrix case (add a case or an \
         explicit exemption with a reason): {missing:?}"
    );

    // ---- STRUCTURAL non-vacuity (E3 r3): the runtime-path program must
    //      actually CALL every runtime helper of every covered str row —
    //      compiled in MODULE form (helper definitions live in the imported
    //      runtime package, so `helper(` in the JS is a genuine call site).
    //      This is what makes a false-world mutation of a helper
    //      (`pyStrIsalpha = () => false`) go RED: the executable runtime leg
    //      below routes through the mutated helper and diverges from the
    //      oracle.
    let runtime_src = build_program(false);
    let runtime_module = pyths_parser::parse(&runtime_src).expect("runtime program parses");
    let module_js = pyths_codegen_js::codegen(&runtime_module);
    let mut unrouted = Vec::new();
    for e in TABLE {
        let is_str_row = matches!(e.receiver, ReceiverKind::Str)
            || (matches!(e.receiver, ReceiverKind::Multi) && STR_MULTI.contains(&e.name));
        if !is_str_row || !covered.contains(e.name) {
            continue;
        }
        let helper = match e.strategy {
            Strategy::Runtime(h) => Some(h),
            Strategy::Hybrid { runtime, .. } => Some(runtime),
            _ => None, // Rename/Inline rows have no runtime twin to route to
        };
        if let Some(h) = helper {
            if !module_js.contains(&format!("{h}(")) {
                unrouted.push((e.name, h));
            }
        }
    }
    assert!(
        unrouted.is_empty(),
        "runtime-path program does NOT call these runtime helpers — the \
         opaque receiver failed to defeat inlining, so the false-world guard \
         is vacuous for them: {unrouted:?}"
    );

    // ---- compile both paths, run under node, diff line-by-line
    let dir = std::env::temp_dir().join(format!("pyths_strmatrix_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    pyths_runtime::materialize_runtime_package(&dir).expect("materialize runtime");

    let mut outputs = Vec::new();
    for (label, inline_path) in [("inline", true), ("runtime", false)] {
        let src = build_program(inline_path);
        let module = pyths_parser::parse(&src)
            .unwrap_or_else(|e| panic!("{label} program parse failed: {e:?}"));
        let js = pyths_codegen_js::codegen_inline(&module);
        let path = dir.join(format!("strmatrix_{label}.mjs"));
        std::fs::write(&path, &js).unwrap();
        let out = Command::new("node")
            .arg(&path)
            .output()
            .expect("node available");
        assert!(
            out.status.success(),
            "{label} program crashed (the #237 bad-lowering class):\n{}",
            String::from_utf8_lossy(&out.stderr)
        );
        outputs.push(String::from_utf8_lossy(&out.stdout).to_string());
    }
    // ---- the ORACLE leg: run the same (Python-compatible) program source
    //      through live CPython 3.14; both compiled paths must match it.
    let (bin, args) = resolve_oracle();
    let py_src = build_program(true); // literal receivers, valid Python
    let py_path = dir.join("strmatrix_oracle.py");
    std::fs::write(&py_path, &py_src).unwrap();
    let out = Command::new(&bin)
        .args(&args)
        .arg("-X")
        .arg("utf8")
        .arg(&py_path)
        .env("PYTHONIOENCODING", "utf-8")
        .output()
        .expect("oracle python runnable");
    assert!(
        out.status.success(),
        "oracle program crashed:\n{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let oracle_out = String::from_utf8_lossy(&out.stdout).replace("\r\n", "\n");
    let _ = std::fs::remove_dir_all(&dir);

    let inline_lines: Vec<&str> = outputs[0].lines().collect();
    let runtime_lines: Vec<&str> = outputs[1].lines().collect();
    assert_eq!(
        inline_lines.len(),
        runtime_lines.len(),
        "line-count mismatch"
    );
    let oracle_lines: Vec<String> = oracle_out.lines().map(|l| l.to_string()).collect();
    assert_eq!(
        oracle_lines.len(),
        inline_lines.len(),
        "oracle line-count mismatch"
    );
    let mut diffs = Vec::new();
    for (i, (a, b)) in inline_lines.iter().zip(runtime_lines.iter()).enumerate() {
        let (m, recv, args) = CASES[i];
        if a != b {
            diffs.push(format!(
                "{m}({args}) on {recv:?}: inline={a:?} runtime={b:?}"
            ));
        }
        let want = oracle_lines[i].as_str();
        if *a != want {
            diffs.push(format!(
                "{m}({args}) on {recv:?}: inline={a:?} CPYTHON={want:?}"
            ));
        }
        if *b != want {
            diffs.push(format!(
                "{m}({args}) on {recv:?}: runtime={b:?} CPYTHON={want:?}"
            ));
        }
    }
    assert!(
        diffs.is_empty(),
        "{} divergence(s) across inline/runtime/CPython:\n{}",
        diffs.len(),
        diffs.join("\n")
    );
}
