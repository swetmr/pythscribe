//! Source-map DX regression gate (found by the reference-app DX/DevTools test,
//! 2026-07-29). Locks the three sourcemap-emitter fixes at CI scale (no browser):
//!   DX-3 the map is shifted for the import/prelude lines `finish()` prepends,
//!        so stack frames resolve to the right `.ps` line in DevTools;
//!   DX-2 the original source is inlined as `sourcesContent`;
//!   DX-1 `names` carries the preserved identifiers (never generated helpers).
//! The Playwright harness in reference-app is the end-to-end DevTools counterpart.

fn compile_map(src: &str) -> (String, serde_json::Value) {
    let module = pyths_parser::parse(src).expect("fixture parses");
    let out = pyths_codegen_js::codegen_inline_with_sourcemap(&module, src, "test.ps", "test.js");
    let map: serde_json::Value =
        serde_json::from_str(out.source_map.as_deref().expect("map emitted")).unwrap();
    (out.js, map)
}

// A decoded segment: (gen_line, gen_col, src_idx, orig_line, orig_col, name_idx?).
type Seg = (u32, u32, i64, i64, i64, Option<i64>);

/// Minimal base64-VLQ decoder — reconstruct ABSOLUTE segment positions from the
/// map's relative deltas. Used to assert the decoded map is actually CORRECT:
/// mutation testing (2026-07-29) found the delta arithmetic in `encode_mappings`
/// was executed by the structural tests but never asserted — 11 surviving
/// mutants in the delta/`==`/`||` logic. These decode-and-verify tests kill them.
fn decode_segments(map: &serde_json::Value) -> Vec<Seg> {
    const B64: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mappings = map["mappings"].as_str().unwrap();
    let mut segs = Vec::new();
    let (mut src, mut ol, mut oc, mut nm) = (0i64, 0i64, 0i64, 0i64);
    for (gline, line) in mappings.split(';').enumerate() {
        // 143 guard: a well-formed line never starts with a segment separator.
        assert!(
            !line.starts_with(','),
            "malformed mappings line (leading comma)"
        );
        let mut gc = 0i64;
        for seg in line.split(',').filter(|s| !s.is_empty()) {
            let mut fields = Vec::new();
            let (mut shift, mut val) = (0u32, 0i64);
            for &b in seg.as_bytes() {
                let d = B64.iter().position(|&c| c == b).expect("base64 char") as i64;
                let (cont, digit) = (d & 32, d & 31);
                val += digit << shift;
                if cont != 0 {
                    shift += 5;
                } else {
                    let neg = val & 1;
                    let v = if neg != 0 { -(val >> 1) } else { val >> 1 };
                    fields.push(v);
                    shift = 0;
                    val = 0;
                }
            }
            if fields.is_empty() {
                continue;
            }
            gc += fields[0];
            let mut name = None;
            if fields.len() >= 4 {
                src += fields[1];
                ol += fields[2];
                oc += fields[3];
            }
            if fields.len() >= 5 {
                nm += fields[4];
                name = Some(nm);
            }
            segs.push((gline as u32, gc as u32, src, ol, oc, name));
        }
    }
    segs
}

/// The identifier token starting exactly at `(line, col)` in `text` (mirrors the
/// emitter's `ident_at`, for the name-correctness assertion).
fn ident_at_src(text: &str, line: u32, col: u32) -> Option<String> {
    let (mut l, mut c) = (0u32, 0u32);
    let mut start = None;
    for (i, ch) in text.char_indices() {
        if l == line && c == col {
            start = Some(i);
            break;
        }
        if ch == '\n' {
            l += 1;
            c = 0;
        } else {
            c += 1;
        }
    }
    let start = start?;
    let rest = &text[start..];
    let mut end = 0usize;
    for (j, ch) in rest.char_indices() {
        if j == 0 {
            if !(ch == '_' || ch.is_alphabetic()) {
                return None;
            }
        } else if !(ch == '_' || ch.is_alphanumeric()) {
            break;
        }
        end = j + ch.len_utf8();
    }
    if end == 0 {
        None
    } else {
        Some(rest[..end].to_string())
    }
}

#[test]
fn sourcemap_decoded_positions_are_correct() {
    let src = "def f(x):\n    total = x + 1\n    return total\n\nprint(f(2))\n";
    let (js, map) = compile_map(src);
    let segs = decode_segments(&map);
    assert!(!segs.is_empty(), "map has segments");

    let src_lines = src.lines().count() as i64;
    let js_lines = js.lines().count() as u32;
    for &(gl, _gc, srci, ol, oc, _nm) in &segs {
        // Single source → its index never moves off 0 (kills the field-2 mutants).
        assert_eq!(srci, 0, "source index must stay 0");
        // A `-`→`+`/`-`→`/` mutation in the orig-line/col delta math produces
        // escalating out-of-range positions — caught here.
        assert!(
            ol >= 0 && ol < src_lines,
            "orig line {ol} out of [0,{src_lines})"
        );
        assert!((0..300).contains(&oc), "orig col {oc} implausible");
        // A `<`→`<=` mutation in the line-skip loop inflates generated lines.
        assert!(
            gl < js_lines + 2,
            "gen line {gl} exceeds js line count {js_lines}"
        );
    }
    // DX-3: every body mapping is shifted past the runtime prelude (line 0).
    assert!(
        segs.iter().all(|s| s.0 >= 1),
        "body mappings must be shifted past the prelude"
    );
}

#[test]
fn sourcemap_names_match_the_identifier_at_their_original_position() {
    let src = "def f(x):\n    total = x + 1\n    return total\n\nprint(f(2))\n";
    let (js, map) = compile_map(src);
    let names: Vec<String> = map["names"]
        .as_array()
        .unwrap()
        .iter()
        .map(|n| n.as_str().unwrap().to_string())
        .collect();
    let segs = decode_segments(&map);

    let mut checked = 0;
    for &(gl, gc, _srci, ol, oc, nm) in &segs {
        if let Some(ni) = nm {
            let name = &names[ni as usize];
            // A `names` entry is emitted ONLY for a preserved identifier, so it
            // must equal the token at BOTH ends of the mapping:
            //  - original side (ident_at on the .ps) — the emitter's `orig`;
            let ident = ident_at_src(src, ol as u32, oc as u32);
            assert_eq!(
                Some(name.clone()),
                ident,
                "named segment must equal the source identifier at orig ({ol},{oc})"
            );
            //  - generated side (ident_at on the FINAL js at the decoded gen
            //    position) — the emitter's `gen`. This is the assertion the
            //    3 residual mutants need: line-skip `<`→`<=` (inflates gen_line
            //    → wrong js line), gen-col `-`→`+` (wrong gen_col), and
            //    preserved_name `==`→`!=` (a name at a helper site where the
            //    generated token differs from the original) all break it.
            let gen_ident = ident_at_src(&js, gl, gc);
            assert_eq!(
                Some(name.clone()),
                gen_ident,
                "named segment's GENERATED token at ({gl},{gc}) must equal the name {name:?}"
            );
            assert!(
                !name.starts_with("py"),
                "generated helpers must never be named"
            );
            checked += 1;
        }
    }
    assert!(
        checked > 0,
        "expected at least one preserved-identifier name segment"
    );
    assert!(
        names.iter().any(|n| n == "total" || n == "f" || n == "x"),
        "expected preserved identifiers, got {names:?}"
    );
}

#[test]
fn sourcemap_is_prelude_shifted() {
    // A program that pulls a runtime prelude (print -> pyPrint, so `finish`
    // prepends the inline runtime before the body).
    let src = "def f(x):\n    return x + 1\n\nprint(f(2))\n";
    let (js, map) = compile_map(src);
    let mappings = map["mappings"].as_str().unwrap();

    // The body is preceded by a multi-line prelude in the final JS...
    assert!(
        !js.trim_start().starts_with("export function f") && js.contains("function f"),
        "expected a prelude before the body in the final JS"
    );
    // ...so the map must skip those leading generated lines: `mappings` now
    // begins with `;` groups (before the fix it began with a real segment at
    // generated line 0, which pointed DevTools at the wrong line).
    assert!(
        mappings.starts_with(';'),
        "DX-3 regression: mappings must start with skipped prelude lines, got {:?}",
        &mappings[..mappings.len().min(12)]
    );
}

#[test]
fn sourcemap_inlines_sources_content() {
    let src = "x = 1\nprint(x)\n";
    let (_js, map) = compile_map(src);
    // DX-2: the original .ps is inlined (a deployed bundle does not serve it).
    assert_eq!(
        map["sourcesContent"][0].as_str().unwrap(),
        src,
        "DX-2 regression: sourcesContent must inline the original source"
    );
}

#[test]
fn sourcemap_names_are_preserved_identifiers_only() {
    let src = "def f(x):\n    total = x + 1\n    return total\n\nprint(f(2))\n";
    let (_js, map) = compile_map(src);
    let names: Vec<String> = map["names"]
        .as_array()
        .unwrap()
        .iter()
        .map(|n| n.as_str().unwrap().to_string())
        .collect();
    // DX-1: real Python identifiers are labeled...
    assert!(
        names.iter().any(|n| n == "total" || n == "f" || n == "x"),
        "DX-1 regression: names should carry preserved identifiers, got {:?}",
        names
    );
    // ...but generated runtime helpers are NEVER labeled (that would mislead the
    // Scope panel — the whole reason `names` is gated on a preserved-token check).
    assert!(
        !names.iter().any(|n| n.starts_with("py")),
        "DX-1 regression: names must not include generated helpers, got {:?}",
        names
    );
}
