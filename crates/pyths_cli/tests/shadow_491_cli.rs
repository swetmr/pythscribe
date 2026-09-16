//! #491 call-time builtin-shadow resolution — end-to-end `js+wasm` controls under
//! Node (the behavioral oracle), plus the twin's JS text. Each program's expected
//! output is CPython's (recorded in `tests/differential/shadow_491/cases`, where
//! the same shapes run 3-way against the pinned interpreter); here the shipped
//! `pyths` binary compiles `--target js+wasm`, the glue + module are rewired to the
//! repo runtime, and node runs the module body.
//!
//! Paired controls (RED at base commit de345491, see
//! `tests/differential/shadow_491/evidence/base-de345491.txt`):
//!   - B1 twin: the overflow fallback must return the USER `abs` result;
//!   - the math-import twin: a `floor` twin keeps its import (old defs-only twin →
//!     ReferenceError at fallback);
//!   - B3: a module call before the shadowing def sees the builtin, after it the
//!     user fn, and the caller is DEMOTED (`Skipped use_abs`);
//!   - S3: import → def → import / def → import → def chains LOAD and last-wins.

use std::path::{Path, PathBuf};
use std::process::Command;

fn pyths_bin() -> Command {
    Command::new(env!("CARGO_BIN_EXE_pyths"))
}

fn node_present() -> bool {
    Command::new("node")
        .arg("--version")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

fn runtime_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("runtime")
        .join("src")
        .canonicalize()
        .unwrap()
}

fn file_url(p: &Path) -> String {
    format!(
        "file:///{}",
        p.to_string_lossy()
            .trim_start_matches("\\\\?\\")
            .replace('\\', "/")
    )
}

/// Rewire `pyths-runtime[/sub/path]` specifiers in one emitted file to the repo
/// runtime (the same rewrite the differential harnesses do).
fn rewire(p: &Path) {
    let rt = runtime_dir();
    let src = std::fs::read_to_string(p).unwrap();
    let mut out = String::with_capacity(src.len());
    for line in src.lines() {
        let mut l = line.to_string();
        if let Some(start) = l.find("\"pyths-runtime/") {
            let rest = &l[start + 1..];
            let end = rest.find('"').unwrap();
            let spec = &rest[..end]; // pyths-runtime/stdlib/math
            let sub = &spec["pyths-runtime/".len()..];
            let target = rt.join(format!("{sub}.js"));
            l = l.replace(
                &format!("\"{spec}\""),
                &format!("\"{}\"", file_url(&target)),
            );
        }
        l = l.replace(
            "\"pyths-runtime\"",
            &format!("\"{}\"", file_url(&rt.join("index.js"))),
        );
        out.push_str(&l);
        out.push('\n');
    }
    std::fs::write(p, out).unwrap();
}

struct Run {
    stdout: String,
    stderr: String,
    ok: bool,
    admitted: Vec<String>,
    skipped: Vec<String>,
    js: String,
    glue: String,
}

/// Compile `src` with `--target js+wasm --verbose`, rewire, run under node.
fn run_js_wasm(name: &str, src: &str) -> Run {
    let dir = std::env::temp_dir().join(format!("pyths_shadow_491_{name}"));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let ps = dir.join(format!("{name}.ps"));
    std::fs::write(&ps, src).unwrap();
    let js = dir.join(format!("{name}.js"));
    let out = pyths_bin()
        .env("PYTHS_NO_CACHE", "1")
        .args([
            "compile",
            ps.to_str().unwrap(),
            "-o",
            js.to_str().unwrap(),
            "--target",
            "js+wasm",
            "--verbose",
        ])
        .output()
        .expect("pyths spawn");
    let log = format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(out.status.success(), "compile failed:\n{log}");
    // The verbose lines carry ANSI color codes; match by substring (the same
    // `WASM: (\w+)` / `Skipped (\w+):` shape the Python harnesses use).
    let word_after = |l: &str, key: &str| -> Option<String> {
        let i = l.find(key)? + key.len();
        let w: String = l[i..]
            .chars()
            .take_while(|c| c.is_alphanumeric() || *c == '_')
            .collect();
        (!w.is_empty()).then_some(w)
    };
    let admitted: Vec<String> = log
        .lines()
        .filter_map(|l| word_after(l, "WASM: "))
        .collect();
    let skipped: Vec<String> = log
        .lines()
        .filter_map(|l| word_after(l, "Skipped "))
        .collect();
    std::fs::write(dir.join("package.json"), "{\"type\":\"module\"}\n").unwrap();
    rewire(&js);
    let glue_path = dir.join(format!("{name}.glue.js"));
    if glue_path.exists() {
        rewire(&glue_path);
    }
    let node = Command::new("node").arg(&js).output().expect("node spawn");
    let r = Run {
        stdout: String::from_utf8_lossy(&node.stdout).replace("\r\n", "\n"),
        stderr: String::from_utf8_lossy(&node.stderr).to_string(),
        ok: node.status.success(),
        admitted,
        skipped,
        js: std::fs::read_to_string(&js).unwrap(),
        glue: std::fs::read_to_string(&glue_path).unwrap_or_default(),
    };
    let _ = std::fs::remove_dir_all(&dir);
    r
}

/// B1 (fable): the overflow twin resolves `abs` to the USER def. `use_abs` and
/// `abs` are both WASM; 2**62 * 7 overflows i64, so the glue re-runs the JS twin —
/// which must compute the user value (32281802128991715328), not `pyAbs`'s
/// 4611686018427387904. The twin text: `use_abs` calls the bare `abs` cell, and the
/// cell is initialized to the builtin then rebound to the user def inside `__jsfb`.
#[test]
fn twin_overflow_fallback_returns_the_user_shadow_result() {
    if !node_present() {
        return;
    }
    let r = run_js_wasm(
        "twin_ovf",
        "def use_abs(x: int) -> int:\n    return abs(x)\n\n\
         def abs(x: int) -> int:\n    return x * 7\n\n\
         print(use_abs(-5))\nprint(use_abs(2 ** 62))\nprint(use_abs(-(2 ** 62)))\n",
    );
    assert!(r.ok, "node failed: {}", r.stderr);
    assert_eq!(r.admitted, vec!["abs".to_string(), "use_abs".to_string()]);
    assert_eq!(
        r.stdout, "-35\n32281802128991715328\n-32281802128991715328\n",
        "CPython: -35 / 2**62*7 / -(2**62*7)"
    );
    let twin_start = r.glue.find("const __jsfb").expect("twin IIFE present");
    let twin = &r.glue[twin_start..];
    assert!(
        twin.contains("let abs = pyAbs;") && twin.contains("abs = function"),
        "the twin owns a builtin-initialized cell rebound to the user def:\n{twin}"
    );
    assert!(
        !twin.contains("pyAbs(x)"),
        "the twin's use_abs must call the cell, never a baked pyAbs:\n{twin}"
    );
}

/// The math-import twin: `floor` is a canonical `from math import`; `f` overflows
/// i64 for 2**62, so the twin runs and must resolve `floor` through the import the
/// twin module carries (the old defs-only twin construction dropped every import →
/// `floor is not defined` at fallback time).
#[test]
fn twin_math_import_resolves_at_fallback() {
    if !node_present() {
        return;
    }
    let r = run_js_wasm(
        "twin_math",
        "from math import floor\n\n\
         def f(x: int) -> int:\n    return int(floor(x * 1.0)) * 7\n\n\
         print(f(3))\nprint(f(2 ** 62))\n",
    );
    assert!(r.ok, "node failed: {}", r.stderr);
    assert_eq!(
        r.admitted,
        vec!["f".to_string()],
        "f must be WASM for the twin to matter"
    );
    assert_eq!(r.stdout, "21\n32281802128991715328\n");
    assert!(
        r.glue.contains("stdlib/math"),
        "the glue carries the twin module's math import:\n{}",
        r.glue
    );
}

/// B3 (fable): a WASM-eligible function called at module scope BEFORE the
/// shadowing def sees the builtin for that call (5) and the user fn after (-35).
/// The compiler DEMOTES `use_abs` (a WASM body would bake one binding).
#[test]
fn early_module_call_sees_builtin_then_user_and_caller_is_demoted() {
    if !node_present() {
        return;
    }
    let r = run_js_wasm(
        "b3_early",
        "def use_abs(x: int) -> int:\n    return abs(x)\n\n\
         print(use_abs(-5))\n\n\
         def abs(x: int) -> int:\n    return x * 7\n\n\
         print(use_abs(-5))\n",
    );
    assert!(r.ok, "node failed: {}", r.stderr);
    assert_eq!(r.stdout, "5\n-35\n");
    assert!(
        r.skipped.contains(&"use_abs".to_string()) && !r.admitted.contains(&"use_abs".to_string()),
        "use_abs must be demoted: admitted={:?} skipped={:?}",
        r.admitted,
        r.skipped
    );
    assert!(r.admitted.contains(&"abs".to_string()));
    // The JS module: a builtin-initialized cell, rebound at the def's position to
    // the hidden glue import (no `Identifier already declared`).
    assert!(r.js.contains("export let abs = pyAbs;"), "{}", r.js);
    assert!(r.js.contains("abs = __wasm$abs;"), "{}", r.js);
    assert!(r.js.contains("as __wasm$abs }"), "{}", r.js);
}

/// S3: import → def → import and def → import → def of one name LOAD (no
/// double declaration) and resolve last-binder-wins at every point, including a
/// function defined before all of them and called between each step.
#[test]
fn import_def_import_chains_load_and_resolve_last_wins() {
    if !node_present() {
        return;
    }
    let r = run_js_wasm(
        "s3_chain",
        "from math import sqrt\n\n\
         def sqrt(x: int) -> int:\n    return x * x\n\n\
         print(sqrt(3))\nfrom math import sqrt\nprint(sqrt(16.0))\n\n\
         def use_floor(x: float) -> float:\n    return floor(x) * 1.0\n\n\
         def floor(x: float) -> float:\n    return x + 100.0\n\n\
         print(floor(2.5))\nprint(use_floor(2.5))\nfrom math import floor\n\
         print(floor(2.5))\nprint(use_floor(2.5))\n\n\
         def floor(x: float) -> float:\n    return x - 100.0\n\n\
         print(floor(2.5))\nprint(use_floor(2.5))\n",
    );
    assert!(r.ok, "node failed (S3 double-declaration?): {}", r.stderr);
    assert_eq!(
        r.stdout, "9\n4.0\n102.5\n102.5\n2\n2.0\n-97.5\n-97.5\n",
        "CPython's last-binder-wins sequence"
    );
}

/// `del` of the shadowing def restores the BUILTIN — at module scope and for a
/// function called afterwards.
#[test]
fn del_restores_the_builtin() {
    if !node_present() {
        return;
    }
    let r = run_js_wasm(
        "del_restore",
        "def use(x: int) -> int:\n    return abs(x)\n\n\
         def abs(x: int) -> int:\n    return x * 7\n\n\
         print(abs(-5))\nprint(use(-5))\ndel abs\nprint(abs(-5))\nprint(use(-5))\n",
    );
    assert!(r.ok, "node failed: {}", r.stderr);
    assert_eq!(r.stdout, "-35\n-35\n5\n5\n");
    assert!(
        r.js.contains("abs = pyAbs;"),
        "del restores the cell's init:\n{}",
        r.js
    );
}
