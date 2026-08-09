use logos::Logos;

/// #95: decode Python string-literal escape sequences.
///
/// The lexer previously stored string bodies VERBATIM (source `"a\nb"`
/// kept the two chars `\` + `n`), so documented escapes never became
/// their characters at runtime (`len("a\tb")` was 4). Decoding happens
/// here, once, at the lexer boundary; the canonical printer re-escapes
/// on output (see pyths_print), and the JS emitter re-escapes for JS.
///
/// Handled (CPython 3.12 semantics): `\n \t \r \\ \' \" \a \b \f \v`,
/// `\0`..`\7` octal (up to 3 digits), `\xNN`, `\uNNNN`, `\UNNNNNNNN`,
/// and backslash-newline line continuation (removed). Any other escape
/// stays literal — backslash + char — matching the VALUE CPython
/// produces for unknown escapes (its deprecation warning has no output
/// effect).
pub fn decode_py_escapes(s: &str) -> String {
    if !s.contains('\\') {
        return s.to_string();
    }
    let chars: Vec<char> = s.chars().collect();
    let mut out = String::with_capacity(s.len());
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        if c != '\\' || i + 1 >= chars.len() {
            out.push(c);
            i += 1;
            continue;
        }
        let esc = chars[i + 1];
        match esc {
            'n' => {
                out.push('\n');
                i += 2;
            }
            't' => {
                out.push('\t');
                i += 2;
            }
            'r' => {
                out.push('\r');
                i += 2;
            }
            '\\' => {
                out.push('\\');
                i += 2;
            }
            '\'' => {
                out.push('\'');
                i += 2;
            }
            '"' => {
                out.push('"');
                i += 2;
            }
            'a' => {
                out.push('\u{07}');
                i += 2;
            }
            'b' => {
                out.push('\u{08}');
                i += 2;
            }
            'f' => {
                out.push('\u{0c}');
                i += 2;
            }
            'v' => {
                out.push('\u{0b}');
                i += 2;
            }
            '\n' => {
                i += 2;
            } // line continuation — removed
            '0'..='7' => {
                // Octal: up to 3 digits.
                let mut val: u32 = 0;
                let mut n = 0;
                while n < 3 && i + 1 + n < chars.len() {
                    let d = chars[i + 1 + n];
                    if !('0'..='7').contains(&d) {
                        break;
                    }
                    val = val * 8 + d.to_digit(8).unwrap();
                    n += 1;
                }
                out.push(char::from_u32(val).unwrap_or('\u{fffd}'));
                i += 1 + n;
            }
            'x' | 'u' | 'U' => {
                let want = match esc {
                    'x' => 2,
                    'u' => 4,
                    _ => 8,
                };
                let digits: String = chars[i + 2..].iter().take(want).collect();
                if digits.len() == want && digits.chars().all(|d| d.is_ascii_hexdigit()) {
                    if let Some(ch) = u32::from_str_radix(&digits, 16)
                        .ok()
                        .and_then(char::from_u32)
                    {
                        out.push(ch);
                        i += 2 + want;
                        continue;
                    }
                }
                // Malformed — keep literal (CPython raises SyntaxError;
                // keeping the source text is the lenient fallback).
                out.push('\\');
                out.push(esc);
                i += 2;
            }
            other => {
                // Unknown escape: literal backslash + char (CPython value
                // semantics for e.g. `\d` in a non-raw string).
                out.push('\\');
                out.push(other);
                i += 2;
            }
        }
    }
    out
}

/// #283: parse an imaginary literal (`2j`, `3.5j`, `1e3j`, `.5j`, `1.j`) into
/// the f64 magnitude of its imaginary part. Strips the trailing `j`/`J` and
/// any `_` digit separators, then reuses Rust's f64 parser (massaging a bare
/// trailing `.` — e.g. `1.j` -> `1.0` — the same way the Float rule does).
fn parse_imag(lex: &mut logos::Lexer<Token>) -> Option<f64> {
    let s = lex.slice();
    let s = &s[..s.len() - 1]; // drop the `j`/`J`
    let s = s.replace('_', "");
    // Massage `1.` / `1.e5` (trailing/interior bare dot) into a parseable form.
    let s = if let Some(rest) = s.strip_suffix('.') {
        format!("{}.0", rest)
    } else if let Some(idx) = s.find(".e").or_else(|| s.find(".E")) {
        format!("{}.0{}", &s[..idx], &s[idx + 1..])
    } else {
        s
    };
    s.parse::<f64>().ok()
}

/// Lex a triple-double-quoted string: scan until closing `"""`
fn lex_triple_double_string(lex: &mut logos::Lexer<Token>) -> Option<String> {
    let remainder = lex.remainder();
    if let Some(end) = remainder.find("\"\"\"") {
        let content = &remainder[..end];
        lex.bump(end + 3); // consume content + closing """
        Some(decode_py_escapes(content))
    } else {
        None
    }
}

/// Lex a triple-single-quoted string: scan until closing `'''`
fn lex_triple_single_string(lex: &mut logos::Lexer<Token>) -> Option<String> {
    let remainder = lex.remainder();
    if let Some(end) = remainder.find("'''") {
        let content = &remainder[..end];
        lex.bump(end + 3);
        Some(decode_py_escapes(content))
    } else {
        None
    }
}

/// Raw triple-double-quoted string (`r"""..."""`) — body verbatim (#100).
fn lex_triple_double_raw_string(lex: &mut logos::Lexer<Token>) -> Option<String> {
    let remainder = lex.remainder();
    if let Some(end) = remainder.find("\"\"\"") {
        let content = &remainder[..end];
        lex.bump(end + 3);
        Some(content.to_string())
    } else {
        None
    }
}

/// Raw triple-single-quoted string (`r'''...'''`) — body verbatim (#100).
fn lex_triple_single_raw_string(lex: &mut logos::Lexer<Token>) -> Option<String> {
    let remainder = lex.remainder();
    if let Some(end) = remainder.find("'''") {
        let content = &remainder[..end];
        lex.bump(end + 3);
        Some(content.to_string())
    } else {
        None
    }
}

/// PBT-3 (PEP 701 subset): scan a single-line f-string body whose
/// expression parts may reuse the outer quote (CPython >= 3.12 allows
/// `f'{'a'}'`; `ast.unparse` emits this form, so any 3.12-authored source
/// can contain it). A regex cannot see brace depth, so `f'...'` / `f"..."`
/// are lexed by this scanner instead:
///   - `\` skips the next char (escape);
///   - `{{` at literal level is an escaped brace; a lone `{` enters an
///     expression part (depth += 1), `}` leaves it;
///   - inside an expression part (depth > 0), a nested SINGLE-LINE string
///     literal of either quote — same or different from the f-string's own —
///     is skipped to its closing quote (its braces/quotes don't count);
///   - the f-string closes at the first unescaped own-quote at depth 0.
///
/// Subset limits (documented, unchanged from before where noted): raw
/// newlines still terminate (PEP 701 multi-line expression parts in
/// single-quoted f-strings are unsupported), and a same-quote f-string
/// nested INSIDE an expression part (f-in-f quote reuse) is not supported —
/// the nested `f'...'` is skipped as a plain string, which is correct
/// unless ITS expression parts reuse the quote again.
fn scan_line_fstring(lex: &mut logos::Lexer<Token>, quote: char) -> Option<String> {
    let rem = lex.remainder();
    let mut depth = 0usize;
    let mut iter = rem.char_indices().peekable();
    while let Some((i, c)) = iter.next() {
        match c {
            '\\' => {
                iter.next();
            }
            '\n' => return None,
            '{' => {
                if depth == 0 && matches!(iter.peek(), Some(&(_, '{'))) {
                    iter.next(); // literal `{{`
                } else {
                    depth += 1;
                }
            }
            '}' => {
                if depth == 0 && matches!(iter.peek(), Some(&(_, '}'))) {
                    iter.next(); // literal `}}`
                } else {
                    depth = depth.saturating_sub(1);
                }
            }
            '\'' | '"' if depth > 0 => {
                // Nested string literal inside an expression part.
                let q = c;
                let mut closed = false;
                while let Some((_, c2)) = iter.next() {
                    match c2 {
                        '\\' => {
                            iter.next();
                        }
                        '\n' => return None,
                        _ if c2 == q => {
                            closed = true;
                            break;
                        }
                        _ => {}
                    }
                }
                if !closed {
                    return None;
                }
            }
            _ if c == quote => {
                // depth == 0 here (the depth > 0 quote arm above matched first).
                let content = &rem[..i];
                lex.bump(i + quote.len_utf8());
                return Some(content.to_string());
            }
            _ => {}
        }
    }
    None
}

fn lex_double_quote_fstring(lex: &mut logos::Lexer<Token>) -> Option<String> {
    scan_line_fstring(lex, '"')
}

fn lex_single_quote_fstring(lex: &mut logos::Lexer<Token>) -> Option<String> {
    scan_line_fstring(lex, '\'')
}

/// Lex a triple-double-quoted f-string: scan until closing `"""`
fn lex_triple_double_fstring(lex: &mut logos::Lexer<Token>) -> Option<String> {
    let remainder = lex.remainder();
    if let Some(end) = remainder.find("\"\"\"") {
        let content = &remainder[..end];
        lex.bump(end + 3);
        Some(content.to_string())
    } else {
        None
    }
}

/// Lex a triple-single-quoted f-string: scan until closing `'''`
fn lex_triple_single_fstring(lex: &mut logos::Lexer<Token>) -> Option<String> {
    let remainder = lex.remainder();
    if let Some(end) = remainder.find("'''") {
        let content = &remainder[..end];
        lex.bump(end + 3);
        Some(content.to_string())
    } else {
        None
    }
}

/// Token type produced by the lexer.
/// The logos-derived tokens handle raw tokenization; the indentation
/// preprocessor adds Indent/Dedent/Newline tokens.
#[derive(Logos, Debug, Clone, PartialEq)]
#[logos(skip r"[ \t]+")] // Skip spaces/tabs (handled by indent preprocessor for line starts)
pub enum Token {
    // ── Keywords ──────────────────────────────────────────
    #[token("False")]
    False,
    #[token("None")]
    None_,
    #[token("True")]
    True_,
    #[token("and")]
    And,
    #[token("as")]
    As,
    #[token("assert")]
    Assert,
    #[token("async")]
    Async,
    #[token("await")]
    Await,
    #[token("break")]
    Break,
    #[token("class")]
    Class,
    #[token("continue")]
    Continue,
    #[token("def")]
    Def,
    #[token("del")]
    Del,
    #[token("elif")]
    Elif,
    #[token("else")]
    Else,
    #[token("except")]
    Except,
    #[token("finally")]
    Finally,
    #[token("for")]
    For,
    #[token("from")]
    From,
    #[token("global")]
    Global,
    #[token("if")]
    If,
    #[token("import")]
    Import,
    #[token("in")]
    In,
    #[token("is")]
    Is,
    #[token("lambda")]
    Lambda,
    #[token("nonlocal")]
    Nonlocal,
    #[token("not")]
    Not,
    #[token("or")]
    Or,
    #[token("pass")]
    Pass,
    #[token("raise")]
    Raise,
    #[token("return")]
    Return,
    #[token("try")]
    Try,
    #[token("while")]
    While,
    #[token("with")]
    With,
    #[token("yield")]
    Yield,
    // NOTE: `match` is deliberately NOT a `#[token]` — like `case` below it is
    // a CPython SOFT keyword: it introduces a match statement only as the
    // leading word of `match SUBJECT:`, and is an ordinary identifier
    // everywhere else (`match = re.search(...)`, `def match(...)`,
    // `for match in ...`). #231-follow-up: it used to be a hard keyword, which
    // made all of those a parse error. The variant is retained for the parser's
    // contextual dispatch; the lexer emits `Identifier("match")`.
    Match,
    // NOTE: `case` is deliberately NOT a token. CPython treats it as a
    // SOFT keyword — reserved only as the leading word of a case clause
    // inside a `match` suite; `case = 1`, `def case(): ...` etc. are legal
    // everywhere else (#79). It lexes as an ordinary Identifier and the
    // parser recognizes it contextually in parse_match_stmt.

    // ── Literals ──────────────────────────────────────────
    // #255: hold values as i128 so literals up to 39 digits (past i64's ~19)
    // lex; codegen emits a BigInt for anything beyond 2**53. Hex/oct/bin bases
    // too (`0xFF`, `0o17`, `0b101`). Truly-unbounded (>i128) is a documented
    // residual. Base prefixes are matched before the decimal rule so `0x…`
    // isn't split as Integer(0) + identifier.
    #[regex(r"0[xX][0-9a-fA-F][0-9a-fA-F_]*", |lex| i128::from_str_radix(&lex.slice()[2..].replace('_', ""), 16).ok())]
    #[regex(r"0[oO][0-7][0-7_]*", |lex| i128::from_str_radix(&lex.slice()[2..].replace('_', ""), 8).ok())]
    #[regex(r"0[bB][01][01_]*", |lex| i128::from_str_radix(&lex.slice()[2..].replace('_', ""), 2).ok())]
    #[regex(r"[0-9][0-9_]*", |lex| lex.slice().replace('_', "").parse::<i128>().ok())]
    Integer(i128),

    #[regex(r"[0-9][0-9_]*\.[0-9][0-9_]*([eE][+-]?[0-9]+)?", |lex| lex.slice().replace('_', "").parse::<f64>().ok())]
    #[regex(r"[0-9][0-9_]*[eE][+-]?[0-9]+", |lex| lex.slice().replace('_', "").parse::<f64>().ok())]
    // #208: trailing-dot floats (`1.`, `1.e5`) — a `.` with no fractional
    // digit. Previously `1.` lexed as Integer(1) + Dot, so `1. / 3` and
    // `a, b = -1., 1.` failed with "Expected identifier, found ...". A digit
    // can't begin an identifier, so `5.` is unambiguously a float (CPython
    // treats `5.bit_length()` as a syntax error for the same reason). logos
    // longest-match keeps `1.5` on the fractional regex above. `.parse::<f64>`
    // rejects a bare `1.` so append "0" to the fraction before parsing.
    #[regex(r"[0-9][0-9_]*\.([eE][+-]?[0-9]+)?", |lex| {
        let s = lex.slice().replace('_', "");
        let s = if let Some(rest) = s.strip_suffix('.') {
            format!("{}.0", rest)
        } else {
            s.replacen('.', ".0", 1) // `1.e5` -> `1.0e5`
        };
        s.parse::<f64>().ok()
    })]
    // #208: leading-dot floats (`.5`, `.5e3`). A `.` followed by a digit is
    // never member access (identifiers can't start with a digit), so this is
    // unambiguous. `parse::<f64>` accepts `.5`, so no massaging needed.
    #[regex(r"\.[0-9][0-9_]*([eE][+-]?[0-9]+)?", |lex| lex.slice().replace('_', "").parse::<f64>().ok())]
    Float(f64),

    // #283: imaginary literals — a numeric literal with a trailing `j`/`J`
    // (`2j`, `3.5j`, `1e3j`, `.5j`, `1.j`). Logos longest-match prefers these
    // over Integer/Float since the `[jJ]` suffix extends the match by one char.
    // The stored f64 is the magnitude of the imaginary part; the codegen wraps
    // it in a runtime complex value. `cmath` remains out of scope (#283).
    #[regex(r"[0-9][0-9_]*\.[0-9][0-9_]*([eE][+-]?[0-9]+)?[jJ]", parse_imag)]
    #[regex(r"[0-9][0-9_]*[eE][+-]?[0-9]+[jJ]", parse_imag)]
    #[regex(r"[0-9][0-9_]*\.([eE][+-]?[0-9]+)?[jJ]", parse_imag)]
    #[regex(r"\.[0-9][0-9_]*([eE][+-]?[0-9]+)?[jJ]", parse_imag)]
    #[regex(r"[0-9][0-9_]*[jJ]", parse_imag)]
    Imaginary(f64),

    // Triple-double-quoted string (must come before single-double)
    #[regex(r#"""""#, lex_triple_double_string)]
    // Triple-single-quoted string
    #[regex(r"'''", lex_triple_single_string)]
    // #100: raw strings — the r/R prefix is consumed (not left to lex as
    // an identifier) and the body is kept VERBATIM (no escape decoding;
    // the backslash still prevents the quote from terminating the
    // literal, exactly like CPython). Triple-quoted raw forms first so
    // `r'''` doesn't lex as an empty `r''`.
    #[regex(r#"[rR]""""#, lex_triple_double_raw_string)]
    #[regex(r"[rR]'''", lex_triple_single_raw_string)]
    #[regex(r#"[rR]"([^"\\]|\\.)*""#, |lex| {
        let s = lex.slice();
        Some(s[2..s.len()-1].to_string())
    })]
    #[regex(r#"[rR]'([^'\\]|\\.)*'"#, |lex| {
        let s = lex.slice();
        Some(s[2..s.len()-1].to_string())
    })]
    // Double-quoted string (#95: escapes decoded at the lexer boundary)
    #[regex(r#""([^"\\]|\\.)*""#, |lex| {
        let s = lex.slice();
        Some(decode_py_escapes(&s[1..s.len()-1]))
    })]
    // Single-quoted string
    #[regex(r#"'([^'\\]|\\.)*'"#, |lex| {
        let s = lex.slice();
        Some(decode_py_escapes(&s[1..s.len()-1]))
    })]
    String_(String),

    // Triple-quoted f-strings
    #[regex(r#"f""""#, lex_triple_double_fstring)]
    #[regex(r"f'''", lex_triple_single_fstring)]
    // Single-line f-strings — custom scanner (PBT-3): PEP 701 quote reuse
    // inside {...} expression parts needs brace-depth tracking a regex
    // cannot express. The `f"""`/`f'''` triple patterns above are longer
    // matches, so logos still prefers them.
    #[regex(r#"f""#, lex_double_quote_fstring)]
    #[regex(r"f'", lex_single_quote_fstring)]
    FString(String),

    // ── Identifiers ──────────────────────────────────────
    // PEP 3131: identifiers are `XID_Start XID_Continue*` (plus a leading
    // `_`, which is XID_Continue but not XID_Start). ASCII letters/digits/`_`
    // are a subset of XID, so ordinary names still match; non-ASCII names
    // (`übervar`, `café`, `变量`) now lex instead of hitting the
    // "Unexpected character" path. JS permits the same XID identifier set,
    // so the codegen passes these through verbatim. (Caveat: CPython NFKC-
    // normalizes identifiers before comparison; we keep them verbatim, so two
    // names that are distinct code points but NFKC-equal — e.g. the `ﬀ`
    // ligature vs `ff` — stay distinct here. Rare in practice.)
    #[regex(r"[_\p{XID_Start}][\p{XID_Continue}]*", |lex| lex.slice().to_string(), priority = 1)]
    Identifier(String),

    // ── Operators ─────────────────────────────────────────
    #[token("+")]
    Plus,
    #[token("-")]
    Minus,
    #[token("*")]
    Star,
    #[token("**")]
    DoubleStar,
    #[token("/")]
    Slash,
    #[token("//")]
    DoubleSlash,
    #[token("%")]
    Percent,
    #[token("@")]
    At,

    #[token("==")]
    EqEq,
    #[token("!=")]
    NotEq,
    #[token("<")]
    Lt,
    #[token("<=")]
    LtEq,
    #[token(">")]
    Gt,
    #[token(">=")]
    GtEq,

    #[token("=")]
    Eq,
    #[token("+=")]
    PlusEq,
    #[token("-=")]
    MinusEq,
    #[token("*=")]
    StarEq,
    #[token("/=")]
    SlashEq,
    #[token("//=")]
    DoubleSlashEq,
    #[token("%=")]
    PercentEq,
    #[token("**=")]
    DoubleStarEq,
    #[token("&=")]
    AmpEq,
    #[token("|=")]
    PipeEq,
    #[token("^=")]
    CaretEq,
    #[token("<<=")]
    ShiftLeftEq,
    #[token(">>=")]
    ShiftRightEq,

    #[token(":=")]
    ColonEq,

    #[token("&")]
    Amp,
    #[token("|")]
    Pipe,
    #[token("^")]
    Caret,
    #[token("~")]
    Tilde,
    #[token("<<")]
    ShiftLeft,
    #[token(">>")]
    ShiftRight,

    #[token("->")]
    Arrow,

    #[token("?.")]
    QuestionDot,
    #[token("??")]
    QuestionQuestion,
    #[token("|>")]
    PipeGt,

    // ── Delimiters ────────────────────────────────────────
    #[token("(")]
    LParen,
    #[token(")")]
    RParen,
    #[token("[")]
    LBracket,
    #[token("]")]
    RBracket,
    #[token("{")]
    LBrace,
    #[token("}")]
    RBrace,
    #[token(",")]
    Comma,
    #[token(":")]
    Colon,
    #[token(";")]
    Semicolon,
    #[token(".")]
    Dot,
    #[token("...")]
    Ellipsis,

    // ── Comments ──────────────────────────────────────────
    #[regex(r"#[^\n]*")]
    Comment,

    // ── Synthetic tokens (injected by indent preprocessor) ──
    Newline,
    Indent,
    Dedent,

    // End of file
    Eof,
}

impl std::fmt::Display for Token {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Token::Integer(n) => write!(f, "{}", n),
            Token::Float(n) => write!(f, "{}", n),
            Token::Imaginary(n) => write!(f, "{}j", n),
            Token::String_(s) => write!(f, "\"{}\"", s),
            Token::FString(s) => write!(f, "f\"{}\"", s),
            Token::Identifier(s) => write!(f, "{}", s),
            Token::True_ => write!(f, "True"),
            Token::False => write!(f, "False"),
            Token::None_ => write!(f, "None"),
            Token::And => write!(f, "and"),
            Token::Or => write!(f, "or"),
            Token::Not => write!(f, "not"),
            Token::If => write!(f, "if"),
            Token::Elif => write!(f, "elif"),
            Token::Else => write!(f, "else"),
            Token::While => write!(f, "while"),
            Token::For => write!(f, "for"),
            Token::In => write!(f, "in"),
            Token::Def => write!(f, "def"),
            Token::Return => write!(f, "return"),
            Token::Class => write!(f, "class"),
            Token::Pass => write!(f, "pass"),
            Token::Break => write!(f, "break"),
            Token::Continue => write!(f, "continue"),
            Token::Import => write!(f, "import"),
            Token::From => write!(f, "from"),
            Token::As => write!(f, "as"),
            Token::Try => write!(f, "try"),
            Token::Except => write!(f, "except"),
            Token::Finally => write!(f, "finally"),
            Token::Raise => write!(f, "raise"),
            Token::With => write!(f, "with"),
            Token::Assert => write!(f, "assert"),
            Token::Async => write!(f, "async"),
            Token::Await => write!(f, "await"),
            Token::Yield => write!(f, "yield"),
            Token::Global => write!(f, "global"),
            Token::Nonlocal => write!(f, "nonlocal"),
            Token::Del => write!(f, "del"),
            Token::Lambda => write!(f, "lambda"),
            Token::Is => write!(f, "is"),
            Token::Match => write!(f, "match"),
            Token::Plus => write!(f, "+"),
            Token::Minus => write!(f, "-"),
            Token::Star => write!(f, "*"),
            Token::DoubleStar => write!(f, "**"),
            Token::Slash => write!(f, "/"),
            Token::DoubleSlash => write!(f, "//"),
            Token::Percent => write!(f, "%"),
            Token::At => write!(f, "@"),
            Token::EqEq => write!(f, "=="),
            Token::NotEq => write!(f, "!="),
            Token::Lt => write!(f, "<"),
            Token::LtEq => write!(f, "<="),
            Token::Gt => write!(f, ">"),
            Token::GtEq => write!(f, ">="),
            Token::Eq => write!(f, "="),
            Token::PlusEq => write!(f, "+="),
            Token::MinusEq => write!(f, "-="),
            Token::StarEq => write!(f, "*="),
            Token::SlashEq => write!(f, "/="),
            Token::DoubleSlashEq => write!(f, "//="),
            Token::PercentEq => write!(f, "%="),
            Token::DoubleStarEq => write!(f, "**="),
            Token::AmpEq => write!(f, "&="),
            Token::PipeEq => write!(f, "|="),
            Token::CaretEq => write!(f, "^="),
            Token::ShiftLeftEq => write!(f, "<<="),
            Token::ShiftRightEq => write!(f, ">>="),
            Token::ColonEq => write!(f, ":="),
            Token::Amp => write!(f, "&"),
            Token::Pipe => write!(f, "|"),
            Token::Caret => write!(f, "^"),
            Token::Tilde => write!(f, "~"),
            Token::ShiftLeft => write!(f, "<<"),
            Token::ShiftRight => write!(f, ">>"),
            Token::Arrow => write!(f, "->"),
            Token::QuestionDot => write!(f, "?."),
            Token::QuestionQuestion => write!(f, "??"),
            Token::PipeGt => write!(f, "|>"),
            Token::LParen => write!(f, "("),
            Token::RParen => write!(f, ")"),
            Token::LBracket => write!(f, "["),
            Token::RBracket => write!(f, "]"),
            Token::LBrace => write!(f, "{{"),
            Token::RBrace => write!(f, "}}"),
            Token::Comma => write!(f, ","),
            Token::Colon => write!(f, ":"),
            Token::Semicolon => write!(f, ";"),
            Token::Dot => write!(f, "."),
            Token::Ellipsis => write!(f, "..."),
            Token::Comment => write!(f, "#comment"),
            Token::Newline => write!(f, "NEWLINE"),
            Token::Indent => write!(f, "INDENT"),
            Token::Dedent => write!(f, "DEDENT"),
            Token::Eof => write!(f, "EOF"),
        }
    }
}
