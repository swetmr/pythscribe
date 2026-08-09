pub mod format_spec;
pub mod parser;

pub use parser::Parser;

/// Parse a source string into a Module AST.
pub fn parse(source: &str) -> Result<pyths_syntax::ast::Module, Vec<ParseError>> {
    let lex_result = pyths_lexer::lex_recovering(source);
    let lex_errors: Vec<ParseError> = lex_result
        .errors
        .iter()
        .map(|e| ParseError {
            message: e.message.clone(),
            span: pyths_syntax::span::Span::new(e.span.start, e.span.end),
            notes: vec![],
        })
        .collect();
    let mut parser = Parser::new(lex_result.tokens, source);
    match parser.parse_module() {
        Ok(module) if lex_errors.is_empty() => Ok(module),
        Ok(_) => Err(lex_errors),
        Err(mut parse_errors) => {
            let mut all = lex_errors;
            all.append(&mut parse_errors);
            Err(all)
        }
    }
}

#[derive(Debug, Clone)]
pub struct ParseError {
    pub message: String,
    pub span: pyths_syntax::span::Span,
    pub notes: Vec<String>,
}

impl std::fmt::Display for ParseError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "Parse error at {}-{}: {}",
            self.span.start, self.span.end, self.message
        )?;
        for note in &self.notes {
            write!(f, "\n  note: {}", note)?;
        }
        Ok(())
    }
}
