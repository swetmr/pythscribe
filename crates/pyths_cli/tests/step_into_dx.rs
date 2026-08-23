//! DX-4 (step-into) end-to-end CLI tests: `pyths bundle --sourcemap`
//! ignore-lists the runtime/glue sections, while user `.ps` maps stay
//! ignore-free — the map-level proof that DevTools stepping stays in `.ps`
//! code and skips pyths internals (the anti-Transcrypt property).

use std::process::Command;

fn pyths_bin() -> Command {
    Command::new(env!("CARGO_BIN_EXE_pyths"))
}

/// Minimal Source Map v3 VLQ decoder (1/4/5-field segments) for asserting
/// bundle-map contents. Returns `(gen_line, gen_col, src, orig_line, orig_col)`.
fn decode_mappings(mappings: &str) -> Vec<(u32, u32, u32, u32, u32)> {
    const CHARS: &str = "ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = Vec::new();
    let (mut src, mut orig_line, mut orig_col) = (0i64, 0i64, 0i64);
    for (gen_line, group) in mappings.split(';').enumerate() {
        let mut gen_col = 0i64;
        for seg in group.split(',') {
            if seg.is_empty() {
                continue;
            }
            let mut fields = Vec::new();
            let (mut value, mut shift) = (0i64, 0u32);
            for ch in seg.chars() {
                let digit = CHARS.find(ch).expect("valid base64 vlq char") as i64;
                value += (digit & 31) << shift;
                if digit & 32 != 0 {
                    shift += 5;
                } else {
                    let negative = value & 1 == 1;
                    let mut v = value >> 1;
                    if negative {
                        v = -v;
                    }
                    fields.push(v);
                    value = 0;
                    shift = 0;
                }
            }
            assert!(
                fields.len() == 1 || fields.len() == 4 || fields.len() == 5,
                "malformed segment {seg:?}"
            );
            gen_col += fields[0];
            if fields.len() >= 4 {
                src += fields[1];
                orig_line += fields[2];
                orig_col += fields[3];
                out.push((
                    gen_line as u32,
                    gen_col as u32,
                    src as u32,
                    orig_line as u32,
                    orig_col as u32,
                ));
            }
        }
    }
    out
}

/// SHOULD-HAVE 4: `pyths bundle --sourcemap` emits a map where user sections
/// map to their `.ps` sources and everything else (banner, wrappers, inlined
/// runtime) is mapped to ONE synthetic source that is ignore-listed under
/// BOTH keys — every bundle line is accounted for (no steppable holes).
#[test]
fn bundle_sourcemap_ignore_lists_glue_and_maps_user_code() {
    let dir = std::env::temp_dir().join(format!("pyths_dx4_bundle_{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("helper.ps"), "def double(x):\n    return x * 2\n").unwrap();
    std::fs::write(
        dir.join("app.ps"),
        "import helper\n\ndef main():\n    print(helper.double(21))\n\nmain()\n",
    )
    .unwrap();
    let out_js = dir.join("app.bundle.js");
    let out_map = dir.join("app.bundle.js.map");

    let output = pyths_bin()
        .args([
            "bundle",
            dir.join("app.ps").to_str().unwrap(),
            "--sourcemap",
            "-o",
            out_js.to_str().unwrap(),
        ])
        .output()
        .expect("run pyths bundle");
    assert!(
        output.status.success(),
        "bundle --sourcemap failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let js = std::fs::read_to_string(&out_js).unwrap();
    assert!(
        js.trim_end()
            .ends_with("//# sourceMappingURL=app.bundle.js.map"),
        "bundle must reference its map"
    );

    let map: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&out_map).unwrap()).unwrap();
    let sources: Vec<String> = map["sources"]
        .as_array()
        .unwrap()
        .iter()
        .map(|s| s.as_str().unwrap().to_string())
        .collect();
    // sources = [helper.ps, app.ps, glue] — glue LAST, and the only ignored one.
    assert_eq!(sources.len(), 3, "sources: {sources:?}");
    assert!(sources[0].ends_with("helper.ps"), "sources: {sources:?}");
    assert!(sources[1].ends_with("app.ps"), "sources: {sources:?}");
    assert_eq!(sources[2], "pyths:bundle-glue");
    let glue_idx = (sources.len() - 1) as u64;
    assert_eq!(map["ignoreList"], serde_json::json!([glue_idx]));
    assert_eq!(map["x_google_ignoreList"], serde_json::json!([glue_idx]));
    // User `.ps` sourcesContent inlined; glue content is a stub comment.
    assert!(map["sourcesContent"][1]
        .as_str()
        .unwrap()
        .contains("def main()"));

    // Decode: every bundle line is mapped; user lines resolve to `.ps`
    // sources, all remaining lines to the ignore-listed glue source.
    let segments = decode_mappings(map["mappings"].as_str().unwrap());
    let total_lines = js.matches('\n').count() as u32;
    let mapped_lines: std::collections::HashSet<u32> = segments.iter().map(|s| s.0).collect();
    assert_eq!(
        mapped_lines.len() as u32,
        total_lines,
        "every bundle line must be mapped (user or glue)"
    );
    // Line 0 is the generated header — glue-mapped.
    assert!(segments.iter().any(|s| s.0 == 0 && s.2 == glue_idx as u32));
    // The `print(helper.double(21))` statement (app.ps line 3, col 4,
    // 0-based) must be reachable: some bundle line maps to exactly that
    // original position, and the generated line it lands on carries the
    // compiled call (not a runtime helper).
    let seg = segments
        .iter()
        .find(|s| s.2 == 1 && s.3 == 3)
        .unwrap_or_else(|| panic!("no mapping to app.ps line 3: {segments:?}"));
    let gen_line_text = js.lines().nth(seg.0 as usize).unwrap();
    assert!(
        gen_line_text.contains("double("),
        "app.ps line 3 mapped to a non-call line: {gen_line_text:?}"
    );
    // helper.ps (src 0) mappings exist too — every user module is mapped.
    assert!(
        segments.iter().any(|s| s.2 == 0 && s.3 == 1),
        "helper.ps body mapping missing"
    );
    // The glue source is only ever mapped at its 0:0 stub — it exists to be
    // ignore-listed, not navigated.
    assert!(segments
        .iter()
        .filter(|s| s.2 == glue_idx as u32)
        .all(|s| s.3 == 0 && s.4 == 0));

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn bundle_minify_plus_sourcemap_is_rejected() {
    let dir = std::env::temp_dir().join(format!("pyths_dx4_minmap_{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("app.ps"), "print(1)\n").unwrap();
    let output = pyths_bin()
        .args([
            "bundle",
            dir.join("app.ps").to_str().unwrap(),
            "--sourcemap",
            "--minify",
            "-o",
            dir.join("app.bundle.js").to_str().unwrap(),
        ])
        .output()
        .expect("run pyths bundle");
    assert!(!output.status.success());
    assert!(
        String::from_utf8_lossy(&output.stderr).contains("--minify with --sourcemap"),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let _ = std::fs::remove_dir_all(&dir);
}

/// MUST-HAVE (c): a user `.ps` `--sourcemap` map still maps to the `.ps`
/// source and carries NO ignore-list keys — user code is never skipped.
#[test]
fn compile_sourcemap_user_map_is_never_ignore_listed() {
    let dir = std::env::temp_dir().join(format!("pyths_dx4_user_{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("hello.ps"), "def greet(name):\n    return name\n").unwrap();
    let out_js = dir.join("hello.js");
    let output = pyths_bin()
        .args([
            "compile",
            dir.join("hello.ps").to_str().unwrap(),
            "--sourcemap",
            "-o",
            out_js.to_str().unwrap(),
        ])
        .output()
        .expect("run pyths compile");
    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let map_raw = std::fs::read_to_string(dir.join("hello.js.map")).unwrap();
    let map: serde_json::Value = serde_json::from_str(&map_raw).unwrap();
    assert_eq!(map["sources"], serde_json::json!(["hello.ps"]));
    assert!(!map["mappings"].as_str().unwrap().is_empty());
    assert!(map.get("ignoreList").is_none(), "user map: {map_raw}");
    assert!(map.get("x_google_ignoreList").is_none());
    let _ = std::fs::remove_dir_all(&dir);
}

/// SHOULD-HAVE 5 (auto+kernel gap closed): under automatic routing, a kernel
/// module compiled WITH `--sourcemap` must route the kernel through the WASM
/// glue exactly like the non-sourcemap compile does (the `--sourcemap` path
/// used to drop `wasm_skip`, silently re-implementing kernels in JS).
#[test]
fn compile_sourcemap_auto_kernel_still_routes_through_glue() {
    let dir = std::env::temp_dir().join(format!("pyths_dx4_kernel_{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(
        dir.join("kern.ps"),
        "def add(a: int, b: int) -> int:\n    return a + b\n",
    )
    .unwrap();

    // Reference: non-sourcemap auto compile.
    let out_plain = dir.join("plain.js");
    let output = pyths_bin()
        .args([
            "compile",
            dir.join("kern.ps").to_str().unwrap(),
            "-o",
            out_plain.to_str().unwrap(),
        ])
        .output()
        .expect("run pyths compile");
    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let plain_js = std::fs::read_to_string(&out_plain).unwrap();
    if !plain_js.contains(".glue.js") {
        // Auto routing did not admit the kernel on this build — the parity
        // claim below would be vacuous; fail loudly so the fixture is fixed.
        panic!("expected auto routing to WASM-admit `add`: {plain_js}");
    }

    // Under test: --sourcemap auto compile of the same module.
    let out_mapped = dir.join("mapped.js");
    let output = pyths_bin()
        .args([
            "compile",
            dir.join("kern.ps").to_str().unwrap(),
            "--sourcemap",
            "-o",
            out_mapped.to_str().unwrap(),
        ])
        .output()
        .expect("run pyths compile --sourcemap");
    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let mapped_js = std::fs::read_to_string(&out_mapped).unwrap();
    assert!(
        mapped_js.contains("mapped.glue.js"),
        "--sourcemap output must re-export the kernel from the glue like the \
         plain output does:\n{mapped_js}"
    );
    assert!(
        mapped_js.contains("export { add }"),
        "kernel re-export missing:\n{mapped_js}"
    );
    // And its map still resolves to the `.ps` source, ignore-free.
    let map: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(dir.join("mapped.js.map")).unwrap()).unwrap();
    assert_eq!(map["sources"], serde_json::json!(["kern.ps"]));
    assert!(map.get("ignoreList").is_none());
    let _ = std::fs::remove_dir_all(&dir);
}

/// Review SHOULD-FIX (bundle map hygiene): (1) `sources` are RELATIVE to the
/// entry dir — no absolute build-machine paths leak into a shipped `.js.map`;
/// (2) `--no-sources-content` drops the inlined `.ps` (A17 parity) while the
/// glue stays ignore-listed.
#[test]
fn bundle_sourcemap_sources_are_relative_and_no_sources_content_honored() {
    let dir = std::env::temp_dir().join(format!("pyths_dx4_hygiene_{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("helper.ps"), "def double(x):\n    return x * 2\n").unwrap();
    std::fs::write(
        dir.join("app.ps"),
        "import helper\n\nprint(helper.double(21))\n",
    )
    .unwrap();

    // (1) default --sourcemap: sources relative, content inlined.
    let out = pyths_bin()
        .args([
            "bundle",
            dir.join("app.ps").to_str().unwrap(),
            "--sourcemap",
            "-o",
            dir.join("a.bundle.js").to_str().unwrap(),
        ])
        .output()
        .expect("run bundle");
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let map: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(dir.join("a.bundle.js.map")).unwrap())
            .unwrap();
    for s in map["sources"].as_array().unwrap() {
        let s = s.as_str().unwrap();
        assert!(
            !s.contains(":\\") && !s.contains(":/") && !s.starts_with('/'),
            "bundle map source must be relative, got absolute: {s:?}"
        );
    }
    assert_eq!(
        map["sources"],
        serde_json::json!(["helper.ps", "app.ps", "pyths:bundle-glue"])
    );
    assert!(map["sourcesContent"][1]
        .as_str()
        .unwrap()
        .contains("double"));

    // (2) --no-sources-content: every sourcesContent entry null, glue still ignored.
    let out = pyths_bin()
        .args([
            "bundle",
            dir.join("app.ps").to_str().unwrap(),
            "--sourcemap",
            "--no-sources-content",
            "-o",
            dir.join("b.bundle.js").to_str().unwrap(),
        ])
        .output()
        .expect("run bundle --no-sources-content");
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let map: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(dir.join("b.bundle.js.map")).unwrap())
            .unwrap();
    for c in map["sourcesContent"].as_array().unwrap() {
        assert!(
            c.is_null(),
            "--no-sources-content must null every entry: {c:?}"
        );
    }
    let glue_idx = (map["sources"].as_array().unwrap().len() - 1) as u64;
    assert_eq!(map["ignoreList"], serde_json::json!([glue_idx]));
    let _ = std::fs::remove_dir_all(&dir);
}
