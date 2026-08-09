//! `pyths_expand` — Phase 2 compressed-source expander.
//!
//! Takes `.psc` (compressed PythScribe) and produces canonical `.ps`
//! PythScribe via line-aware textual substitution. The expander is
//! deliberately small and parser-free: every Phase 1 lexer/parser/type
//! check applies unchanged to the expanded output.
//!
//! Every tier — including Tier A — classifies the source with the ONE
//! shared zone scanner in [`zones`]: rewrites happen in code zones only,
//! and strings, docstrings and comments are emitted byte-for-byte.
//!
//! Tier A substitutions implemented:
//!   * **Preset imports** — `R*`, `A*`, `T*`, `T+`, `D*`, `W*`, `R+`
//!     occupy an entire line **of code** and expand to a canonical import
//!     line. A marker alone on a line inside a `"""…"""` docstring is
//!     docstring text and is left alone.
//!   * **Decorator aliases** — `@c`, `@d`, `@v`, `@h`, `@k` at the
//!     decorator slot expand to `@component`, `@dataclass`, etc.
//!     Both standalone (`@c`) and call forms (`@d(coerce=True)`) are
//!     supported.
//!
//! Tier B substitutions implemented:
//!   * **Kwarg-position aliases** — `st=`, `cn=`, `cl=`, `oc=`, `oh=`,
//!     `os=`, `oa=`, `ph=`, `dis=` expand to their canonical kwarg
//!     names (`style=`, `class_name=`, …) but **only** when they
//!     appear in function-call argument position. See [`kwargs`] for
//!     the state-machine that enforces this.

pub mod decorators;
pub mod hooks;
pub mod idioms;
/// Bounded model-checking harnesses for the shared zone classifier. Compiled
/// **only** under `cargo kani` (`--cfg kani`); invisible to `cargo build` and
/// `cargo test`, and the crate takes no dependency on Kani. See `KANI.md`.
#[cfg(kani)]
mod kani_proofs;
pub mod kwargs;
pub mod presets;
pub mod strings;
pub mod zones;

/// Expand a `.psc` source string into canonical PythScribe.
///
/// Line endings are preserved exactly. The expander runs in two
/// passes: (1) per-line preset and decorator-alias expansion, then
/// (2) source-wide kwarg-position substitution. Anything outside a
/// substitution site passes through byte-for-byte.
///
/// Equivalent to [`expand_with_dict`] with an empty user dictionary.
pub fn expand(source: &str) -> String {
    expand_with_dict(source, &std::collections::HashMap::new())
}

/// Expand a `.psc` source string into canonical PythScribe, with a
/// caller-supplied dictionary of `$NAME` aliases layered on top of the
/// bundled table.
///
/// User entries override bundled aliases of the same name. Pass an empty
/// `HashMap` to get the zero-config behavior (identical to [`expand`]).
///
/// Equivalent to [`expand_with_config`] with an empty idioms map.
pub fn expand_with_dict(
    source: &str,
    user_dict: &std::collections::HashMap<String, String>,
) -> String {
    expand_with_config(source, user_dict, &std::collections::HashMap::new())
}

/// Expand a `.psc` source string into canonical PythScribe, with both a
/// caller-supplied `$NAME` dictionary **and** a `%NAME` idiom map.
///
/// ## Pipeline order (rationale in each comment)
///
/// 1. **Tier E — idiom substitution** (`%NAME` → canonical code fragment).
///    Runs *first* so that any aliases a fragment happens to contain
///    (kwarg shorthands, `$` string aliases, hook aliases) flow through the
///    subsequent passes unchanged.  Running idioms last would silently
///    suppress those inner substitutions.
/// 2. **Tier A — per-line preset and decorator expansion**.
/// 3. **Tier B — kwarg-position aliases**.
/// 4. **Phase 2.5 — hook-call shorthand**.
/// 5. **Phase 2.10 — domain-dictionary `$NAME` string aliases** (with user
///    overrides).
pub fn expand_with_config(
    source: &str,
    user_dict: &std::collections::HashMap<String, String>,
    idiom_map: &std::collections::HashMap<String, String>,
) -> String {
    // Step 1 — Tier E: idiom (%NAME) substitution — must be first.
    let idiom_expanded = idioms::substitute_with_map(source, idiom_map);
    // Step 2 — Tier A: per-line preset and decorator expansion, ZONE-AWARE.
    //
    // Tier A is the one tier that rewrites whole lines rather than scanning
    // characters, so it asks the shared classifier (`zones::line_start_states`)
    // which lines begin in a code zone. A line that begins inside an
    // unterminated string or docstring is emitted VERBATIM — a `R*` or `@c`
    // sitting alone on a line inside a `"""…"""` block is docstring text, not
    // a marker. (Comments need no special case: a `#` never survives its
    // newline, and a line whose trimmed body starts with `#` is neither a
    // preset key nor a decorator alias.)
    let line_states = zones::line_start_states(&idiom_expanded);
    let mut line_expanded = String::with_capacity(idiom_expanded.len() + idiom_expanded.len() / 4);
    for (idx, line) in split_lines(&idiom_expanded).enumerate() {
        if line_states.get(idx).copied().flatten().is_some() {
            // Protected zone — pass the line through byte-for-byte.
            line_expanded.push_str(line);
        } else {
            line_expanded.push_str(&expand_line(line));
        }
    }
    // Step 3 — Tier B: kwarg-position aliases.
    let kwarg_expanded = kwargs::substitute(&line_expanded);
    // Step 4 — Phase 2.5: hook-call shorthand.
    let hooks_expanded = hooks::substitute(&kwarg_expanded);
    // Step 5 — Phase 2.10: domain-dictionary string-literal aliases (with user overrides).
    strings::substitute_with_dict(&hooks_expanded, user_dict)
}

/// The fixed tier order of [`expand_with_config`], as data. The Lean
/// model (`verification/PythExpandVerify.lean`, `expand_order`) proves
/// determinism FOR THIS ORDER, and `verification/model-manifest.txt`
/// pins it together with the dictionary domain (checked by
/// `tests/gates.rs`). If you reorder the pipeline, update this
/// constant, the manifest, and the Lean model together.
pub const TIER_ORDER: &[&str] = &["E", "A", "B", "hooks", "Dict"];

/// Returns `true` if `line` (possibly with leading whitespace and
/// trailing newline) is a recognised preset marker.
pub fn is_preset_line(line: &str) -> bool {
    let trimmed = line.trim_end_matches(['\r', '\n']).trim();
    presets::lookup(trimmed).is_some()
}

/// Rewrite one line — called ONLY for lines the shared classifier says begin
/// in a code zone (see [`expand_with_config`]).
fn expand_line(line: &str) -> String {
    let (body, newline) = split_newline(line);
    let trim_start = body.trim_start();
    let indent_len = body.len() - trim_start.len();
    let indent = &body[..indent_len];
    let trimmed = trim_start.trim_end();

    if trimmed.is_empty() {
        return line.to_string();
    }

    // A preset marker occupies the whole (trimmed) line. The marker and any
    // trailing whitespace are pure code — a preset key contains no quote and
    // no `#` — so dropping the trailing whitespace is zone-safe.
    if let Some(expansion) = presets::lookup(trimmed) {
        return format!("{}{}{}", indent, expansion, newline);
    }

    // A decorator alias is matched against the body with its LEADING
    // whitespace removed but its TRAILING bytes intact: everything after the
    // alias — call-args, comments, trailing whitespace — is copied
    // byte-for-byte. (Trimming the tail here would be observable: on a line
    // whose args open an unterminated string, the trailing spaces are inside
    // that string, i.e. protected.)
    if let Some(expanded) = expand_decorator_line(trim_start) {
        return format!("{}{}{}", indent, expanded, newline);
    }

    line.to_string()
}

/// `@c` → `@component`, `@d(coerce=True)` → `@dataclass(coerce=True)`.
///
/// `body` is the line minus its newline and minus its leading indentation.
/// Everything after the alias run is returned verbatim.
fn expand_decorator_line(body: &str) -> Option<String> {
    if !body.starts_with('@') {
        return None;
    }

    // Find where the alias ends — alphanumeric run after the `@`.
    let after_at = &body[1..];
    let alias_len = after_at
        .find(|c: char| !c.is_ascii_alphanumeric())
        .unwrap_or(after_at.len());
    if alias_len == 0 {
        return None;
    }
    let alias_with_at = &body[..=alias_len];
    let rest = &body[alias_with_at.len()..];

    let canonical = decorators::lookup(alias_with_at)?;
    Some(format!("{}{}", canonical, rest))
}

fn split_newline(line: &str) -> (&str, &str) {
    if let Some(stripped) = line.strip_suffix("\r\n") {
        (stripped, "\r\n")
    } else if let Some(stripped) = line.strip_suffix('\n') {
        (stripped, "\n")
    } else {
        (line, "")
    }
}

fn split_lines(source: &str) -> impl Iterator<Item = &str> {
    let mut remaining = source;
    std::iter::from_fn(move || {
        if remaining.is_empty() {
            return None;
        }
        let end = remaining
            .find('\n')
            .map(|i| i + 1)
            .unwrap_or(remaining.len());
        let (line, rest) = remaining.split_at(end);
        remaining = rest;
        Some(line)
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_input_passes_through() {
        assert_eq!(expand(""), "");
    }

    #[test]
    fn pass_through_when_no_markers() {
        let src = "x = 1\ny = 2\n";
        assert_eq!(expand(src), src);
    }

    #[test]
    fn preset_react_expansion() {
        let out = expand("R*\n");
        assert_eq!(
            out,
            "from pyths.react import component, use_state, use_effect, use_callback, use_memo\n"
        );
    }

    #[test]
    fn preset_asyncio_expansion() {
        assert_eq!(expand("A*\n"), "from pyths.asyncio import gather, sleep\n");
    }

    #[test]
    fn preset_dataclass_with_field() {
        assert_eq!(expand("T+\n"), "from dataclasses import dataclass, Field\n");
    }

    #[test]
    fn preset_dataclass_plain() {
        assert_eq!(expand("T*\n"), "from dataclasses import dataclass\n");
    }

    #[test]
    fn preset_dom() {
        assert_eq!(
            expand("D*\n"),
            "from pyths.dom import query, query_all, get_element_by_id, set_text, get_text, add_event_listener\n"
        );
    }

    #[test]
    fn preset_web() {
        assert_eq!(expand("W*\n"), "from pyths.web import handler, Response\n");
    }

    #[test]
    fn decorator_component_alias() {
        assert_eq!(expand("@c\n"), "@component\n");
    }

    #[test]
    fn decorator_dataclass_alias() {
        assert_eq!(expand("@d\n"), "@dataclass\n");
    }

    #[test]
    fn decorator_alias_with_call_args() {
        assert_eq!(expand("@d(coerce=True)\n"), "@dataclass(coerce=True)\n");
    }

    #[test]
    fn decorator_validator_alias() {
        assert_eq!(expand("@v\n"), "@validator\n");
    }

    #[test]
    fn decorator_handler_alias() {
        assert_eq!(expand("@h\n"), "@handler\n");
    }

    #[test]
    fn decorator_alias_in_indented_class_body() {
        let src = "class Outer:\n    @c\n    def inner(self): ...\n";
        let want = "class Outer:\n    @component\n    def inner(self): ...\n";
        assert_eq!(expand(src), want);
    }

    #[test]
    fn unknown_decorator_passes_through() {
        // `@compose` is not in the alias table and must NOT be confused
        // with `@c`.
        assert_eq!(expand("@compose\n"), "@compose\n");
    }

    #[test]
    fn unknown_one_letter_decorator_passes_through() {
        // `@x` is not in the alias table.
        assert_eq!(expand("@x\n"), "@x\n");
    }

    #[test]
    fn full_decorator_name_passes_through() {
        // `@component` already canonical — leave alone.
        assert_eq!(expand("@component\n"), "@component\n");
    }

    #[test]
    fn preset_marker_with_indentation_expands() {
        // Conservative: preset markers usually appear at column 0, but
        // we accept indented forms too.
        assert_eq!(
            expand("    R*\n"),
            "    from pyths.react import component, use_state, use_effect, use_callback, use_memo\n"
        );
    }

    #[test]
    fn preset_marker_inside_expression_does_not_expand() {
        // `R*` is only a preset if it occupies the entire line. Inside
        // an expression it must pass through (it would be a name-times
        // multiply anyway — unlikely but safe).
        let src = "x = R*y\n";
        assert_eq!(expand(src), src);
    }

    #[test]
    fn crlf_line_endings_preserved() {
        assert_eq!(
            expand("R*\r\n"),
            "from pyths.react import component, use_state, use_effect, use_callback, use_memo\r\n"
        );
    }

    #[test]
    fn last_line_without_newline_still_expands() {
        assert_eq!(
            expand("R*"),
            "from pyths.react import component, use_state, use_effect, use_callback, use_memo"
        );
    }

    #[test]
    fn blank_lines_preserved() {
        let src = "R*\n\n@c\ndef x(): pass\n";
        let want = "from pyths.react import component, use_state, use_effect, use_callback, use_memo\n\n@component\ndef x(): pass\n";
        assert_eq!(expand(src), want);
    }

    #[test]
    fn mixed_program_expansion() {
        let src = "\
R*
T*

@d
class Order:
    id: int

@c
def App():
    s, ss = use_state(0)
    return div()(s)
";
        let want = "\
from pyths.react import component, use_state, use_effect, use_callback, use_memo
from dataclasses import dataclass

@dataclass
class Order:
    id: int

@component
def App():
    s, ss = use_state(0)
    return div()(s)
";
        assert_eq!(expand(src), want);
    }

    // -----------------------------------------------------------------
    // Tier A zone-safety (the shared classifier gates the line rewrite)
    // -----------------------------------------------------------------

    #[test]
    fn tier_a_preset_inside_triple_double_quote_is_verbatim() {
        let src = "doc = \"\"\"\nR*\n\"\"\"\ny = 1\n";
        assert_eq!(expand(src), src);
    }

    #[test]
    fn tier_a_decorator_inside_triple_single_quote_is_verbatim() {
        let src = "d = '''\n@c\n'''\n";
        assert_eq!(expand(src), src);
    }

    #[test]
    fn tier_a_indented_marker_inside_docstring_is_verbatim() {
        let src = "doc = \"\"\"\n    R+  \n@d\n\"\"\"\n";
        assert_eq!(expand(src), src);
    }

    #[test]
    fn tier_a_marker_in_code_after_a_closed_docstring_still_expands() {
        let src = "doc = \"\"\"\nR*\n\"\"\"\nR*\n@c\n";
        let want = "doc = \"\"\"\nR*\n\"\"\"\n\
                    from pyths.react import component, use_state, use_effect, use_callback, use_memo\n\
                    @component\n";
        assert_eq!(expand(src), want);
    }

    #[test]
    fn tier_a_marker_after_a_closed_single_line_string_still_expands() {
        let src = "s = \"R*\"\nR*\n";
        let want = "s = \"R*\"\nfrom pyths.react import component, use_state, use_effect, use_callback, use_memo\n";
        assert_eq!(expand(src), want);
    }

    #[test]
    fn tier_a_marker_inside_unterminated_single_quoted_string_is_verbatim() {
        // The string never closes, so every following line is protected.
        let src = "s = \"oops\nR*\n";
        assert_eq!(expand(src), src);
    }

    #[test]
    fn tier_a_marker_on_a_line_that_merely_contains_a_quote_expands() {
        // The quote closes on the same line — the next line is code.
        let src = "s = \"a\" + \"b\"\n@c\ndef App(): ...\n";
        let want = "s = \"a\" + \"b\"\n@component\ndef App(): ...\n";
        assert_eq!(expand(src), want);
    }

    #[test]
    fn tier_a_marker_in_comment_line_is_verbatim() {
        let src = "# R*\n# @c\n";
        assert_eq!(expand(src), src);
    }

    #[test]
    fn tier_a_adjacent_docstrings_track_open_and_close() {
        // First docstring closes, code between, second docstring opens.
        let src = "a = \"\"\"x\"\"\"\nR*\nb = \"\"\"\n@c\n\"\"\"\n";
        let want = "a = \"\"\"x\"\"\"\n\
                    from pyths.react import component, use_state, use_effect, use_callback, use_memo\n\
                    b = \"\"\"\n@c\n\"\"\"\n";
        assert_eq!(expand(src), want);
    }

    #[test]
    fn tier_a_escaped_quote_keeps_following_lines_protected() {
        // `\"` does not close the string, so `R*` on the next line is inside it.
        let src = "s = \"a\\\"b\nR*\n";
        assert_eq!(expand(src), src);
    }

    #[test]
    fn tier_a_decorator_preserves_the_rest_of_the_line_verbatim() {
        // Trailing whitespace after a decorator alias survives (it is bytes of
        // the line, and on a line with an open string it would be protected).
        assert_eq!(expand("@c  \n"), "@component  \n");
        assert_eq!(
            expand("@h(path=\"/api\")  # route\n"),
            "@handler(path=\"/api\")  # route\n"
        );
    }

    #[test]
    fn tier_a_docstring_content_survives_the_whole_pipeline() {
        // Byte-exact idempotence on a docstring holding every Tier A marker.
        let src = "\"\"\"\nR.\nR*\nR+\nA*\nT*\nT+\nD*\nW*\n@c\n@d\n@v\n@h\n@k\n\"\"\"\n";
        assert_eq!(expand(src), src);
    }

    #[test]
    fn is_preset_line_recognises_markers() {
        assert!(is_preset_line("R*"));
        assert!(is_preset_line("R*\n"));
        assert!(is_preset_line("  T+  \n"));
        assert!(!is_preset_line("R**"));
        assert!(!is_preset_line("X*"));
        assert!(!is_preset_line("import foo"));
    }

    #[test]
    fn minimal_react_preset_expands() {
        assert!(is_preset_line("R."));
        assert!(is_preset_line("R.\n"));
        assert!(!is_preset_line("R.."));
        assert_eq!(
            expand("R.\n"),
            "from pyths.react import component, use_state\n"
        );
    }

    #[test]
    fn idempotent_on_already_expanded() {
        // Expanding canonical PythScribe is a no-op.
        let canonical = "\
from pyths.react import component, use_state, use_effect, use_callback, use_memo
from dataclasses import dataclass

@dataclass
class X:
    a: int

@component
def App():
    return None
";
        assert_eq!(expand(canonical), canonical);
    }

    // -----------------------------------------------------------------
    // Tier B: kwarg-position substitution
    // -----------------------------------------------------------------

    #[test]
    fn kwarg_style_alias_substitutes() {
        assert_eq!(expand("div(st=1)\n"), "div(style=1)\n");
    }

    #[test]
    fn kwarg_on_click_alias_substitutes() {
        assert_eq!(expand("button(oc=handler)\n"), "button(on_click=handler)\n");
    }

    #[test]
    fn kwarg_on_change_alias_substitutes() {
        assert_eq!(expand("input(oh=handle)\n"), "input(on_change=handle)\n");
    }

    #[test]
    fn kwarg_classname_camel_alias() {
        assert_eq!(expand("div(cl=\"foo\")\n"), "div(className=\"foo\")\n");
    }

    #[test]
    fn kwarg_class_name_snake_alias() {
        assert_eq!(expand("div(cn=\"foo\")\n"), "div(class_name=\"foo\")\n");
    }

    #[test]
    fn kwarg_multiple_aliases_one_call() {
        assert_eq!(
            expand("button(st={}, oc=f, ph=\"hi\")\n"),
            "button(style={}, on_click=f, placeholder=\"hi\")\n"
        );
    }

    #[test]
    fn kwarg_disabled_alias() {
        assert_eq!(expand("button(dis=True)\n"), "button(disabled=True)\n");
    }

    #[test]
    fn kwarg_alias_on_continuation_line() {
        // The alias may appear on a continuation line; the
        // `at_arg_start` flag must persist through whitespace and
        // newlines.
        let src = "\
button(
    oc=callback,
    st={\"padding\": \"4px\"},
)
";
        let want = "\
button(
    on_click=callback,
    style={\"padding\": \"4px\"},
)
";
        assert_eq!(expand(src), want);
    }

    #[test]
    fn kwarg_alias_inside_string_not_substituted() {
        // `st=` appears inside a string literal — must NOT substitute.
        let src = "log(\"st=value goes here\")\n";
        assert_eq!(expand(src), src);
    }

    #[test]
    fn kwarg_alias_inside_comment_not_substituted() {
        let src = "div() # st=foo\n";
        assert_eq!(expand(src), src);
    }

    #[test]
    fn kwarg_alias_inside_f_string_not_substituted() {
        // F-strings are treated as opaque — kwargs inside `{…}`
        // interpolation are left alone.
        let src = "log(f\"st={x}\")\n";
        assert_eq!(expand(src), src);
    }

    #[test]
    fn kwarg_alias_in_statement_not_substituted() {
        // `st = ...` at statement level is NOT a kwarg — it must
        // pass through.
        let src = "st = 42\n";
        assert_eq!(expand(src), src);
    }

    #[test]
    fn kwarg_alias_in_comparison_not_substituted() {
        // `st == something` is a comparison, not a kwarg.
        let src = "if st == 1:\n    pass\n";
        assert_eq!(expand(src), src);
    }

    #[test]
    fn kwarg_alias_after_double_equals_not_substituted() {
        let src = "func(x == st)\n";
        assert_eq!(expand(src), src);
    }

    #[test]
    fn kwarg_alias_as_positional_name_not_substituted() {
        // `func(st)` — `st` is positional, NOT followed by `=`.
        let src = "func(st)\n";
        assert_eq!(expand(src), src);
    }

    #[test]
    fn kwarg_alias_in_dict_literal_not_substituted() {
        // Dicts use `:` not `=` between key and value, so the
        // followed-by-`=` check correctly fails.
        let src = "{st: 1, oc: 2}\n";
        assert_eq!(expand(src), src);
    }

    #[test]
    fn kwarg_alias_in_default_arg_of_def_substitutes() {
        // `def foo(st=1):` — after `(`, `st=1` IS in arg position.
        // This is technically a function-signature default rather
        // than a call kwarg, but both forms use the same syntax
        // and produce the same expanded output. The semantic
        // distinction is preserved (parameter named `style` rather
        // than `st`) — which is what an LLM-emitted .psc would
        // expect.
        assert_eq!(
            expand("def foo(st=1):\n    pass\n"),
            "def foo(style=1):\n    pass\n"
        );
    }

    #[test]
    fn kwarg_unknown_alias_passes_through() {
        // `xy=` is not in the alias table — leave untouched.
        let src = "func(xy=1)\n";
        assert_eq!(expand(src), src);
    }

    #[test]
    fn kwarg_substitution_skips_escaped_quote_in_string() {
        // `"\"st=\""` — escaped quote inside string must not
        // prematurely end string tracking.
        let src = "log(\"\\\"st=\\\" inside\")\n";
        assert_eq!(expand(src), src);
    }

    #[test]
    fn kwarg_combined_with_tier_a() {
        // Tier A presets/decorators + Tier B kwargs in one source.
        let src = "\
R*

@c
def App():
    return div(st={\"color\": \"red\"}, oc=handle)
";
        let want = "\
from pyths.react import component, use_state, use_effect, use_callback, use_memo

@component
def App():
    return div(style={\"color\": \"red\"}, on_click=handle)
";
        assert_eq!(expand(src), want);
    }

    #[test]
    fn kwarg_substitution_preserves_multibyte_utf8_in_strings() {
        // Regression: string-literal contents with non-ASCII UTF-8
        // bytes (e.g., the "←" arrow character, em-dash, accented
        // letters) must pass through byte-identical. An earlier
        // implementation that did `byte as char` per-byte would
        // corrupt the encoding on each pass.
        let src = "Button(label=\"← Back\", oc=on_back)\n";
        let want = "Button(label=\"← Back\", on_click=on_back)\n";
        assert_eq!(expand(src), want);
        // Idempotent under repeated expansion.
        assert_eq!(expand(&expand(src)), want);
    }

    #[test]
    fn kwarg_substitution_preserves_emoji_in_strings() {
        let src = "div(st={\"icon\": \"🎉\"}, oc=f)\n";
        let want = "div(style={\"icon\": \"🎉\"}, on_click=f)\n";
        assert_eq!(expand(src), want);
        assert_eq!(expand(&expand(src)), want);
    }

    #[test]
    fn kwarg_substitution_preserves_multibyte_after_backslash() {
        // Backslash followed by a multi-byte UTF-8 char inside a
        // string must emit both bytes correctly.
        let src = "log(\"\\→ next\")\n";
        assert_eq!(expand(src), src);
        assert_eq!(expand(&expand(src)), src);
    }

    // -----------------------------------------------------------------
    // Phase 2.10: Domain dictionary ($-prefix string aliases)
    // -----------------------------------------------------------------

    #[test]
    fn dict_color_alias_expands() {
        assert_eq!(expand("color = $c1\n"), "color = \"#9ca3af\"\n");
    }

    #[test]
    fn dict_color_in_kwarg_position() {
        assert_eq!(expand("div(color=$c4)\n"), "div(color=\"#3b82f6\")\n");
    }

    #[test]
    fn dict_color_in_dict_value() {
        assert_eq!(
            expand("style = {\"color\": $c3, \"background\": $c2}\n"),
            "style = {\"color\": \"#6b7280\", \"background\": \"#ffffff\"}\n"
        );
    }

    #[test]
    fn dict_px_alias_expands() {
        assert_eq!(expand("padding = $p4\n"), "padding = \"16px\"\n");
    }

    #[test]
    fn dict_alphanumeric_alias() {
        // `$cA` mixes a letter and a hex-like ID.
        assert_eq!(expand("bg = $cA\n"), "bg = \"#fffbeb\"\n");
    }

    #[test]
    fn dict_unknown_alias_passes_through() {
        // `$xyz` is not registered — `$` and the identifier emit
        // unchanged so the downstream lexer produces a clear error
        // on the stray `$`.
        let src = "x = $xyz\n";
        assert_eq!(expand(src), src);
    }

    #[test]
    fn dict_bare_dollar_passes_through() {
        let src = "price = $\n";
        assert_eq!(expand(src), src);
    }

    #[test]
    fn dict_alias_inside_string_not_substituted() {
        let src = "label = \"$c1 cost\"\n";
        assert_eq!(expand(src), src);
    }

    #[test]
    fn dict_alias_inside_comment_not_substituted() {
        let src = "x = 1  # see $c1\n";
        assert_eq!(expand(src), src);
    }

    #[test]
    fn dict_alias_inside_fstring_not_substituted() {
        // F-strings are treated as opaque (same convention as
        // kwargs.rs / hooks.rs).
        let src = "log(f\"hex={$c1}\")\n";
        assert_eq!(expand(src), src);
    }

    #[test]
    fn dict_multiple_aliases_one_line() {
        assert_eq!(
            expand("col = $c1; bg = $c2\n"),
            "col = \"#9ca3af\"; bg = \"#ffffff\"\n"
        );
    }

    #[test]
    fn dict_alias_preserves_multibyte_neighbours() {
        // A multi-byte char near the alias must pass through verbatim.
        let src = "x = $c1  # ← arrow\n";
        let want = "x = \"#9ca3af\"  # ← arrow\n";
        assert_eq!(expand(src), want);
    }

    #[test]
    fn dict_alias_combined_with_kwarg_alias_in_call() {
        // The kwarg pass rewrites `st=` to `style=`; the dict pass then
        // resolves the `$c4` sitting in that kwarg's value position.
        let src = "return div(st=$c4)(label)\n";
        let want = "return div(style=\"#3b82f6\")(label)\n";
        assert_eq!(expand(src), want);
    }

    #[test]
    fn dict_alias_combined_with_kwarg_substitution() {
        // The kwarg pass (`cn=` → `class_name=`) runs before the
        // dict pass. End-to-end: `cn=$c4` becomes
        // `class_name="#3b82f6"`.
        assert_eq!(expand("div(cn=$c4)\n"), "div(class_name=\"#3b82f6\")\n");
    }

    #[test]
    fn dict_idempotent_on_canonical_input() {
        let src = "color = \"#9ca3af\"\n";
        assert_eq!(expand(src), src);
    }

    #[test]
    fn dict_long_string_alias() {
        assert_eq!(
            expand("font_family = $ff\n"),
            "font_family = \"system-ui, sans-serif\"\n"
        );
    }

    // -----------------------------------------------------------------
    // Phase 2.5: hook-call shorthand
    // -----------------------------------------------------------------

    #[test]
    fn hook_use_state_substitutes() {
        assert_eq!(expand("c, sc = us(0)\n"), "c, sc = use_state(0)\n");
    }

    #[test]
    fn hook_use_effect_substitutes() {
        assert_eq!(
            expand("ue(lambda: load(), [])\n"),
            "use_effect(lambda: load(), [])\n"
        );
    }

    #[test]
    fn hook_use_memo_substitutes() {
        assert_eq!(
            expand("x = um(compute, [a])\n"),
            "x = use_memo(compute, [a])\n"
        );
    }

    #[test]
    fn hook_use_callback_substitutes() {
        assert_eq!(
            expand("h = uc(handler, [d])\n"),
            "h = use_callback(handler, [d])\n"
        );
    }

    #[test]
    fn hook_use_ref_substitutes() {
        assert_eq!(expand("r = ur(None)\n"), "r = use_ref(None)\n");
    }

    #[test]
    fn hook_use_context_substitutes() {
        assert_eq!(expand("v = ux(Ctx)\n"), "v = use_context(Ctx)\n");
    }

    #[test]
    fn hook_alias_not_substituted_when_used_as_variable() {
        // `us` without `(` is a variable reference — must pass through.
        let src = "x = us + 1\n";
        assert_eq!(expand(src), src);
    }

    #[test]
    fn hook_alias_not_substituted_inside_string() {
        let src = "log(\"us(0)\")\n";
        assert_eq!(expand(src), src);
    }

    #[test]
    fn hook_alias_not_substituted_inside_comment() {
        let src = "x = 1  # us(0)\n";
        assert_eq!(expand(src), src);
    }

    #[test]
    fn hook_alias_not_substituted_as_substring_of_identifier() {
        // `bus(` starts with `b` then has `us(`. The match must NOT
        // fire — `bus` is a single identifier.
        let src = "bus(3)\n";
        assert_eq!(expand(src), src);
    }

    #[test]
    fn hook_alias_not_substituted_when_preceded_by_dot() {
        // `obj.us(0)` — `us` is a method call on `obj`, not a hook.
        let src = "obj.us(0)\n";
        assert_eq!(expand(src), src);
    }

    #[test]
    fn hook_alias_not_substituted_when_attribute_access() {
        // `state.use_state` should pass through — not a call.
        let src = "x = state.us\n";
        assert_eq!(expand(src), src);
    }

    #[test]
    fn hook_substitution_combined_with_tier_a_and_b() {
        let src = "\
R*

@c
def App():
    c, sc = us(0)
    ue(lambda: print(c), [c])
    return div(st={}, oc=lambda: sc(c+1))
";
        let want = "\
from pyths.react import component, use_state, use_effect, use_callback, use_memo

@component
def App():
    c, sc = use_state(0)
    use_effect(lambda: print(c), [c])
    return div(style={}, on_click=lambda: sc(c+1))
";
        assert_eq!(expand(src), want);
    }

    #[test]
    fn hook_substitution_idempotent_on_canonical() {
        let src = "x = use_state(0)\n";
        assert_eq!(expand(src), src);
    }

    #[test]
    fn hook_substitution_multiple_aliases_one_line() {
        assert_eq!(
            expand("a = us(1); b = ue(f, [])\n"),
            "a = use_state(1); b = use_effect(f, [])\n"
        );
    }

    #[test]
    fn hook_substitution_preserves_multibyte_utf8() {
        let src = "log(\"← us(0)\")\n";
        assert_eq!(expand(src), src);
    }

    #[test]
    fn kwarg_idempotent_on_canonical_input() {
        // Already-canonical kwargs (`style=`, `on_click=`) must not
        // be re-mangled.
        let src = "div(style=1, on_click=f)\n";
        assert_eq!(expand(src), src);
    }

    // -----------------------------------------------------------------
    // Phase 2.12: User-supplied dictionary overrides
    // -----------------------------------------------------------------

    #[test]
    fn user_dict_adds_new_alias() {
        let mut dict = std::collections::HashMap::new();
        dict.insert("BRAND".to_string(), "#9ca3af".to_string());
        let out = expand_with_dict("color = $BRAND\n", &dict);
        assert_eq!(out, "color = \"#9ca3af\"\n");
    }

    #[test]
    fn user_dict_overrides_bundled_alias() {
        // `$c1` is bundled as "#9ca3af"; user override wins.
        let mut dict = std::collections::HashMap::new();
        dict.insert("c1".to_string(), "#000000".to_string());
        let out = expand_with_dict("color = $c1\n", &dict);
        assert_eq!(out, "color = \"#000000\"\n");
    }

    #[test]
    fn user_dict_accepts_pre_quoted_values() {
        // If the user already wrapped quotes (e.g. for single-quoted
        // output or escape sequences), emit verbatim.
        let mut dict = std::collections::HashMap::new();
        dict.insert("X".to_string(), "'foo'".to_string());
        dict.insert("Y".to_string(), "\"bar\\n\"".to_string());
        let out = expand_with_dict("a = $X\nb = $Y\n", &dict);
        assert_eq!(out, "a = 'foo'\nb = \"bar\\n\"\n");
    }

    #[test]
    fn user_dict_empty_falls_back_to_bundled() {
        // Empty user dict — bundled `$c1` still works.
        let dict = std::collections::HashMap::new();
        let out = expand_with_dict("color = $c1\n", &dict);
        assert_eq!(out, "color = \"#9ca3af\"\n");
    }

    // -----------------------------------------------------------------
    // Triple-quoted string handling (Python docstring regression)
    // -----------------------------------------------------------------

    #[test]
    fn triple_quoted_docstring_does_not_break_hook_substitution() {
        // Regression: a docstring containing inner `"…"` groups used to
        // leave the scanner in an "in-string" state past the closing
        // `"""`, blocking later hook/kwarg/dictionary substitutions.
        // The trigger required at least one `#` after an unbalanced inner
        // quote — common in CSS-color examples like `"#3b82f6"`.
        let src = "\"\"\"\n`\"#3b82f6\"`\n\"\"\"\nus(0)\n";
        let want = "\"\"\"\n`\"#3b82f6\"`\n\"\"\"\nuse_state(0)\n";
        assert_eq!(expand(src), want);
    }

    #[test]
    fn triple_quoted_docstring_does_not_break_dollar_substitution() {
        let src = "\"\"\"\n`\"#3b82f6\"`\n\"\"\"\nx = $p4\n";
        let want = "\"\"\"\n`\"#3b82f6\"`\n\"\"\"\nx = \"16px\"\n";
        assert_eq!(expand(src), want);
    }

    #[test]
    fn triple_quoted_string_inner_quote_preserved() {
        // Idempotency: the docstring content itself is preserved exactly
        // (no leaks of substitution into the body).
        let src = "\"\"\"\n  `us(0)` and $p4\n\"\"\"\n";
        assert_eq!(expand(src), src);
    }

    #[test]
    fn triple_quoted_single_quote_variant_works() {
        let src = "'''\n`\"#fff\"`\n'''\nx = $p4\n";
        let want = "'''\n`\"#fff\"`\n'''\nx = \"16px\"\n";
        assert_eq!(expand(src), want);
    }

    #[test]
    fn user_dict_underscore_alias() {
        // User aliases may contain underscores — bundled aliases are
        // letter+digit only, but project-local names like `BRAND_GRAY`
        // should work.
        let mut dict = std::collections::HashMap::new();
        dict.insert("BRAND_GRAY".to_string(), "#9ca3af".to_string());
        let out = expand_with_dict("c = $BRAND_GRAY\n", &dict);
        assert_eq!(out, "c = \"#9ca3af\"\n");
    }

    #[test]
    fn expander_output_is_lexable() {
        // Smoke check: feed expanded `.psc` into the Phase 1 lexer and
        // assert it produces no lex errors.
        let psc = "\
R*
T*

@d
class Counter:
    count: int

@c
def App():
    c, sc = use_state(0)
    return None
";
        let expanded = expand(psc);
        let result = pyths_lexer::lex_recovering(&expanded);
        assert!(
            result.errors.is_empty(),
            "expander emitted code that fails lexer: {:?}",
            result.errors
        );
    }

    // -----------------------------------------------------------------
    // Tier E: %NAME idiom substitution + expand_with_config end-to-end
    // -----------------------------------------------------------------

    #[test]
    fn idiom_expand_with_config_basic() {
        let mut idioms = std::collections::HashMap::new();
        idioms.insert(
            "HTTPCHECK".into(),
            "if not response.ok:\n    raise Exception(f\"HTTP {response.status}\")\nreturn await response.json()".into(),
        );
        let dict = std::collections::HashMap::new();
        let src = "%HTTPCHECK\n";
        let out = expand_with_config(src, &dict, &idioms);
        assert_eq!(
            out,
            "if not response.ok:\n    raise Exception(f\"HTTP {response.status}\")\nreturn await response.json()\n"
        );
    }

    #[test]
    fn idiom_plus_dollar_alias_composes() {
        // An idiom fragment containing a `$NAME` alias — idioms run first,
        // then the `$NAME` dictionary pass picks up the alias from the
        // expanded fragment.
        let mut idioms = std::collections::HashMap::new();
        idioms.insert("STYLED".into(), "color = $c1".into());
        let dict = std::collections::HashMap::new();
        let src = "%STYLED\n";
        let out = expand_with_config(src, &dict, &idioms);
        // `$c1` → `"#9ca3af"` (bundled alias from strings.rs)
        assert_eq!(out, "color = \"#9ca3af\"\n");
    }

    #[test]
    fn idiom_unknown_passes_through_in_full_pipeline() {
        let idioms = std::collections::HashMap::new();
        let dict = std::collections::HashMap::new();
        let src = "%ZZ\n";
        let out = expand_with_config(src, &dict, &idioms);
        assert_eq!(out, "%ZZ\n");
    }

    #[test]
    fn expand_with_dict_still_works_unchanged() {
        // Regression: existing callers of expand_with_dict must be
        // unaffected (idioms implicitly empty).
        let mut dict = std::collections::HashMap::new();
        dict.insert("BRAND".to_string(), "#abc".to_string());
        let out = expand_with_dict("color = $BRAND\n", &dict);
        assert_eq!(out, "color = \"#abc\"\n");
    }

    #[test]
    fn idiom_combined_with_tier_a_and_kwargs_and_hooks() {
        // Full pipeline integration: idiom → Tier A →
        // Tier B kwargs → hooks → $-dict.
        let mut idioms = std::collections::HashMap::new();
        // Idiom that includes a kwarg alias and a hook alias.
        idioms.insert(
            "COUNTER".into(),
            "c, sc = us(0)\nreturn div(st={\"color\": $c1})(c)".into(),
        );
        let dict = std::collections::HashMap::new();
        let out = expand_with_config("%COUNTER\n", &dict, &idioms);
        assert_eq!(
            out,
            "c, sc = use_state(0)\nreturn div(style={\"color\": \"#9ca3af\"})(c)\n"
        );
    }
}
