use crate::tokens::Token;
use logos::Logos;

/// A token with its byte offset span in the source.
#[derive(Debug, Clone, PartialEq)]
pub struct SpannedToken {
    pub token: Token,
    pub span: std::ops::Range<usize>,
}

/// Result of error-recovering lexing.
#[derive(Debug, Clone)]
pub struct LexResult {
    pub tokens: Vec<SpannedToken>,
    pub errors: Vec<LexError>,
}

/// Lex source code into a stream of tokens with INDENT/DEDENT/NEWLINE injection.
///
/// This implements Python-style indentation handling:
/// 1. At each line start, measure the indentation level (spaces; tabs = 8 spaces).
/// 2. If increased, emit INDENT.
/// 3. If decreased, emit one or more DEDENT tokens to return to a previous level.
/// 4. Emit NEWLINE between logical lines.
/// 5. Lines inside brackets ((), [], {}) do not emit NEWLINE/INDENT/DEDENT.
/// 6. Blank lines and comment-only lines are ignored for indentation.
pub fn lex(source: &str) -> Result<Vec<SpannedToken>, LexError> {
    let result = lex_recovering(source);
    if result.errors.is_empty() {
        Ok(result.tokens)
    } else {
        Err(result.errors.into_iter().next().unwrap())
    }
}

/// Lex source code with error recovery, collecting all errors instead of stopping at the first.
pub fn lex_recovering(source: &str) -> LexResult {
    let mut errors = Vec::new();

    // Phase 1: Get raw tokens from logos, recovering from bad characters
    let (raw_tokens, continuations) = lex_raw_recovering(source, &mut errors);

    // Phase 2: Process indentation with recovery
    let tokens = inject_indentation_recovering(source, raw_tokens, &continuations, &mut errors);

    LexResult { tokens, errors }
}

#[derive(Debug, Clone)]
pub struct LexError {
    pub message: String,
    pub span: std::ops::Range<usize>,
}

impl std::fmt::Display for LexError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Lexer error at {:?}: {}", self.span, self.message)
    }
}

/// Phase 1: Raw tokenization with logos, recovering from errors by skipping bad characters.
///
/// Returns the tokens plus the byte offsets of explicit line-continuation
/// backslashes (round-4 follow-up: a code-level `\` immediately before
/// the line terminator is Python's explicit continuation, previously
/// rejected as "Unexpected character"). Backslashes inside strings,
/// f-strings, and comments never reach the error branch (they are part
/// of those tokens), so any `\` seen here is code-level by construction.
fn lex_raw_recovering(source: &str, errors: &mut Vec<LexError>) -> (Vec<SpannedToken>, Vec<usize>) {
    let mut tokens = Vec::new();
    let mut continuations = Vec::new();
    let mut lexer = Token::lexer(source);

    while let Some(result) = lexer.next() {
        let span = lexer.span();
        match result {
            Ok(token) => {
                // Skip comments
                if matches!(token, Token::Comment) {
                    continue;
                }
                tokens.push(SpannedToken { token, span });
            }
            Err(()) => {
                // Check if it's a newline character — logos skips whitespace but not newlines
                let slice = &source[span.clone()];
                if slice.contains('\n') || slice.contains('\r') {
                    // Skip — we handle newlines via line analysis
                    continue;
                }
                if slice == "\\" {
                    // Explicit line continuation: `\` directly before the
                    // line terminator (Python allows nothing in between).
                    let rest = &source.as_bytes()[span.end..];
                    let after_cr = if rest.first() == Some(&b'\r') {
                        &rest[1..]
                    } else {
                        rest
                    };
                    if after_cr.first() == Some(&b'\n') {
                        continuations.push(span.start);
                        continue;
                    }
                }
                // B13: a single leading UTF-8 BOM (U+FEFF, bytes EF BB BF) is
                // stripped, matching CPython which silently accepts a BOM at the
                // very start of a source file. Skipping it here (rather than
                // pre-stripping the source string) keeps every downstream byte
                // span exact. A BOM anywhere but byte 0 stays an error.
                if span.start == 0 && slice == "\u{feff}" {
                    continue;
                }
                // B11: an integer literal too large for the i128 backing store
                // reaches this branch because the logos numeric callbacks return
                // `None` on overflow, which logos reports as an error over the
                // whole matched digit run. Surface a dedicated, clear diagnostic
                // instead of the misleading "Unexpected character '999…'". Any
                // token that began with an ASCII digit but failed to lex is an
                // out-of-range numeric literal (identifiers can't start with a
                // digit). See docs/known-limitations.md ("Integer literal range").
                if slice.starts_with(|c: char| c.is_ascii_digit()) && slice.len() > 1 {
                    errors.push(LexError {
                        message: format!(
                            "Integer literal '{}' exceeds the supported range (values must fit in 128 bits)",
                            slice
                        ),
                        span,
                    });
                    continue;
                }
                // Recovery: record error and skip the bad character
                errors.push(LexError {
                    message: format!("Unexpected character '{}' — not valid in PythScribe", slice),
                    span,
                });
            }
        }
    }

    (tokens, continuations)
}

/// Phase 2: Walk through raw tokens and inject INDENT/DEDENT/NEWLINE with recovery.
fn inject_indentation_recovering(
    source: &str,
    raw_tokens: Vec<SpannedToken>,
    continuations: &[usize],
    errors: &mut Vec<LexError>,
) -> Vec<SpannedToken> {
    let mut result = Vec::new();
    let mut indent_stack: Vec<usize> = vec![0]; // Stack of indentation levels
    let mut bracket_depth: usize = 0; // Inside brackets = implicit line continuation
                                      // Explicit `\`-continuation: true while the PREVIOUS processed line
                                      // ended in a continuation backslash — the current line is the same
                                      // logical line (no INDENT/DEDENT), and the previous line emitted no
                                      // NEWLINE.
    let mut continued_from_prev = false;

    // Build a list of byte ranges occupied by multi-line tokens — chiefly
    // triple-quoted strings (and their f-string siblings). Lines whose
    // content starts inside one of these ranges must NOT generate
    // INDENT/DEDENT tokens, since their textual indentation is part of
    // the string's content, not Python-level block structure.
    let multiline_string_ranges: Vec<std::ops::Range<usize>> = raw_tokens
        .iter()
        .filter(|t| {
            matches!(t.token, Token::String_(_) | Token::FString(_))
                && t.span.end > t.span.start
                && source.as_bytes()[t.span.start..t.span.end].contains(&b'\n')
        })
        .map(|t| t.span.clone())
        .collect();
    let inside_multiline_string = |byte_offset: usize| -> bool {
        multiline_string_ranges
            .iter()
            .any(|r| byte_offset > r.start && byte_offset < r.end)
    };

    // Group tokens by line
    let lines = split_into_lines(source);
    let mut token_idx = 0;

    for line_info in &lines {
        if line_info.is_blank || line_info.is_comment_only {
            // Skip blank / comment-only lines — advance past their tokens
            while token_idx < raw_tokens.len()
                && raw_tokens[token_idx].span.start < line_info.end_offset
            {
                token_idx += 1;
            }
            continue;
        }

        // Lines whose content STARTS inside a multi-line string don't
        // carry INDENT/DEDENT semantics — their indentation is string
        // content, not block structure. INTERIOR lines (the string
        // continues past them) contribute nothing else and are skipped
        // outright. But the CLOSING line of a triple-quoted string can
        // carry trailing tokens after the string ends — `print("""a
        // b""")`, and the f-string siblings (#109) — which were
        // previously dropped wholesale (the `)` vanished → "Expected ),
        // found EOF"). Closing lines now fall through to normal token /
        // bracket / NEWLINE processing, with only the indent handling
        // suppressed.
        let starts_inside_string = inside_multiline_string(line_info.content_start);
        // Whether this line's END is still inside a multi-line string —
        // true for the OPENING and INTERIOR lines of a string that
        // continues past this line.
        //
        // `end_offset` is the byte offset of the line's terminating `\n`
        // (see `split_into_lines`), and THAT is exactly the byte to test:
        // if the newline itself lies inside the string span, the string
        // continues onto the next line; if it does not, the string ended
        // on this line, so this is a CLOSING line.
        //
        // Bug fixed (#193): this used `end_offset - 1`. On a closing line
        // the `"""` abuts the newline, so `end_offset - 1` is the final
        // `"` — still strictly inside the span under the half-open
        // `> r.start && < r.end` test. Closing lines were therefore
        // misclassified as INTERIOR, `closes_string` never fired, and the
        // statement's NEWLINE was never emitted.
        let ends_inside_string = inside_multiline_string(line_info.end_offset);
        if starts_inside_string {
            // Skip tokens starting inside the string (in practice none —
            // the string is a single token emitted with its opening line).
            while token_idx < raw_tokens.len()
                && raw_tokens[token_idx].span.start < line_info.end_offset
                && inside_multiline_string(raw_tokens[token_idx].span.start)
            {
                token_idx += 1;
            }
            if ends_inside_string {
                // Interior line — nothing else on it.
                continue;
            }
        }

        // Does THIS line end with an explicit `\` continuation?
        let ends_with_continuation = continuations
            .iter()
            .any(|&off| off >= line_info.start_offset && off < line_info.end_offset);

        // Emit indentation changes (only outside brackets, and never for
        // string-continuation lines or `\`-continuation lines — those are
        // the same logical line as their predecessor)
        if bracket_depth == 0 && !starts_inside_string && !continued_from_prev {
            let indent = line_info.indent_level;
            let current = *indent_stack.last().unwrap();

            if indent > current {
                indent_stack.push(indent);
                result.push(SpannedToken {
                    token: Token::Indent,
                    span: line_info.start_offset..line_info.start_offset,
                });
            } else if indent < current {
                // Pop indent levels until we match
                while *indent_stack.last().unwrap() > indent {
                    indent_stack.pop();
                    result.push(SpannedToken {
                        token: Token::Dedent,
                        span: line_info.start_offset..line_info.start_offset,
                    });
                }
                if *indent_stack.last().unwrap() != indent {
                    // Recovery: record error and snap to nearest valid indent level
                    let expected = *indent_stack.last().unwrap();
                    errors.push(LexError {
                        message: format!(
                            "Inconsistent indentation — expected {} spaces, found {}",
                            expected, indent
                        ),
                        span: line_info.start_offset..line_info.content_start,
                    });
                    // Snap: treat as if indented at the current stack level
                }
            }
        }

        // Emit tokens for this line
        let mut emitted_any = false;
        while token_idx < raw_tokens.len()
            && raw_tokens[token_idx].span.start < line_info.end_offset
        {
            let tok = &raw_tokens[token_idx];
            // Track bracket depth
            match &tok.token {
                Token::LParen | Token::LBracket | Token::LBrace => bracket_depth += 1,
                Token::RParen | Token::RBracket | Token::RBrace => {
                    bracket_depth = bracket_depth.saturating_sub(1);
                }
                _ => {}
            }
            result.push(tok.clone());
            emitted_any = true;
            token_idx += 1;
        }

        // Emit NEWLINE at end of line — only outside brackets, only for
        // lines that carried tokens, and NOT when the line ends inside a
        // multi-line string (the statement continues on the string's
        // closing line, which emits the NEWLINE instead; a closing line
        // emits it even with zero trailing tokens, since the statement
        // the string belongs to ends here).
        let closes_string = starts_inside_string && !ends_inside_string;
        if bracket_depth == 0
            && (emitted_any || closes_string)
            && !ends_inside_string
            && !ends_with_continuation
        {
            let nl_pos = line_info.end_offset;
            result.push(SpannedToken {
                token: Token::Newline,
                span: nl_pos..nl_pos,
            });
        }

        continued_from_prev = ends_with_continuation;
    }

    // Emit final DEDENTs to close all open blocks
    while indent_stack.len() > 1 {
        indent_stack.pop();
        let pos = source.len();
        result.push(SpannedToken {
            token: Token::Dedent,
            span: pos..pos,
        });
    }

    // Emit EOF
    let eof_pos = source.len();
    result.push(SpannedToken {
        token: Token::Eof,
        span: eof_pos..eof_pos,
    });

    result
}

/// Information about a single source line.
struct LineInfo {
    start_offset: usize,
    end_offset: usize,
    content_start: usize,
    indent_level: usize,
    is_blank: bool,
    is_comment_only: bool,
}

/// Split source into lines with indentation info.
fn split_into_lines(source: &str) -> Vec<LineInfo> {
    let mut lines = Vec::new();
    let mut offset = 0;

    for line in source.split('\n') {
        let line_start = offset;
        let line_end = offset + line.len();
        let trimmed = line.trim_end_matches('\r');

        // Calculate indentation
        let mut indent = 0;
        let mut content_start = line_start;
        for ch in trimmed.chars() {
            match ch {
                ' ' => {
                    indent += 1;
                    content_start += 1;
                }
                '\t' => {
                    indent = (indent / 8 + 1) * 8; // Tab stops at multiples of 8
                    content_start += 1;
                }
                _ => break,
            }
        }

        let remaining = trimmed.trim_start();
        let is_blank = remaining.is_empty();
        let is_comment_only = remaining.starts_with('#');

        lines.push(LineInfo {
            start_offset: line_start,
            end_offset: line_end,
            content_start,
            indent_level: indent,
            is_blank,
            is_comment_only,
        });

        offset = line_end + 1; // +1 for the \n
    }

    lines
}

#[cfg(test)]
mod tests {
    use super::*;

    fn token_types(source: &str) -> Vec<Token> {
        lex(source).unwrap().into_iter().map(|t| t.token).collect()
    }

    #[test]
    fn test_triple_quoted_crlf_universal_newlines() {
        // A Windows (CRLF) checkout: CPython reads source in universal-newlines
        // mode, so the VALUE of a triple-quoted literal never contains `\r`.
        assert_eq!(
            token_types("'''a\r\nb'''"),
            vec![Token::String_("a\nb".into()), Token::Newline, Token::Eof],
        );
        assert_eq!(
            token_types("\"\"\"x\r\ny\r\nz\"\"\""),
            vec![Token::String_("x\ny\nz".into()), Token::Newline, Token::Eof],
        );
        // Raw triple strings: raw-ness suppresses escape processing, not
        // source newline decoding — CPython gives `\n` here too.
        assert_eq!(
            token_types("r'''a\r\nb'''"),
            vec![Token::String_("a\nb".into()), Token::Newline, Token::Eof],
        );
        // An ESCAPED `\r` (backslash-r in source) must survive as a real CR.
        assert_eq!(
            token_types("'''a\\r\\nb'''"),
            vec![Token::String_("a\r\nb".into()), Token::Newline, Token::Eof],
        );
    }

    #[test]
    fn test_base_prefix_underscore_literals() {
        // CPython grammar: `"0" ("x"|"X") (["_"] hexdigit)+` — an underscore
        // directly after the base prefix is legal (autotester dashed_numbers).
        assert_eq!(
            token_types("0x_ff_ff_ff"),
            vec![Token::Integer(0xff_ff_ff), Token::Newline, Token::Eof],
        );
        assert_eq!(
            token_types("0b_1010"),
            vec![Token::Integer(0b1010), Token::Newline, Token::Eof],
        );
        assert_eq!(
            token_types("0o_17"),
            vec![Token::Integer(0o17), Token::Newline, Token::Eof],
        );
    }

    #[test]
    fn test_pep701_same_quote_fstring() {
        // PBT-3 (PEP 701 subset): the outer quote may be reused inside
        // {...} expression parts (CPython >= 3.12; ast.unparse emits this).
        assert_eq!(
            token_types("f'{''}'"),
            vec![Token::FString("{''}".into()), Token::Newline, Token::Eof],
        );
        assert_eq!(
            token_types("f'val: {'Hello, World!'}'"),
            vec![
                Token::FString("val: {'Hello, World!'}".into()),
                Token::Newline,
                Token::Eof
            ],
        );
        // Same-quote string containing the f-string's closing brace/quote.
        assert_eq!(
            token_types("f'{'}'}'"),
            vec![Token::FString("{'}'}".into()), Token::Newline, Token::Eof],
        );
        // Different-quote nesting keeps working.
        assert_eq!(
            token_types("f\"x{\"y\"}z\""),
            vec![
                Token::FString("x{\"y\"}z".into()),
                Token::Newline,
                Token::Eof
            ],
        );
        // Escaped braces + plain bodies unchanged.
        assert_eq!(
            token_types("f'{{lit}}'"),
            vec![Token::FString("{{lit}}".into()), Token::Newline, Token::Eof],
        );
        assert_eq!(
            token_types("f''"),
            vec![Token::FString(String::new()), Token::Newline, Token::Eof],
        );
        // Adjacent expression parts and a following plain string.
        assert_eq!(
            token_types("f'{a}{b}' + 'z'"),
            vec![
                Token::FString("{a}{b}".into()),
                Token::Plus,
                Token::String_("z".into()),
                Token::Newline,
                Token::Eof,
            ],
        );
        // Triple-quoted f-strings still take the long-string path.
        assert_eq!(
            token_types("f'''a{b}c'''"),
            vec![Token::FString("a{b}c".into()), Token::Newline, Token::Eof],
        );
        // Unterminated forms still fail to lex.
        assert!(lex("f'{x").is_err());
        assert!(lex("f'{'unclosed").is_err());
    }

    #[test]
    fn test_radix_and_bigint_literals() {
        // #255: hex/oct/bin bases + big decimals (past i64, up to i128).
        assert_eq!(
            token_types("0xFF"),
            vec![Token::Integer(255), Token::Newline, Token::Eof]
        );
        assert_eq!(
            token_types("0o17"),
            vec![Token::Integer(15), Token::Newline, Token::Eof]
        );
        assert_eq!(
            token_types("0b1010"),
            vec![Token::Integer(10), Token::Newline, Token::Eof]
        );
        assert_eq!(
            token_types("0xDEAD_BEEF"),
            vec![Token::Integer(0xDEAD_BEEF), Token::Newline, Token::Eof]
        );
        assert_eq!(
            token_types("12345678901234567890"),
            vec![
                Token::Integer(12345678901234567890),
                Token::Newline,
                Token::Eof
            ],
        );
        // > 2^64 still fits i128
        assert_eq!(
            token_types("0xFFFFFFFFFFFFFFFF"),
            vec![
                Token::Integer(0xFFFF_FFFF_FFFF_FFFF),
                Token::Newline,
                Token::Eof
            ],
        );
    }

    #[test]
    fn test_dotted_float_forms_lex_as_float() {
        // #208: trailing-dot (`1.`, `1.e3`) and leading-dot (`.5`) floats.
        // Previously `1.` lexed as Integer(1) + Dot → "Expected identifier".
        assert_eq!(
            token_types("1."),
            vec![Token::Float(1.0), Token::Newline, Token::Eof]
        );
        assert_eq!(
            token_types(".5"),
            vec![Token::Float(0.5), Token::Newline, Token::Eof]
        );
        assert_eq!(
            token_types("1.e3"),
            vec![Token::Float(1000.0), Token::Newline, Token::Eof]
        );
        // The fractional form is unchanged, and `1. / 3` is three tokens.
        assert_eq!(
            token_types("1. / 3"),
            vec![
                Token::Float(1.0),
                Token::Slash,
                Token::Integer(3),
                Token::Newline,
                Token::Eof
            ],
        );
        // Attribute access on a name is untouched (`.` before a letter is Dot).
        assert_eq!(
            token_types("x.y"),
            vec![
                Token::Identifier("x".to_string()),
                Token::Dot,
                Token::Identifier("y".to_string()),
                Token::Newline,
                Token::Eof,
            ],
        );
    }

    #[test]
    fn test_backslash_line_continuation_joins_logical_line() {
        // Round-4 follow-up: `\` before the newline is Python's explicit
        // continuation — one logical line, no NEWLINE/INDENT between.
        let tokens = token_types("b = 1 \\\n    and 2");
        assert!(
            !tokens.contains(&Token::Indent),
            "no INDENT across continuation: {:?}",
            tokens
        );
        assert_eq!(
            tokens
                .iter()
                .filter(|t| matches!(t, Token::Newline))
                .count(),
            1,
            "single logical line: {:?}",
            tokens
        );
    }

    #[test]
    fn test_backslash_continuation_dedented_next_line() {
        // The continuation line's indentation carries no block meaning,
        // even when LESS indented than the opener.
        assert!(lex("def f(a):\n    return a \\\nor 2\nx = 1").is_ok());
    }

    #[test]
    fn test_backslash_inside_string_still_escape() {
        // A trailing backslash INSIDE a string is an escape, not a
        // continuation — must lex exactly as before.
        let tokens = token_types("s = \"a\\\\b\"\nprint(s)");
        assert_eq!(
            tokens
                .iter()
                .filter(|t| matches!(t, Token::Newline))
                .count(),
            2,
            "two logical lines: {:?}",
            tokens
        );
    }

    #[test]
    fn test_bare_backslash_not_before_newline_still_errors() {
        // `\` NOT at line end stays an error (Python agrees).
        assert!(lex("a = 1 \\ + 2").is_err());
    }

    #[test]
    fn test_simple_print() {
        let tokens = token_types("print(\"hello\")");
        assert_eq!(
            tokens,
            vec![
                Token::Identifier("print".to_string()),
                Token::LParen,
                Token::String_("hello".to_string()),
                Token::RParen,
                Token::Newline,
                Token::Eof,
            ]
        );
    }

    #[test]
    fn test_indentation() {
        let source = "if True:\n    x = 1\n    y = 2\nz = 3";
        let tokens = token_types(source);
        assert_eq!(
            tokens,
            vec![
                Token::If,
                Token::True_,
                Token::Colon,
                Token::Newline,
                Token::Indent,
                Token::Identifier("x".to_string()),
                Token::Eq,
                Token::Integer(1),
                Token::Newline,
                Token::Identifier("y".to_string()),
                Token::Eq,
                Token::Integer(2),
                Token::Newline,
                Token::Dedent,
                Token::Identifier("z".to_string()),
                Token::Eq,
                Token::Integer(3),
                Token::Newline,
                Token::Eof,
            ]
        );
    }

    #[test]
    fn test_nested_indentation() {
        let source = "if True:\n    if False:\n        x = 1\ny = 2";
        let tokens = token_types(source);
        assert!(tokens.contains(&Token::Indent));
        // Should have 2 indents and 2 dedents
        let indent_count = tokens.iter().filter(|t| matches!(t, Token::Indent)).count();
        let dedent_count = tokens.iter().filter(|t| matches!(t, Token::Dedent)).count();
        assert_eq!(indent_count, 2);
        assert_eq!(dedent_count, 2);
    }

    #[test]
    fn test_bracket_continuation() {
        let source = "x = (1 +\n    2)";
        let tokens = token_types(source);
        // Should NOT have INDENT/DEDENT inside brackets
        assert!(!tokens.contains(&Token::Indent));
        assert!(!tokens.contains(&Token::Dedent));
    }

    #[test]
    fn test_blank_lines_ignored() {
        let source = "x = 1\n\ny = 2";
        let tokens = token_types(source);
        assert_eq!(
            tokens,
            vec![
                Token::Identifier("x".to_string()),
                Token::Eq,
                Token::Integer(1),
                Token::Newline,
                Token::Identifier("y".to_string()),
                Token::Eq,
                Token::Integer(2),
                Token::Newline,
                Token::Eof,
            ]
        );
    }

    // --- Error recovery tests ---

    #[test]
    fn test_lex_recovery_bad_char() {
        // $ is not valid in PythScribe — recovery should skip it and produce tokens for valid parts
        let result = lex_recovering("x = $1 + 2");
        assert!(!result.errors.is_empty());
        assert_eq!(result.errors.len(), 1);
        assert!(result.errors[0].message.contains("$"));
        // Should still have tokens for x, =, 1, +, 2
        let token_types: Vec<_> = result.tokens.iter().map(|t| &t.token).collect();
        assert!(token_types.contains(&&Token::Identifier("x".to_string())));
        assert!(token_types.contains(&&Token::Integer(1)));
        assert!(token_types.contains(&&Token::Plus));
        assert!(token_types.contains(&&Token::Integer(2)));
    }

    #[test]
    fn test_lex_recovery_multiple_bad_chars() {
        let result = lex_recovering("x = $1\ny = @#2");
        // Should have errors for $ and # (# starts a comment, so just $ and potentially others)
        // Actually @ is valid (Token::At), and # starts a comment, so only $ is invalid
        assert!(result.errors.len() >= 1);
        // Tokens for valid parts should still be present
        let has_x = result
            .tokens
            .iter()
            .any(|t| t.token == Token::Identifier("x".to_string()));
        let has_y = result
            .tokens
            .iter()
            .any(|t| t.token == Token::Identifier("y".to_string()));
        assert!(has_x);
        assert!(has_y);
    }

    #[test]
    fn test_lex_recovery_inconsistent_indent() {
        // 4-space indent then 3-space (inconsistent)
        let source = "if True:\n    x = 1\n   y = 2\nz = 3";
        let result = lex_recovering(source);
        // Should have an inconsistent indentation error
        assert!(!result.errors.is_empty());
        assert!(result.errors[0]
            .message
            .contains("Inconsistent indentation"));
        // Should still produce tokens including z = 3
        let has_z = result
            .tokens
            .iter()
            .any(|t| t.token == Token::Identifier("z".to_string()));
        assert!(has_z);
    }

    #[test]
    fn test_lex_recovering_returns_lex_result() {
        let result = lex_recovering("x = 1");
        assert!(result.errors.is_empty());
        assert!(!result.tokens.is_empty());
        assert_eq!(result.tokens.last().unwrap().token, Token::Eof);
    }

    #[test]
    fn test_triple_quoted_docstring_with_indented_content() {
        // Regression: indented content inside `"""…"""` used to inject
        // a spurious INDENT/DEDENT pair because the line-by-line
        // splitter didn't know about multi-line strings. The lexer's
        // logos rule correctly absorbs the whole `"""…"""` as one
        // String_ token; the indent injector now skips lines that
        // start inside any such multi-line string span.
        let source = "\"\"\"\nThis is a docstring.\n  Indented line that looks like code.\n\"\"\"\n\nx = 1\n";
        let result = lex_recovering(source);
        assert!(
            result.errors.is_empty(),
            "no lex errors: {:?}",
            result.errors
        );
        // The token sequence: String_, NEWLINE, Identifier("x"), Eq, Integer(1), NEWLINE, EOF.
        // Critically: no INDENT or DEDENT.
        let ttypes: Vec<_> = result.tokens.iter().map(|t| &t.token).collect();
        assert!(
            !ttypes.iter().any(|t| matches!(t, Token::Indent)),
            "no spurious INDENT: {:?}",
            ttypes,
        );
        assert!(
            !ttypes.iter().any(|t| matches!(t, Token::Dedent)),
            "no spurious DEDENT: {:?}",
            ttypes,
        );
    }

    #[test]
    fn test_triple_quoted_class_docstring_indented() {
        // Real-world: docstrings inside a class body are indented
        // themselves, and may contain further-indented content.
        let source = "class C:\n    \"\"\"\n    A class docstring with\n    multiple indented lines.\n    \"\"\"\n    x = 1\n";
        let result = lex_recovering(source);
        assert!(
            result.errors.is_empty(),
            "no lex errors: {:?}",
            result.errors
        );
        // Should produce: class, C, :, NEWLINE, INDENT, String_, NEWLINE,
        //                  x, =, 1, NEWLINE, DEDENT, EOF
        // Exactly one INDENT (for the class body) and one DEDENT.
        let indent_count = result
            .tokens
            .iter()
            .filter(|t| matches!(t.token, Token::Indent))
            .count();
        let dedent_count = result
            .tokens
            .iter()
            .filter(|t| matches!(t.token, Token::Dedent))
            .count();
        assert_eq!(indent_count, 1, "exactly one INDENT for class body");
        assert_eq!(dedent_count, 1, "exactly one DEDENT for class body");
    }

    #[test]
    fn test_triple_single_quoted_works_too() {
        // The `'''…'''` form follows the same rules.
        let source = "'''\n  indented content\n'''\nx = 1\n";
        let result = lex_recovering(source);
        assert!(result.errors.is_empty());
        assert!(!result
            .tokens
            .iter()
            .any(|t| matches!(t.token, Token::Indent)));
    }

    #[test]
    fn test_multiline_string_stmt_emits_newline_before_next_stmt() {
        // REGRESSION (#193): the closing line of a triple-quoted string
        // must emit the NEWLINE that terminates the statement the string
        // belongs to. `ends_inside_string` used `end_offset - 1`, which
        // for a closing line whose `"""` abuts the newline is still
        // strictly inside the string span — so `closes_string` never
        // fired and NO NEWLINE was emitted. Harmless while the parser's
        // statement-end check was a no-op; fatal once #190/#191 made
        // statement termination strict.
        let tokens = token_types("W = \"\"\"\nfoo\n\"\"\"\nX = 1\n");
        assert_eq!(
            tokens,
            vec![
                Token::Identifier("W".to_string()),
                Token::Eq,
                Token::String_("\nfoo\n".to_string()),
                Token::Newline,
                Token::Identifier("X".to_string()),
                Token::Eq,
                Token::Integer(1),
                Token::Newline,
                Token::Eof,
            ],
            "NEWLINE must separate the string statement from the next"
        );
    }

    /// Count NEWLINE tokens — the statement terminators the parser needs.
    fn newline_count(source: &str) -> usize {
        token_types(source)
            .iter()
            .filter(|t| matches!(t, Token::Newline))
            .count()
    }

    #[test]
    fn test_multiline_string_closing_line_emits_newline_variants() {
        // REGRESSION (#193), every shape the closing line can take.
        // Each source below is ONE multi-line-string statement plus one
        // more statement → exactly 2 NEWLINEs.
        for src in [
            // Assignment; module docstring; triple-single; f-string sibling.
            "W = \"\"\"\nfoo\n\"\"\"\nX = 1\n",
            "\"\"\"\ndoc\n\"\"\"\nx = 1\n",
            "a = '''\nx\n'''\nb = 2\n",
            "g = f\"\"\"\nval {n}\n\"\"\"\nh = 3\n",
            // Closing line with trailing tokens after the string ends.
            "print(\"\"\"a\nb\"\"\")\nx = 1\n",
            // Quote characters adjacent to the terminator.
            "s = \"\"\"a\"\n\"\"\"\nx = 1\n",
            // No trailing newline at EOF.
            "W = \"\"\"\nfoo\n\"\"\"\nX = 1",
        ] {
            assert_eq!(
                newline_count(src),
                2,
                "two statements → two NEWLINEs: {:?} → {:?}",
                src.escape_debug(),
                token_types(src)
            );
        }
    }

    #[test]
    fn test_multiline_string_as_last_statement_still_terminated() {
        // A string that closes at EOF must still terminate its statement.
        assert_eq!(newline_count("x = 1\nW = \"\"\"\nfoo\n\"\"\"\n"), 2);
        assert_eq!(newline_count("x = 1\nW = \"\"\"\nfoo\n\"\"\""), 2);
        // ...and a lone multi-line string is one statement.
        assert_eq!(newline_count("\"\"\"\nfoo\n\"\"\"\n"), 1);
    }

    #[test]
    fn test_multiline_string_inside_brackets_emits_no_interior_newline() {
        // Inside brackets nothing is terminated until the bracket closes:
        // the list is one statement, `m = 1` is the second.
        let src = "L = [\n    \"\"\"\na\n\"\"\",\n    \"b\",\n]\nm = 1\n";
        assert_eq!(
            newline_count(src),
            2,
            "bracketed multi-line string: {:?}",
            token_types(src)
        );
        assert!(!token_types(src).contains(&Token::Indent));
    }

    #[test]
    fn test_multiline_string_in_function_body_keeps_block_structure() {
        // Docstring + body + a statement after the block: the DEDENT must
        // still land, and each statement still gets its NEWLINE.
        let src = "def f():\n    \"\"\"\n    doc\n    \"\"\"\n    return 1\ny = 2\n";
        let tokens = token_types(src);
        assert_eq!(
            tokens.iter().filter(|t| matches!(t, Token::Indent)).count(),
            1,
            "one INDENT: {:?}",
            tokens
        );
        assert_eq!(
            tokens.iter().filter(|t| matches!(t, Token::Dedent)).count(),
            1,
            "one DEDENT: {:?}",
            tokens
        );
        // def-header, docstring, return, y = 2 → 4 NEWLINEs.
        assert_eq!(newline_count(src), 4, "{:?}", tokens);
    }

    #[test]
    fn test_multiline_string_content_looking_like_code_is_inert() {
        // The string body contains what looks like a block; it must not
        // produce INDENT/DEDENT, and the statement after it is still
        // separated.
        let src = "W = \"\"\"\ndef g():\n    return 1\n\"\"\"\nX = 1\n";
        let tokens = token_types(src);
        assert!(!tokens.contains(&Token::Indent), "{:?}", tokens);
        assert!(!tokens.contains(&Token::Dedent), "{:?}", tokens);
        assert!(
            !tokens.contains(&Token::Def),
            "string body is not code: {:?}",
            tokens
        );
        assert_eq!(newline_count(src), 2, "{:?}", tokens);
    }

    #[test]
    fn test_two_adjacent_multiline_strings_then_statement() {
        let src = "p = \"\"\"\n1\n\"\"\"\nq = \"\"\"\n2\n\"\"\"\nr = 3\n";
        assert_eq!(newline_count(src), 3, "{:?}", token_types(src));
    }

    #[test]
    fn test_crlf_multiline_string_closing_line_emits_newline() {
        // The closing line under CRLF ends `"""\r` — the `\r` sits between
        // the string's end and the `\n`. Still exactly one terminator.
        assert_eq!(newline_count("W = \"\"\"\r\nfoo\r\n\"\"\"\r\nX = 1\r\n"), 2);
    }

    #[test]
    fn test_lex_backward_compat() {
        // lex() should still return first error only
        let result = lex("x = $1");
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.message.contains("$"));

        // And succeed on valid input
        let result = lex("x = 1");
        assert!(result.is_ok());
    }

    // B13: a single leading UTF-8 BOM (U+FEFF) is silently accepted, matching
    // CPython. It must not become an "Unexpected character" error and must not
    // perturb the tokens that follow (spans stay exact).
    #[test]
    fn test_leading_bom_is_stripped() {
        let src = "\u{feff}x = 1\n";
        let result = lex(src);
        assert!(
            result.is_ok(),
            "leading BOM should lex cleanly: {:?}",
            result.err()
        );
        assert_eq!(
            token_types(src),
            vec![
                Token::Identifier("x".into()),
                Token::Eq,
                Token::Integer(1),
                Token::Newline,
                Token::Eof,
            ],
        );
    }

    // B13: a BOM anywhere other than byte 0 is NOT special — still an error.
    #[test]
    fn test_interior_bom_still_errors() {
        assert!(lex("x = 1\n\u{feff}y = 2\n").is_err());
    }

    // B11: an integer literal beyond the i128 backing store gets a dedicated,
    // clear diagnostic instead of the misleading "Unexpected character '999…'".
    #[test]
    fn test_over_range_integer_literal_diagnostic() {
        let over = "9".repeat(45); // ~10^45 ≫ i128::MAX (~1.7·10^38)
        let src = format!("x = {}\n", over);
        let result = lex(&src);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(
            err.message.contains("exceeds the supported range"),
            "want range diagnostic, got: {}",
            err.message
        );
        assert!(
            !err.message.contains("Unexpected character"),
            "must not surface the misleading generic message: {}",
            err.message
        );
    }

    // B11: a value that still fits in i128 keeps lexing (regression guard on the
    // boundary — the diagnostic must not fire for in-range literals).
    #[test]
    fn test_large_but_in_range_integer_still_lexes() {
        // 10^30 fits in i128 (max ≈ 1.7·10^38).
        assert!(lex("x = 1000000000000000000000000000000\n").is_ok());
    }
}
