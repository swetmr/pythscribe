//! PEP 3101 format-spec parser + JS lowering.
//!
//! Format spec grammar:
//! ```text
//! format_spec     ::= [[fill]align][sign]["#"]["0"][width][grouping_option]["." precision][type]
//! fill            ::= <any character>
//! align           ::= "<" | ">" | "=" | "^"
//! sign            ::= "+" | "-" | " "
//! width           ::= digit+
//! grouping_option ::= "_" | ","
//! precision       ::= digit+
//! type            ::= "b" | "c" | "d" | "e" | "E" | "f" | "F" | "g" | "G"
//!                   | "n" | "o" | "s" | "x" | "X" | "%"
//! ```
//!
//! Used by f-string lowering in the parser. Each spec lowers to a JS
//! expression that wraps the formatted value.

use pyths_syntax::ast::{Expr, ExprKind};
use pyths_syntax::span::Span;

/// Parsed `format_spec` mini-language. All fields optional; the empty
/// spec parses to a default [`FormatSpec`] with no transformation.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct FormatSpec {
    pub fill: Option<char>,
    pub align: Option<Align>,
    pub sign: Option<Sign>,
    pub alt_form: bool,
    pub zero_pad: bool,
    pub width: Option<u32>,
    pub grouping: Option<Grouping>,
    pub precision: Option<u32>,
    pub ty: Option<FormatType>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Align {
    Left,
    Right,
    Center,
    /// `=` — pad between sign and digits. Numeric only.
    PadAfterSign,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Sign {
    Plus,
    Minus,
    Space,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Grouping {
    Comma,
    Underscore,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FormatType {
    Binary,       // b
    Char,         // c
    Decimal,      // d
    ExpLower,     // e
    ExpUpper,     // E
    FixedLower,   // f
    FixedUpper,   // F (treated like f)
    GeneralLower, // g
    GeneralUpper, // G
    Octal,        // o
    String,       // s
    HexLower,     // x
    HexUpper,     // X
    Percent,      // %
    /// `n` is locale-aware decimal; we approximate as `,` grouping +
    /// decimal type.
    LocaleDecimal, // n
}

/// Parse a format-spec string. Returns None for malformed input — the
/// caller falls back to passing the value through unformatted (matches
/// pre-existing behavior).
pub fn parse(s: &str) -> Option<FormatSpec> {
    let chars: Vec<char> = s.chars().collect();
    let mut i = 0;
    let mut spec = FormatSpec::default();

    // [[fill]align] — fill requires a following align char in {<,>,=,^}
    // and fill is "any character" (PEP 3101). Must look ahead.
    if chars.len() >= 2 && is_align(chars[1]) {
        spec.fill = Some(chars[0]);
        spec.align = Some(parse_align(chars[1])?);
        i = 2;
    } else if !chars.is_empty() && is_align(chars[0]) {
        spec.align = Some(parse_align(chars[0])?);
        i = 1;
    }

    // [sign]
    if i < chars.len() {
        match chars[i] {
            '+' => {
                spec.sign = Some(Sign::Plus);
                i += 1;
            }
            '-' => {
                spec.sign = Some(Sign::Minus);
                i += 1;
            }
            ' ' => {
                spec.sign = Some(Sign::Space);
                i += 1;
            }
            _ => {}
        }
    }

    // ["#"]
    if i < chars.len() && chars[i] == '#' {
        spec.alt_form = true;
        i += 1;
    }

    // ["0"] (zero-pad). Only valid for numeric types; we record and let
    // the lowering decide.
    if i < chars.len() && chars[i] == '0' {
        // distinguish 0 as zero-pad-flag vs 0 as start of width.
        // PEP 3101: a leading '0' before the width means zero-padding.
        // We treat any '0' immediately followed by another digit as
        // zero-pad + width=that_digits.
        spec.zero_pad = true;
        i += 1;
    }

    // [width]
    let mut width_digits = String::new();
    while i < chars.len() && chars[i].is_ascii_digit() {
        width_digits.push(chars[i]);
        i += 1;
    }
    if !width_digits.is_empty() {
        spec.width = width_digits.parse::<u32>().ok();
    }

    // [grouping_option]
    if i < chars.len() {
        match chars[i] {
            ',' => {
                spec.grouping = Some(Grouping::Comma);
                i += 1;
            }
            '_' => {
                spec.grouping = Some(Grouping::Underscore);
                i += 1;
            }
            _ => {}
        }
    }

    // ["." precision]
    if i < chars.len() && chars[i] == '.' {
        i += 1;
        let mut prec_digits = String::new();
        while i < chars.len() && chars[i].is_ascii_digit() {
            prec_digits.push(chars[i]);
            i += 1;
        }
        if prec_digits.is_empty() {
            return None;
        }
        spec.precision = prec_digits.parse::<u32>().ok();
    }

    // [type]
    if i < chars.len() {
        spec.ty = parse_type(chars[i]);
        spec.ty?;
        i += 1;
    }

    if i != chars.len() {
        // unknown trailing characters → bail
        return None;
    }
    Some(spec)
}

fn is_align(c: char) -> bool {
    matches!(c, '<' | '>' | '=' | '^')
}

fn parse_align(c: char) -> Option<Align> {
    Some(match c {
        '<' => Align::Left,
        '>' => Align::Right,
        '^' => Align::Center,
        '=' => Align::PadAfterSign,
        _ => return None,
    })
}

fn parse_type(c: char) -> Option<FormatType> {
    Some(match c {
        'b' => FormatType::Binary,
        'c' => FormatType::Char,
        'd' => FormatType::Decimal,
        'e' => FormatType::ExpLower,
        'E' => FormatType::ExpUpper,
        'f' => FormatType::FixedLower,
        'F' => FormatType::FixedUpper,
        'g' => FormatType::GeneralLower,
        'G' => FormatType::GeneralUpper,
        'n' => FormatType::LocaleDecimal,
        'o' => FormatType::Octal,
        's' => FormatType::String,
        'x' => FormatType::HexLower,
        'X' => FormatType::HexUpper,
        '%' => FormatType::Percent,
        _ => return None,
    })
}

/// Lower a parsed FormatSpec applied to `expr` into a JS expression.
/// Callers pass through any spec that returns `None` (treats it as a
/// no-op formatting).
pub fn lower(spec: &FormatSpec, expr: Expr, span: Span) -> Expr {
    // The lowering composes a chain of helper calls. To keep emitted
    // code legible, we build it in stages:
    //   1. Apply type conversion (toFixed, toExponential, toString(radix)…)
    //   2. Apply alt-form prefix (#)
    //   3. Apply sign (+ / space)
    //   4. Apply grouping (, / _)
    //   5. Apply width + fill + align (padStart/padEnd)
    //
    // Steps that have no effect when the corresponding spec field is
    // unset compile to the identity. We delegate the orchestration to
    // a single runtime helper `pyFormatSpec(value, opts)` so the JS
    // remains compact. The opts object encodes the spec.
    let opts = build_opts_object(spec, span);
    let helper = ident_expr("pyFormatSpec", span);
    Expr {
        kind: ExprKind::Call {
            func: Box::new(helper),
            args: vec![expr, opts],
            kwargs: vec![],
            optional: false,
        },
        span,
    }
}

/// Returns true if the spec is a no-op (empty spec). The caller can
/// short-circuit and emit the bare expression.
pub fn is_noop(spec: &FormatSpec) -> bool {
    *spec == FormatSpec::default()
}

fn ident_expr(name: &str, span: Span) -> Expr {
    Expr {
        kind: ExprKind::Name(name.to_string()),
        span,
    }
}

fn str_lit(value: &str, span: Span) -> Expr {
    Expr {
        kind: ExprKind::StringLiteral(value.to_string()),
        span,
    }
}

fn int_lit(value: i128, span: Span) -> Expr {
    Expr {
        kind: ExprKind::IntLiteral(value),
        span,
    }
}

fn bool_lit(value: bool, span: Span) -> Expr {
    Expr {
        kind: ExprKind::BoolLiteral(value),
        span,
    }
}

/// Build a JS object literal carrying the spec fields. The runtime
/// helper inspects this object and applies the formatting steps.
fn build_opts_object(spec: &FormatSpec, span: Span) -> Expr {
    use pyths_syntax::ast::DictItem;
    let mut items = Vec::<DictItem>::new();

    if let Some(c) = spec.fill {
        items.push(DictItem::KeyValue {
            key: str_lit("fill", span),
            value: str_lit(&c.to_string(), span),
        });
    }
    if let Some(a) = spec.align {
        items.push(DictItem::KeyValue {
            key: str_lit("align", span),
            value: str_lit(
                match a {
                    Align::Left => "<",
                    Align::Right => ">",
                    Align::Center => "^",
                    Align::PadAfterSign => "=",
                },
                span,
            ),
        });
    }
    if let Some(s) = spec.sign {
        items.push(DictItem::KeyValue {
            key: str_lit("sign", span),
            value: str_lit(
                match s {
                    Sign::Plus => "+",
                    Sign::Minus => "-",
                    Sign::Space => " ",
                },
                span,
            ),
        });
    }
    if spec.alt_form {
        items.push(DictItem::KeyValue {
            key: str_lit("alt", span),
            value: bool_lit(true, span),
        });
    }
    if spec.zero_pad {
        items.push(DictItem::KeyValue {
            key: str_lit("zero", span),
            value: bool_lit(true, span),
        });
    }
    if let Some(w) = spec.width {
        items.push(DictItem::KeyValue {
            key: str_lit("width", span),
            value: int_lit(w as i128, span),
        });
    }
    if let Some(g) = spec.grouping {
        items.push(DictItem::KeyValue {
            key: str_lit("grouping", span),
            value: str_lit(
                match g {
                    Grouping::Comma => ",",
                    Grouping::Underscore => "_",
                },
                span,
            ),
        });
    }
    if let Some(p) = spec.precision {
        items.push(DictItem::KeyValue {
            key: str_lit("precision", span),
            value: int_lit(p as i128, span),
        });
    }
    if let Some(t) = spec.ty {
        items.push(DictItem::KeyValue {
            key: str_lit("type", span),
            value: str_lit(
                match t {
                    FormatType::Binary => "b",
                    FormatType::Char => "c",
                    FormatType::Decimal => "d",
                    FormatType::ExpLower => "e",
                    FormatType::ExpUpper => "E",
                    FormatType::FixedLower => "f",
                    FormatType::FixedUpper => "F",
                    FormatType::GeneralLower => "g",
                    FormatType::GeneralUpper => "G",
                    FormatType::LocaleDecimal => "n",
                    FormatType::Octal => "o",
                    FormatType::String => "s",
                    FormatType::HexLower => "x",
                    FormatType::HexUpper => "X",
                    FormatType::Percent => "%",
                },
                span,
            ),
        });
    }

    Expr {
        kind: ExprKind::Dict { items },
        span,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_empty() {
        assert_eq!(parse(""), Some(FormatSpec::default()));
    }

    #[test]
    fn parse_decimal_precision() {
        let s = parse(".2f").unwrap();
        assert_eq!(s.precision, Some(2));
        assert_eq!(s.ty, Some(FormatType::FixedLower));
    }

    #[test]
    fn parse_zero_padded_int() {
        let s = parse("02d").unwrap();
        assert!(s.zero_pad);
        assert_eq!(s.width, Some(2));
        assert_eq!(s.ty, Some(FormatType::Decimal));
    }

    #[test]
    fn parse_align_width() {
        let s = parse(">10").unwrap();
        assert_eq!(s.align, Some(Align::Right));
        assert_eq!(s.width, Some(10));
    }

    #[test]
    fn parse_fill_align_width() {
        let s = parse("*^20").unwrap();
        assert_eq!(s.fill, Some('*'));
        assert_eq!(s.align, Some(Align::Center));
        assert_eq!(s.width, Some(20));
    }

    #[test]
    fn parse_thousands() {
        let s = parse(",").unwrap();
        assert_eq!(s.grouping, Some(Grouping::Comma));
    }

    #[test]
    fn parse_percent() {
        let s = parse(".1%").unwrap();
        assert_eq!(s.precision, Some(1));
        assert_eq!(s.ty, Some(FormatType::Percent));
    }

    #[test]
    fn parse_hex_alt() {
        let s = parse("#x").unwrap();
        assert!(s.alt_form);
        assert_eq!(s.ty, Some(FormatType::HexLower));
    }

    #[test]
    fn parse_sign_plus() {
        let s = parse("+.2f").unwrap();
        assert_eq!(s.sign, Some(Sign::Plus));
        assert_eq!(s.precision, Some(2));
    }

    #[test]
    fn parse_garbage_returns_none() {
        assert!(parse("foo").is_none());
        assert!(parse("..").is_none());
        // Unknown type char
        assert!(parse(".2q").is_none());
    }

    #[test]
    fn is_noop_default() {
        assert!(is_noop(&FormatSpec::default()));
        assert!(!is_noop(&FormatSpec {
            precision: Some(2),
            ..FormatSpec::default()
        }));
    }
}
