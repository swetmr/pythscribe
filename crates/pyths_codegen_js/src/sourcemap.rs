//! Source map v3 generation.
//!
//! Tracks mappings between generated JS positions and original .ps positions,
//! then serializes to the standard Source Map v3 JSON format.

/// A single mapping entry: generated position → original position.
#[derive(Debug, Clone)]
pub struct Mapping {
    /// 0-based line in generated output
    pub gen_line: u32,
    /// 0-based column in generated output
    pub gen_col: u32,
    /// 0-based line in original source
    pub orig_line: u32,
    /// 0-based column in original source
    pub orig_col: u32,
}

/// Builds a source map from a list of mappings.
pub struct SourceMapBuilder {
    pub mappings: Vec<Mapping>,
    pub source_file: String,
    pub output_file: String,
    /// DX-2: the original `.ps` source, inlined as `sourcesContent` so DevTools
    /// can show the original without fetching the `.ps` (which a deployed/edge
    /// bundle does not serve). `None` → the key is omitted.
    pub source_content: Option<String>,
    /// DX-3: line offset added to every recorded `gen_line` at/after
    /// `gen_line_shift_floor`. Mappings are recorded against the codegen BODY;
    /// `finish()` prepends a runtime-import/React prelude, so the map must be
    /// shifted down by the prelude line count (set by `finish_with_sourcemap`),
    /// else stack frames resolve to the wrong `.ps` line in DevTools.
    pub gen_line_shift: u32,
    /// Lines kept at the very top (a hoisted `"use client"`/`"use server"`
    /// directive); mappings below this floor are not shifted.
    pub gen_line_shift_floor: u32,
    /// DX-1: the FINAL generated JS. Used to populate `names` ONLY for
    /// genuinely preserved identifiers (generated token == original token) —
    /// never for helper-call sites, which would mislabel.
    pub generated_js: Option<String>,
}

impl SourceMapBuilder {
    pub fn new(source_file: &str, output_file: &str) -> Self {
        Self {
            mappings: Vec::new(),
            source_file: source_file.to_string(),
            output_file: output_file.to_string(),
            source_content: None,
            gen_line_shift: 0,
            gen_line_shift_floor: 0,
            generated_js: None,
        }
    }

    /// DX-2: attach the original source text (inlined into `sourcesContent`).
    pub fn set_source_content(&mut self, content: String) {
        self.source_content = Some(content);
    }

    /// DX-3: shift generated lines at/after `floor` down by `shift` (the prelude
    /// `finish()` prepends). `floor` is the count of top-of-file directive lines.
    pub fn set_line_shift(&mut self, shift: u32, floor: u32) {
        self.gen_line_shift = shift;
        self.gen_line_shift_floor = floor;
    }

    /// DX-1: the final generated JS, consulted so `names` is emitted only for
    /// preserved identifiers.
    pub fn set_generated_js(&mut self, js: String) {
        self.generated_js = Some(js);
    }

    /// A recorded gen line's position in the FINAL js (after the prelude shift).
    fn eff_gen_line(&self, l: u32) -> u32 {
        if l >= self.gen_line_shift_floor {
            l + self.gen_line_shift
        } else {
            l
        }
    }

    pub fn add_mapping(&mut self, gen_line: u32, gen_col: u32, orig_line: u32, orig_col: u32) {
        self.mappings.push(Mapping {
            gen_line,
            gen_col,
            orig_line,
            orig_col,
        });
    }

    /// Generate the source map JSON string.
    pub fn to_json(&self) -> String {
        let (mappings_str, names) = self.encode_mappings();
        let mut map = serde_json::json!({
            "version": 3,
            "file": self.output_file,
            "sourceRoot": "",
            "sources": [self.source_file],
            "names": names,
            "mappings": mappings_str,
        });
        // DX-2: inline the original source so the debugger needs no `.ps` fetch.
        if let Some(content) = &self.source_content {
            map["sourcesContent"] = serde_json::json!([content]);
        }
        map.to_string()
    }

    /// Encode the mappings VLQ string and, alongside it, the `names` array
    /// (DX-1). Generated lines are shifted by the prelude offset (DX-3).
    fn encode_mappings(&self) -> (String, Vec<String>) {
        if self.mappings.is_empty() {
            return (String::new(), Vec::new());
        }

        // Effective (post-shift) generated line for each mapping.
        let eff = |m: &Mapping| self.eff_gen_line(m.gen_line);

        let mut indices: Vec<usize> = (0..self.mappings.len()).collect();
        indices.sort_by(|&a, &b| {
            eff(&self.mappings[a])
                .cmp(&eff(&self.mappings[b]))
                .then(self.mappings[a].gen_col.cmp(&self.mappings[b].gen_col))
        });

        let mut result = String::new();
        let mut names: Vec<String> = Vec::new();
        let mut name_index: std::collections::HashMap<String, usize> =
            std::collections::HashMap::new();
        let mut prev_gen_line: u32 = 0;
        let mut prev_gen_col: i64 = 0;
        let mut prev_orig_line: i64 = 0;
        let mut prev_orig_col: i64 = 0;
        let mut prev_name: i64 = 0;
        let mut first_in_line = true;

        for &idx in &indices {
            let mapping = &self.mappings[idx];
            let g_line = eff(mapping);
            // Emit semicolons for skipped lines
            while prev_gen_line < g_line {
                result.push(';');
                prev_gen_line += 1;
                prev_gen_col = 0;
                first_in_line = true;
            }

            if !first_in_line {
                result.push(',');
            }

            // Field 1: generated column (relative to previous in same line)
            let gen_col_delta = mapping.gen_col as i64 - prev_gen_col;
            vlq_encode(&mut result, gen_col_delta);
            // Field 2: source index (always 0, relative)
            vlq_encode(&mut result, 0);
            // Field 3: original line (relative)
            vlq_encode(&mut result, mapping.orig_line as i64 - prev_orig_line);
            // Field 4: original column (relative)
            vlq_encode(&mut result, mapping.orig_col as i64 - prev_orig_col);

            // Field 5 (DX-1): original name — ONLY when the generated token is a
            // genuinely preserved identifier (generated token == original token),
            // never for helper-call sites (which would mislabel the frame).
            if let Some(nm) = self.preserved_name(mapping, g_line) {
                let ni = *name_index.entry(nm.clone()).or_insert_with(|| {
                    names.push(nm.clone());
                    names.len() - 1
                });
                vlq_encode(&mut result, ni as i64 - prev_name);
                prev_name = ni as i64;
            }

            prev_gen_col = mapping.gen_col as i64;
            prev_orig_line = mapping.orig_line as i64;
            prev_orig_col = mapping.orig_col as i64;
            first_in_line = false;
        }

        (result, names)
    }

    /// DX-1: the original identifier name for a mapping, but only when the
    /// generated token at the mapped position is the SAME identifier — i.e. a
    /// preserved binding, not a helper call. Returns `None` otherwise (no name
    /// emitted), so `names` never mislabels a generated helper as an original.
    fn preserved_name(&self, m: &Mapping, eff_gen_line: u32) -> Option<String> {
        let src = self.source_content.as_deref()?;
        let js = self.generated_js.as_deref()?;
        let orig = ident_at(src, m.orig_line, m.orig_col)?;
        let gen = ident_at(js, eff_gen_line, m.gen_col)?;
        if orig == gen {
            Some(orig)
        } else {
            None
        }
    }
}

/// The identifier token starting exactly at `(line, col)` (0-based) in `text`,
/// or `None` if that position is not the start of an identifier, or is a Python
/// keyword / JS-reserved word (emitting a name there would mislead). Used by the
/// DX-1 `names` population, keyed on a preserved-identifier check.
fn ident_at(text: &str, line: u32, col: u32) -> Option<String> {
    // locate the byte offset of (line, col)
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
        return None;
    }
    let id = &rest[..end];
    const RESERVED: &[&str] = &[
        // Python keywords
        "def", "return", "if", "else", "elif", "for", "while", "in", "and", "or", "not", "None",
        "True", "False", "class", "import", "from", "as", "with", "try", "except", "finally",
        "raise", "lambda", "pass", "break", "continue", "global", "nonlocal", "yield", "async",
        "await", "del", "assert", "is",
        // JS reserved words that appear in generated output
        "let", "const", "var", "function", "of", "export", "new", "typeof",
    ];
    if RESERVED.contains(&id) {
        return None;
    }
    Some(id.to_string())
}

/// Compute (line, col) from a byte offset into source text.
/// Both are 0-based.
pub fn byte_offset_to_line_col(source: &str, offset: usize) -> (u32, u32) {
    let mut line = 0u32;
    let mut col = 0u32;
    for (i, ch) in source.char_indices() {
        if i >= offset {
            break;
        }
        if ch == '\n' {
            line += 1;
            col = 0;
        } else {
            col += 1;
        }
    }
    (line, col)
}

/// Encode a signed integer as a Base64 VLQ.
fn vlq_encode(out: &mut String, value: i64) {
    const BASE64_CHARS: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    const VLQ_BASE: i64 = 32;
    const VLQ_BASE_MASK: i64 = VLQ_BASE - 1;
    const VLQ_CONTINUATION: i64 = VLQ_BASE;

    // Convert to VLQ signed representation
    let mut vlq = if value < 0 {
        ((-value) << 1) + 1
    } else {
        value << 1
    };

    loop {
        let mut digit = vlq & VLQ_BASE_MASK;
        vlq >>= 5;
        if vlq > 0 {
            digit |= VLQ_CONTINUATION;
        }
        out.push(BASE64_CHARS[digit as usize] as char);
        if vlq == 0 {
            break;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_vlq_encode() {
        let mut s = String::new();
        vlq_encode(&mut s, 0);
        assert_eq!(s, "A");

        s.clear();
        vlq_encode(&mut s, 1);
        assert_eq!(s, "C");

        s.clear();
        vlq_encode(&mut s, -1);
        assert_eq!(s, "D");

        s.clear();
        vlq_encode(&mut s, 16);
        assert_eq!(s, "gB");
    }

    #[test]
    fn test_byte_offset_to_line_col() {
        let src = "hello\nworld\nfoo";
        assert_eq!(byte_offset_to_line_col(src, 0), (0, 0));
        assert_eq!(byte_offset_to_line_col(src, 3), (0, 3));
        assert_eq!(byte_offset_to_line_col(src, 6), (1, 0));
        assert_eq!(byte_offset_to_line_col(src, 8), (1, 2));
        assert_eq!(byte_offset_to_line_col(src, 12), (2, 0));
    }

    #[test]
    fn test_source_map_json() {
        let mut builder = SourceMapBuilder::new("test.ps", "test.js");
        builder.add_mapping(0, 0, 0, 0);
        builder.add_mapping(1, 0, 1, 0);
        let json = builder.to_json();
        assert!(json.contains("\"version\":3"));
        assert!(json.contains("\"sources\":[\"test.ps\"]"));
        assert!(json.contains("\"file\":\"test.js\""));
    }
}
