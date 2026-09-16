//! The WASM ABI contract constants — the SINGLE EMITTED SOURCE (spec
//! 13-09-26-lib-selfcontained-pip-wheel, requirements §5.6, plan §M2.1; rev-4
//! opus B1 option (a)).
//!
//! Every module the compiler emits carries (i) a `pyths.abi` custom section
//! (`{abi, list_layout, array_layout, compiler, history}` as canonical JSON,
//! [`abi_section_json`]) and (ii) an exported immutable i32 global
//! `__pyths_abi` (the major). The generated JS glue inlines the same three
//! contract fields as `__PYTHS_ABI` and refuses a module whose section
//! disagrees; the pip runtime (`pythscribe/runtime/abi.py`) and the browser
//! shim (`pythscribe/ffi/list_buffer.mjs`) keep DECLARED MIRRORS of the layout
//! strings at their K7 homes and compare the section at load — major AND both
//! layout strings, field by field — so a module built by a compiler with a
//! different memory layout fails LOUD at load, never silently misreads.
//!
//! The layout *truth* lives in `emit.rs` (list `[len:i32][cap:i32][elems]`) and
//! `bridge.rs` (array header `[dtype@0][ndim@4][shape0@8][shape1@12]`), bound
//! byte-wise by `bridge_test.rs::test_bridge_array_header_layout_byte_agreement`
//! and the Lean marshalling table; this module binds the LABELS. The rule
//! (gated by the tests below + `tests/pythscribe/abi_golden.json`): a layout
//! string may change ONLY together with a NEW row in [`ABI_HISTORY`] carrying a
//! NEW major — never by editing a past row, never without a row.

/// The ABI major this compiler emits. Bump together with a new [`ABI_HISTORY`] row.
pub const PYTHS_ABI_MAJOR: u32 = 1;

/// The list-buffer layout label (`[len:i32 LE][cap:i32 LE][elements...]`, i64 /
/// f64 / i32-bool elements). Mirrors: `pythscribe/runtime/__init__.py::LAYOUT_VERSION`,
/// `list_buffer.mjs::LAYOUT_VERSION` (three byte-identical copies).
pub const LIST_LAYOUT_VERSION: &str = "pyths-0.2.4-list-v1";

/// The typed-array header layout label (`[dtype@0][ndim@4][shape0@8][shape1@12]`,
/// elements @16). Mirrors: `pythscribe/runtime/array_buffer.py::ARRAY_LAYOUT_VERSION`,
/// `list_buffer.mjs::ARRAY_LAYOUT_VERSION`.
pub const ARRAY_LAYOUT_VERSION: &str = "pyths-0.2.5-array-v2";

/// Every (major, list_layout, array_layout) triple ever shipped, oldest first.
/// The LAST row MUST equal the current constants (`abi_history_last_row_is_current`);
/// past rows are immutable (`abi_history_first_row_is_the_v1_contract` + the
/// committed Python golden `tests/pythscribe/abi_golden.json`).
pub const ABI_HISTORY: &[(u32, &str, &str)] = &[(1, "pyths-0.2.4-list-v1", "pyths-0.2.5-array-v2")];

/// Name of the custom section carrying the contract.
pub const ABI_SECTION_NAME: &str = "pyths.abi";

/// Name of the exported immutable i32 global carrying the major.
pub const ABI_GLOBAL_EXPORT: &str = "__pyths_abi";

/// The compiler version stamped into the section (the workspace version —
/// what `pyths --version` prints and what the pip runtime pins).
pub const COMPILER_VERSION: &str = env!("CARGO_PKG_VERSION");

/// A JSON string literal for an ASCII label. The labels are compile-time
/// constants restricted to `[A-Za-z0-9.-]` (asserted by `labels_are_plain_ascii`),
/// so no escaping is ever needed; the check keeps a future label honest.
fn json_str(s: &str) -> String {
    debug_assert!(
        s.bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'.' || b == b'-' || b == b'_'),
        "ABI label {s:?} must be plain ASCII [A-Za-z0-9._-]"
    );
    format!("\"{s}\"")
}

/// The canonical `pyths.abi` payload: one JSON object, fields in this FIXED order
/// (`abi`, `list_layout`, `array_layout`, `compiler`, `history`), no whitespace.
/// The Rust round-trip test asserts the emitted section's bytes equal this string
/// exactly, so the emit path cannot drop or reorder a field.
pub fn abi_section_json() -> String {
    let history = ABI_HISTORY
        .iter()
        .map(|(major, list, array)| format!("[{},{},{}]", major, json_str(list), json_str(array)))
        .collect::<Vec<_>>()
        .join(",");
    format!(
        "{{\"abi\":{},\"list_layout\":{},\"array_layout\":{},\"compiler\":{},\"history\":[{}]}}",
        PYTHS_ABI_MAJOR,
        json_str(LIST_LAYOUT_VERSION),
        json_str(ARRAY_LAYOUT_VERSION),
        json_str(COMPILER_VERSION),
        history
    )
}

fn push_leb128_u32(mut v: u32, out: &mut Vec<u8>) {
    loop {
        let byte = (v & 0x7f) as u8;
        v >>= 7;
        if v == 0 {
            out.push(byte);
            break;
        }
        out.push(byte | 0x80);
    }
}

fn read_leb128_u32(bytes: &[u8], pos: usize) -> Option<(u32, usize)> {
    let mut result: u32 = 0;
    let mut shift = 0u32;
    for (i, &b) in bytes.get(pos..)?.iter().enumerate().take(5) {
        result |= u32::from(b & 0x7f) << shift;
        if b & 0x80 == 0 {
            return Some((result, i + 1));
        }
        shift += 7;
    }
    None
}

/// The fully encoded custom section (id 0 + size + name + payload) — what
/// `emit.rs` appends through `wasm_encoder::CustomSection` and what
/// `optimize.rs` re-appends should `wasm-opt` strip it.
pub fn abi_section_bytes() -> Vec<u8> {
    let name = ABI_SECTION_NAME.as_bytes();
    let json = abi_section_json();
    let mut payload = Vec::with_capacity(name.len() + json.len() + 4);
    push_leb128_u32(name.len() as u32, &mut payload);
    payload.extend_from_slice(name);
    payload.extend_from_slice(json.as_bytes());
    let mut section = Vec::with_capacity(payload.len() + 6);
    section.push(0u8);
    push_leb128_u32(payload.len() as u32, &mut section);
    section.extend_from_slice(&payload);
    section
}

/// The payloads of every `pyths.abi` custom section in `bytes`, in order.
/// Fail-closed: malformed input yields whatever was found before the damage.
pub fn read_abi_sections(bytes: &[u8]) -> Vec<Vec<u8>> {
    let mut found = Vec::new();
    if bytes.len() < 8 || &bytes[0..4] != b"\0asm" {
        return found;
    }
    let mut i = 8;
    while i < bytes.len() {
        let id = bytes[i];
        i += 1;
        let Some((size, n)) = read_leb128_u32(bytes, i) else {
            return found;
        };
        i += n;
        let Some(end) = i.checked_add(size as usize) else {
            return found;
        };
        if end > bytes.len() {
            return found;
        }
        if id == 0 {
            if let Some((name_len, m)) = read_leb128_u32(bytes, i) {
                let name_start = i + m;
                if let Some(name_end) = name_start.checked_add(name_len as usize) {
                    if name_end <= end
                        && &bytes[name_start..name_end] == ABI_SECTION_NAME.as_bytes()
                    {
                        found.push(bytes[name_end..end].to_vec());
                    }
                }
            }
        }
        i = end;
    }
    found
}

/// Does the module carry exactly one `pyths.abi` section whose payload is
/// this compiler's canonical JSON?
pub fn has_current_abi_section(bytes: &[u8]) -> bool {
    let secs = read_abi_sections(bytes);
    secs.len() == 1 && secs[0] == abi_section_json().as_bytes()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A layout-string change WITHOUT a new history row fails here (the rule).
    #[test]
    fn abi_history_last_row_is_current() {
        let last = ABI_HISTORY.last().expect("ABI_HISTORY is never empty");
        assert_eq!(
            *last,
            (PYTHS_ABI_MAJOR, LIST_LAYOUT_VERSION, ARRAY_LAYOUT_VERSION),
            "ABI_HISTORY's last row must equal the current constants: a layout string may \
             change ONLY together with a NEW row carrying a NEW major"
        );
    }

    /// Editing the shipped v1 row in place (instead of appending) fails here.
    #[test]
    fn abi_history_first_row_is_the_v1_contract() {
        assert_eq!(
            ABI_HISTORY[0],
            (1, "pyths-0.2.4-list-v1", "pyths-0.2.5-array-v2"),
            "the v1 row is immutable history"
        );
    }

    #[test]
    fn abi_history_majors_strictly_increase_and_rows_are_distinct() {
        for w in ABI_HISTORY.windows(2) {
            assert!(w[0].0 < w[1].0, "majors must strictly increase: {:?}", w);
            assert!(
                (w[0].1, w[0].2) != (w[1].1, w[1].2),
                "a new major must change at least one layout label: {:?}",
                w
            );
        }
    }

    #[test]
    fn labels_are_plain_ascii() {
        for s in [LIST_LAYOUT_VERSION, ARRAY_LAYOUT_VERSION, COMPILER_VERSION] {
            assert!(
                s.bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b == b'.' || b == b'-' || b == b'_'),
                "{s:?}"
            );
        }
    }

    #[test]
    fn section_json_has_every_field_in_order() {
        let j = abi_section_json();
        let keys = [
            "\"abi\":",
            "\"list_layout\":",
            "\"array_layout\":",
            "\"compiler\":",
            "\"history\":",
        ];
        let mut last = 0;
        for k in keys {
            let pos = j.find(k).unwrap_or_else(|| panic!("{k} missing from {j}"));
            assert!(pos > last || last == 0, "{k} out of order in {j}");
            last = pos;
        }
        assert!(j.contains(&format!("\"abi\":{PYTHS_ABI_MAJOR},")));
        assert!(j.contains(&format!("\"list_layout\":\"{LIST_LAYOUT_VERSION}\"")));
        assert!(j.contains(&format!("\"array_layout\":\"{ARRAY_LAYOUT_VERSION}\"")));
        assert!(j.contains(&format!("\"compiler\":\"{COMPILER_VERSION}\"")));
        assert!(j.ends_with("]}"));
    }

    #[test]
    fn section_bytes_roundtrip_through_the_reader() {
        let mut m = b"\0asm\x01\0\0\0".to_vec();
        assert!(read_abi_sections(&m).is_empty());
        m.extend_from_slice(&abi_section_bytes());
        let secs = read_abi_sections(&m);
        assert_eq!(secs.len(), 1);
        assert_eq!(secs[0], abi_section_json().as_bytes());
        assert!(has_current_abi_section(&m));
        // a second copy is NOT "current" (ambiguous) and a foreign payload is not either
        let mut two = m.clone();
        two.extend_from_slice(&abi_section_bytes());
        assert!(!has_current_abi_section(&two));
        assert!(!has_current_abi_section(b"\0asm\x01\0\0\0"));
        // the encoded section must not break a validator
        wasmparser::validate(&m).expect("a module with only the abi section validates");
    }
}
