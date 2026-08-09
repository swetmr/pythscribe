use std::cell::Cell;

use pyths_lexer::indent::SpannedToken;
use pyths_lexer::Token;
use pyths_syntax::ast::*;
use pyths_syntax::operators::*;
use pyths_syntax::span::Span;

use crate::ParseError;

/// Maximum nesting depth for recursive-descent parsing — both nested
/// expressions (`((((…))))`, `[[[…]]]`, nested f-strings) and nested statement
/// blocks (`if:` inside `if:` …). Sits in the same ballpark as CPython's
/// default recursion limit (~1000) while remaining far above any realistic
/// program (hand- or machine-written source nests well under ~30). The compile
/// driver runs on a large-stack worker thread (see `pyths_cli::main`), so this
/// bound is reached and reported as a clean diagnostic long before the native
/// stack could overflow.
pub const MAX_PARSE_DEPTH: usize = 1000;

thread_local! {
    /// Current recursive-descent nesting depth for the active thread. Held in a
    /// thread-local (rather than a `Parser` field) so it survives the nested
    /// sub-`Parser`s spun up for f-string interpolations — a fresh `Parser`
    /// shares this counter, so `f'{f'{…}'}'` can't reset the depth and slip
    /// past the guard.
    static PARSE_DEPTH: Cell<usize> = const { Cell::new(0) };
}

/// RAII depth counter. Increments `PARSE_DEPTH` on construction and decrements
/// it on drop, so the count stays correct across every exit path — normal
/// return, `?`-propagated `Err`, and the nested sub-parser boundary alike.
struct DepthGuard;

impl DepthGuard {
    /// Enter one nesting level. Returns `None` (leaving the counter unchanged)
    /// when the bound is exceeded, so the caller can emit a clean parse error
    /// instead of recursing into a stack overflow.
    fn enter() -> Option<DepthGuard> {
        let depth = PARSE_DEPTH.with(|d| {
            let next = d.get() + 1;
            d.set(next);
            next
        });
        if depth > MAX_PARSE_DEPTH {
            PARSE_DEPTH.with(|d| d.set(d.get() - 1));
            None
        } else {
            Some(DepthGuard)
        }
    }
}

impl Drop for DepthGuard {
    fn drop(&mut self) {
        PARSE_DEPTH.with(|d| d.set(d.get().saturating_sub(1)));
    }
}

pub struct Parser<'a> {
    tokens: Vec<SpannedToken>,
    pos: usize,
    #[allow(dead_code)]
    source: &'a str,
    errors: Vec<ParseError>,
    /// B12: nesting depth of enclosing `def`/`async def` bodies. `return` is
    /// only legal when this is > 0 (Python: "'return' outside function").
    function_depth: usize,
    /// B12: nesting depth of enclosing `for`/`while` bodies WITHIN the current
    /// function. `break`/`continue` are only legal when this is > 0. Reset to 0
    /// when entering a nested function or class body — a loop in an enclosing
    /// scope does not make `break` legal inside a nested def/class.
    loop_depth: usize,
}

impl<'a> Parser<'a> {
    pub fn new(tokens: Vec<SpannedToken>, source: &'a str) -> Self {
        Self {
            tokens,
            pos: 0,
            source,
            errors: Vec::new(),
            function_depth: 0,
            loop_depth: 0,
        }
    }

    // ── Token access ──────────────────────────────────────

    fn peek(&self) -> &Token {
        self.tokens
            .get(self.pos)
            .map(|t| &t.token)
            .unwrap_or(&Token::Eof)
    }

    /// `true` when the upcoming tokens introduce a comprehension clause —
    /// either `for ...` or `async for ...`. Used at the list / set /
    /// dict / generator-expression dispatch sites to decide between a
    /// plain literal and a comprehension.
    fn at_comprehension_start(&self) -> bool {
        let here = self.peek();
        if here == &Token::For {
            return true;
        }
        if here == &Token::Async {
            // Peek ahead one token; `async for` is the only valid form
            // here. Bare `async` in this position is a parse error.
            return self
                .tokens
                .get(self.pos + 1)
                .map(|t| t.token == Token::For)
                .unwrap_or(false);
        }
        false
    }

    fn peek_span(&self) -> Span {
        self.tokens
            .get(self.pos)
            .map(|t| Span::new(t.span.start, t.span.end))
            .unwrap_or(Span::dummy())
    }

    fn prev_span(&self) -> Span {
        if self.pos > 0 {
            self.tokens
                .get(self.pos - 1)
                .map(|t| Span::new(t.span.start, t.span.end))
                .unwrap_or(Span::dummy())
        } else {
            Span::dummy()
        }
    }

    fn advance(&mut self) -> SpannedToken {
        let tok = self.tokens.get(self.pos).cloned().unwrap_or(SpannedToken {
            token: Token::Eof,
            span: 0..0,
        });
        self.pos += 1;
        tok
    }

    fn expect(&mut self, expected: &Token) -> Result<SpannedToken, ParseError> {
        if self.peek() == expected {
            Ok(self.advance())
        } else {
            Err(self.error(format!("Expected {}, found {}", expected, self.peek())))
        }
    }

    fn expect_identifier(&mut self) -> Result<(String, Span), ParseError> {
        match self.peek().clone() {
            Token::Identifier(name) => {
                let span = self.peek_span();
                self.advance();
                Ok((name, span))
            }
            _ => Err(self.error(format!("Expected identifier, found {}", self.peek()))),
        }
    }

    /// Attribute names after `.` / `?.` may be any Python keyword — a JS-
    /// interop deviation (`p.finally(cb)`, `s.match(re)`, `params.class`).
    /// JS objects routinely expose such members and ES5+ allows reserved
    /// words as property names. Keywords stay reserved in every other
    /// position; this fires only in postfix attribute position
    /// (parse_postfix), never for bindings.
    fn expect_attr_name(&mut self) -> Result<(String, Span), ParseError> {
        if let Token::Identifier(name) = self.peek() {
            let name = name.clone();
            let span = self.peek_span();
            self.advance();
            return Ok((name, span));
        }
        let kw = match self.peek() {
            Token::False => "False",
            Token::None_ => "None",
            Token::True_ => "True",
            Token::And => "and",
            Token::As => "as",
            Token::Assert => "assert",
            Token::Async => "async",
            Token::Await => "await",
            Token::Break => "break",
            Token::Class => "class",
            Token::Continue => "continue",
            Token::Def => "def",
            Token::Del => "del",
            Token::Elif => "elif",
            Token::Else => "else",
            Token::Except => "except",
            Token::Finally => "finally",
            Token::For => "for",
            Token::From => "from",
            Token::Global => "global",
            Token::If => "if",
            Token::Import => "import",
            Token::In => "in",
            Token::Is => "is",
            Token::Lambda => "lambda",
            Token::Match => "match",
            Token::Nonlocal => "nonlocal",
            Token::Not => "not",
            Token::Or => "or",
            Token::Pass => "pass",
            Token::Raise => "raise",
            Token::Return => "return",
            Token::Try => "try",
            Token::While => "while",
            Token::With => "with",
            Token::Yield => "yield",
            _ => {
                return Err(self.error(format!("Expected identifier, found {}", self.peek())));
            }
        };
        let span = self.peek_span();
        self.advance();
        Ok((kw.to_string(), span))
    }

    fn at(&self, token: &Token) -> bool {
        std::mem::discriminant(self.peek()) == std::mem::discriminant(token)
    }

    fn at_any(&self, tokens: &[Token]) -> bool {
        tokens.iter().any(|t| self.at(t))
    }

    fn eat(&mut self, token: &Token) -> bool {
        if self.at(token) {
            self.advance();
            true
        } else {
            false
        }
    }

    fn skip_newlines(&mut self) {
        while self.at(&Token::Newline) {
            self.advance();
        }
    }

    /// Consume the terminator that ends a SIMPLE statement.
    ///
    /// A simple statement ends at NEWLINE, at `;` (inside a one-line suite),
    /// at DEDENT, or at EOF. Nothing else may follow it on the line.
    ///
    /// Every simple-statement parser used to end with a bare
    /// `self.eat(&Token::Newline)`, which is a NO-OP when the next token is not
    /// a newline. The statement loop then simply started a fresh statement
    /// wherever the last one stopped, so the parser silently accepted token
    /// soup with no separator at all: `x = 1 y = 2`, `pass pass`, `a b`,
    /// `import os x = 1` all "parsed" (as two statements). grammar/pyths.lark
    /// requires `simple_stmt: small_stmt _NEWLINE` and correctly rejected them,
    /// so this was a PARSER over-acceptance, not a grammar gap. Found by
    /// scripts/grammar-fuzz.py (direction B).
    fn expect_stmt_end(&mut self) -> Result<(), ParseError> {
        if self.at(&Token::Newline) {
            self.advance();
            return Ok(());
        }
        // Left for the caller to handle; they delimit the statement too.
        if self.at(&Token::Semicolon) || self.at(&Token::Dedent) || self.at(&Token::Eof) {
            return Ok(());
        }
        Err(self.error(format!(
            "Unexpected token after statement: {}; a statement ends at a              newline, a `;` (inside a one-line suite), or the end of the block",
            self.peek()
        )))
    }

    fn error(&self, message: String) -> ParseError {
        ParseError {
            message,
            span: self.peek_span(),
            notes: vec![],
        }
    }

    /// Like `error`, but points at a caller-supplied span rather than the
    /// current token — used where the offending construct was already consumed
    /// (e.g. B12 parameter-list diagnostics point at the parameter itself).
    fn error_at(&self, message: String, span: Span) -> ParseError {
        ParseError {
            message,
            span,
            notes: vec![],
        }
    }

    fn error_with_notes(&self, message: String, notes: Vec<String>) -> ParseError {
        ParseError {
            message,
            span: self.peek_span(),
            notes,
        }
    }

    /// Clean diagnostic emitted when the recursive-descent depth guard trips,
    /// in place of a native stack overflow. `what` names the construct
    /// (`"expression"` / `"block"`) for a more helpful message.
    fn error_too_deep(&self, what: &str) -> ParseError {
        self.error_with_notes(
            format!(
                "{} nested too deeply (exceeds the maximum of {} levels)",
                what, MAX_PARSE_DEPTH
            ),
            vec![format!(
                "simplify or split the deeply-nested {} — this limit guards against \
                 stack exhaustion on pathological input",
                what
            )],
        )
    }

    /// Expect a colon in a block-introducing statement context, with a helpful hint if missing.
    fn expect_colon_for(&mut self, keyword: &str) -> Result<SpannedToken, ParseError> {
        if self.peek() == &Token::Colon {
            Ok(self.advance())
        } else {
            Err(self.error_with_notes(
                format!("Expected ':', found {}", self.peek()),
                vec![format!(
                    "add ':' after the '{}' statement to start a block",
                    keyword
                )],
            ))
        }
    }

    /// Expect an RParen with a helpful hint if missing.
    fn expect_rparen(&mut self) -> Result<SpannedToken, ParseError> {
        if self.peek() == &Token::RParen {
            Ok(self.advance())
        } else {
            Err(self.error_with_notes(
                format!("Expected ')', found {}", self.peek()),
                vec!["you may have an unmatched '(' — add the closing ')'".to_string()],
            ))
        }
    }

    /// Skip tokens until we reach a newline, dedent, or EOF — used for block-level recovery.
    fn synchronize_block(&mut self) {
        loop {
            match self.peek() {
                Token::Newline => {
                    self.advance();
                    break;
                }
                Token::Dedent | Token::Eof => break,
                _ => {
                    self.advance();
                }
            }
        }
    }

    // ── Module ────────────────────────────────────────────

    pub fn parse_module(&mut self) -> Result<Module, Vec<ParseError>> {
        let start = self.peek_span();
        self.skip_newlines();
        let mut body = Vec::new();
        let mut errors = Vec::new();

        while !self.at(&Token::Eof) {
            self.skip_newlines();
            if self.at(&Token::Eof) {
                break;
            }
            match self.parse_stmt_line(&mut body) {
                Ok(()) => {}
                Err(e) => {
                    errors.push(e);
                    // Recovery: skip to next newline
                    while !self.at(&Token::Newline) && !self.at(&Token::Eof) {
                        self.advance();
                    }
                    if self.at(&Token::Newline) {
                        self.advance();
                    }
                }
            }
        }

        // Merge block-level recovery errors
        errors.append(&mut self.errors);

        if errors.is_empty() {
            let end = self.peek_span();
            Ok(Module {
                body,
                span: start.merge(end),
            })
        } else {
            Err(errors)
        }
    }

    // ── Statements ────────────────────────────────────────

    /// Parse one logical line as `;`-separated simple statements (with an
    /// optional trailing `;`), pushing each onto `out`. #218: `x = 1; y = 2`
    /// and a trailing `frq[i] += 1;` are legal Python outside a one-line suite
    /// too, but only `parse_block`'s inline form handled `;` before — the
    /// module loop and indented-block loop required a NEWLINE after each stmt.
    fn parse_stmt_line(&mut self, out: &mut Vec<Stmt>) -> Result<(), ParseError> {
        out.push(self.parse_stmt()?);
        while self.at(&Token::Semicolon) {
            self.advance();
            if self.at(&Token::Newline) || self.at(&Token::Eof) || self.at(&Token::Dedent) {
                break;
            }
            out.push(self.parse_stmt()?);
        }
        Ok(())
    }

    fn parse_stmt(&mut self) -> Result<Stmt, ParseError> {
        // Recursion-depth guard for nested statement blocks: `if`/`while`/`for`/
        // `with`/`try`/… suites recurse `parse_stmt → parse_*_stmt → parse_block
        // → parse_stmt`. Sequential (non-nested) statements each enter and drop
        // this guard in turn, so a long flat body never trips it — only genuine
        // block nesting accumulates depth.
        let _guard = match DepthGuard::enter() {
            Some(g) => g,
            None => return Err(self.error_too_deep("block")),
        };
        self.skip_newlines();
        let start = self.peek_span();

        match self.peek().clone() {
            Token::Def => self.parse_func_def(false),
            Token::Async => {
                self.advance();
                match self.peek() {
                    Token::Def => self.parse_func_def(true),
                    Token::For => self.parse_for_stmt(true),
                    Token::With => self.parse_with_stmt(true),
                    _ => Err(self.error("Expected 'def', 'for', or 'with' after 'async'".into())),
                }
            }
            Token::Class => self.parse_class_def(),
            Token::If => self.parse_if_stmt(),
            Token::While => self.parse_while_stmt(),
            Token::For => self.parse_for_stmt(false),
            Token::Return => {
                // B12: `return` outside any function is invalid Python. Left
                // unchecked it emitted a top-level JS `return`, which Node
                // rejects with its own SyntaxError downstream.
                if self.function_depth == 0 {
                    return Err(self.error("'return' outside function".into()));
                }
                self.parse_return_stmt()
            }
            Token::Break => {
                // B12: `break` outside a loop is invalid Python.
                if self.loop_depth == 0 {
                    return Err(self.error("'break' outside loop".into()));
                }
                self.advance();
                self.expect_stmt_end()?;
                Ok(Stmt::new(StmtKind::Break, start))
            }
            Token::Continue => {
                // B12: `continue` outside a loop is invalid Python.
                if self.loop_depth == 0 {
                    return Err(self.error("'continue' outside loop".into()));
                }
                self.advance();
                self.expect_stmt_end()?;
                Ok(Stmt::new(StmtKind::Continue, start))
            }
            Token::Pass => {
                self.advance();
                self.expect_stmt_end()?;
                Ok(Stmt::new(StmtKind::Pass, start))
            }
            Token::Import => self.parse_import_stmt(),
            Token::From => self.parse_from_import_stmt(),
            Token::Try => self.parse_try_stmt(),
            Token::Raise => self.parse_raise_stmt(),
            Token::Assert => self.parse_assert_stmt(),
            Token::Global => self.parse_global_stmt(),
            Token::Nonlocal => self.parse_nonlocal_stmt(),
            Token::Del => self.parse_del_stmt(),
            Token::With => self.parse_with_stmt(false),
            Token::At => self.parse_decorated(),
            // `match` is a soft keyword: only a match statement as the leading
            // word of `match SUBJECT:` (with a top-level trailing colon);
            // otherwise it is an ordinary identifier and falls through to the
            // expression/assignment path below.
            Token::Identifier(ref name) if name == "match" && self.looks_like_match_stmt() => {
                self.parse_match_stmt()
            }
            Token::Identifier(ref name) => {
                if let Some(suggestion) = suggest_keyword(name) {
                    Err(self.error_with_notes(
                        format!("Unknown statement '{}', found {}", name, self.peek()),
                        vec![format!("did you mean '{}'?", suggestion)],
                    ))
                } else {
                    self.parse_expr_or_assign_stmt()
                }
            }
            _ => self.parse_expr_or_assign_stmt(),
        }
    }

    fn parse_func_def(&mut self, is_async: bool) -> Result<Stmt, ParseError> {
        let start = self.peek_span();
        self.expect(&Token::Def)?;
        let (name, _) = self.expect_identifier()?;
        self.expect(&Token::LParen)?;
        let params = self.parse_params()?;
        self.expect_rparen()?;

        let return_type = if self.eat(&Token::Arrow) {
            Some(self.parse_expr()?)
        } else {
            None
        };

        self.expect_colon_for("def")?;
        // B12: the body opens a new function scope. `return` becomes legal
        // (function_depth > 0) and any enclosing loop no longer makes `break`/
        // `continue` legal here (loop_depth resets, restored after the body).
        let saved_loop = self.loop_depth;
        self.function_depth += 1;
        self.loop_depth = 0;
        let body_result = self.parse_block();
        self.function_depth -= 1;
        self.loop_depth = saved_loop;
        let body = body_result?;
        let end = body.last().map(|s| s.span).unwrap_or(start);

        Ok(Stmt::new(
            StmtKind::FuncDef {
                name,
                params,
                body,
                decorator_list: vec![],
                return_type,
                is_async,
            },
            start.merge(end),
        ))
    }

    fn parse_params(&mut self) -> Result<Vec<Param>, ParseError> {
        let mut params = Vec::new();
        if self.at(&Token::RParen) {
            return Ok(params);
        }

        // B12 parameter-list validity tracking (each a clean parse-time
        // diagnostic instead of a silently-miscompiled function):
        //   • duplicate parameter names               `def f(a, a)`
        //   • a required param after an optional one   `def f(a=1, b)`
        //   • any parameter after `**kwargs`           `def f(**k, *a)`
        let mut seen_names: std::collections::HashSet<String> = std::collections::HashSet::new();
        let mut seen_default = false; // an optional param seen in the current group
        let mut after_star = false; // past a `*`/`*args` — keyword-only group
        let mut seen_kwargs = false; // past a `**kwargs` — nothing may follow

        loop {
            let start = self.peek_span();
            let mut is_args = false;
            let mut is_kwargs = false;

            // B12: a `**kwargs` parameter must be last. Any further list item
            // (another name, `*`, `/`, ...) is invalid Python.
            if seen_kwargs {
                return Err(self.error(
                    "parameter after **kwargs — the var-keyword parameter must be last".into(),
                ));
            }

            // Round-2 pythonic sweep: bare `*` (keyword-only separator)
            // and `/` (positional-only separator) are structural markers,
            // not parameters — consume them and continue. Binding
            // enforcement is lenient (parameters stay ordinary JS params;
            // keyword calls bind by name via __pyCallKw metadata).
            if self.at(&Token::Slash) {
                self.advance();
                if !self.eat(&Token::Comma) {
                    break;
                }
                if self.at(&Token::RParen) {
                    break;
                }
                continue;
            }

            if self.eat(&Token::Star) {
                if self.at(&Token::Comma) {
                    // bare `*,` — keyword-only separator. Params after it are
                    // keyword-only, where a required param may follow an
                    // optional one, so reset the default-ordering tracking.
                    self.advance();
                    after_star = true;
                    seen_default = false;
                    if self.at(&Token::RParen) {
                        break;
                    }
                    continue;
                }
                is_args = true;
                // A `*args` also opens the keyword-only group.
                after_star = true;
                seen_default = false;
            } else if self.eat(&Token::DoubleStar) {
                is_kwargs = true;
            }

            let (name, name_span) = self.expect_identifier()?;

            // B12: duplicate parameter name (covers `*args`/`**kwargs` names too).
            if !seen_names.insert(name.clone()) {
                return Err(self.error_at(
                    format!("duplicate parameter '{}' in function definition", name),
                    name_span,
                ));
            }

            let annotation = if self.eat(&Token::Colon) {
                Some(Box::new(self.parse_expr()?))
            } else {
                None
            };

            let default = if self.eat(&Token::Eq) {
                Some(self.parse_expr()?)
            } else {
                None
            };

            // B12: within the positional-or-keyword group (before any `*`), a
            // required parameter may not follow an optional one. `*args` /
            // `**kwargs` don't take part, and keyword-only params (after `*`)
            // are exempt.
            if !is_args && !is_kwargs && !after_star {
                if default.is_some() {
                    seen_default = true;
                } else if seen_default {
                    return Err(self.error_at(
                        "non-default parameter follows a default parameter".into(),
                        start,
                    ));
                }
            }

            params.push(Param {
                name,
                annotation,
                default,
                is_args,
                is_kwargs,
                span: start,
            });

            if is_kwargs {
                seen_kwargs = true;
            }

            if !self.eat(&Token::Comma) {
                break;
            }
            if self.at(&Token::RParen) {
                break;
            }
        }

        Ok(params)
    }

    fn parse_class_def(&mut self) -> Result<Stmt, ParseError> {
        let start = self.peek_span();
        self.expect(&Token::Class)?;
        let (name, _) = self.expect_identifier()?;

        let bases = if self.eat(&Token::LParen) {
            let mut bases = Vec::new();
            if !self.at(&Token::RParen) {
                loop {
                    bases.push(self.parse_expr()?);
                    if !self.eat(&Token::Comma) {
                        break;
                    }
                    if self.at(&Token::RParen) {
                        break;
                    }
                }
            }
            self.expect(&Token::RParen)?;
            bases
        } else {
            vec![]
        };

        self.expect_colon_for("class")?;
        // B12: a class body is not a function and not a loop — a bare `break`/
        // `continue`/`return` directly in it is invalid Python even when the
        // class is nested in a loop. Reset loop_depth for the body (function
        // scope is unchanged; methods re-enter via parse_func_def).
        let saved_loop = self.loop_depth;
        self.loop_depth = 0;
        let body_result = self.parse_block();
        self.loop_depth = saved_loop;
        let body = body_result?;
        let end = body.last().map(|s| s.span).unwrap_or(start);

        Ok(Stmt::new(
            StmtKind::ClassDef {
                name,
                bases,
                body,
                decorator_list: vec![],
            },
            start.merge(end),
        ))
    }

    fn parse_if_stmt(&mut self) -> Result<Stmt, ParseError> {
        let start = self.peek_span();
        self.expect(&Token::If)?;
        let test = self.parse_expr()?;
        self.expect_colon_for("if")?;
        let body = self.parse_block()?;

        let mut elif_clauses = Vec::new();
        let mut else_body = None;

        self.skip_newlines();
        while self.eat(&Token::Elif) {
            let cond = self.parse_expr()?;
            self.expect(&Token::Colon)?;
            let elif_body = self.parse_block()?;
            elif_clauses.push((cond, elif_body));
            self.skip_newlines();
        }

        if self.eat(&Token::Else) {
            self.expect(&Token::Colon)?;
            else_body = Some(self.parse_block()?);
        }

        let end_span = else_body
            .as_ref()
            .and_then(|b| b.last().map(|s| s.span))
            .or(elif_clauses
                .last()
                .and_then(|(_, b)| b.last().map(|s| s.span)))
            .or(body.last().map(|s| s.span))
            .unwrap_or(start);

        Ok(Stmt::new(
            StmtKind::If {
                test,
                body,
                elif_clauses,
                else_body,
            },
            start.merge(end_span),
        ))
    }

    fn parse_while_stmt(&mut self) -> Result<Stmt, ParseError> {
        let start = self.peek_span();
        self.expect(&Token::While)?;
        let test = self.parse_expr()?;
        self.expect_colon_for("while")?;
        // B12: `break`/`continue` are legal inside the loop body (loop_depth > 0)
        // but NOT inside the `else` clause, which runs after the loop finishes.
        self.loop_depth += 1;
        let body_result = self.parse_block();
        self.loop_depth -= 1;
        let body = body_result?;

        let mut else_body = None;
        self.skip_newlines();
        if self.eat(&Token::Else) {
            self.expect(&Token::Colon)?;
            else_body = Some(self.parse_block()?);
        }

        let end = else_body
            .as_ref()
            .and_then(|b| b.last().map(|s| s.span))
            .or(body.last().map(|s| s.span))
            .unwrap_or(start);

        Ok(Stmt::new(
            StmtKind::While {
                test,
                body,
                else_body,
            },
            start.merge(end),
        ))
    }

    fn parse_for_stmt(&mut self, is_async: bool) -> Result<Stmt, ParseError> {
        let start = self.peek_span();
        self.expect(&Token::For)?;
        let target = self.parse_target_list()?;
        self.expect(&Token::In)?;
        let iter = self.parse_expr()?;
        self.expect_colon_for("for")?;
        // B12: `break`/`continue` legal inside the loop body, not the `else`.
        self.loop_depth += 1;
        let body_result = self.parse_block();
        self.loop_depth -= 1;
        let body = body_result?;

        let mut else_body = None;
        self.skip_newlines();
        if self.eat(&Token::Else) {
            self.expect(&Token::Colon)?;
            else_body = Some(self.parse_block()?);
        }

        let end = else_body
            .as_ref()
            .and_then(|b| b.last().map(|s| s.span))
            .or(body.last().map(|s| s.span))
            .unwrap_or(start);

        Ok(Stmt::new(
            StmtKind::For {
                target,
                iter,
                body,
                else_body,
                is_async,
            },
            start.merge(end),
        ))
    }

    fn parse_target_list(&mut self) -> Result<Expr, ParseError> {
        let first = self.parse_target_item()?;
        if self.at(&Token::Comma) && !self.at(&Token::In) {
            let start = first.span;
            let mut elts = vec![first];
            while self.eat(&Token::Comma) {
                if self.at(&Token::In) || self.at(&Token::Eq) {
                    break;
                }
                elts.push(self.parse_target_item()?);
            }
            let end = elts.last().map(|e| e.span).unwrap_or(start);
            Ok(Expr::new(ExprKind::Tuple(elts), start.merge(end)))
        } else {
            Ok(first)
        }
    }

    /// One element of a for-target list. Round-2 pythonic sweep: targets
    /// may be starred (`for x, *ys in ...`) in both plain for statements
    /// and comprehension clauses — previously `*` here was a parse error
    /// ("Expected in, found ...").
    fn parse_target_item(&mut self) -> Result<Expr, ParseError> {
        if self.at(&Token::Star) {
            let start = self.peek_span();
            self.advance();
            let inner = self.parse_primary()?;
            let end = inner.span;
            return Ok(Expr::new(
                ExprKind::Starred(Box::new(inner)),
                start.merge(end),
            ));
        }
        self.parse_primary()
    }

    fn parse_return_stmt(&mut self) -> Result<Stmt, ParseError> {
        let start = self.peek_span();
        self.expect(&Token::Return)?;
        // A bare `return` is terminated by NEWLINE, EOF, or — inside a one-line
        // suite — a SEMICOLON (`if x: return; y = 1`). Without the Semicolon
        // arm we fall into parse_expr() and report "Unexpected token: ;", even
        // though every other simple statement (`pass;`, `break;`, `del a;`,
        // and even `return 1;`) accepts the same terminator. Found by
        // scripts/grammar-fuzz.py.
        let value =
            if self.at(&Token::Newline) || self.at(&Token::Eof) || self.at(&Token::Semicolon) {
                None
            } else {
                // A return value is a testlist, so `return a, b` yields a tuple —
                // parse_expr_list, not parse_expr (which stops at the first comma
                // and reports "Unexpected token: ,"). Issue #200.
                Some(self.parse_expr_list()?)
            };
        let end = value.as_ref().map(|e| e.span).unwrap_or(start);
        self.expect_stmt_end()?;
        Ok(Stmt::new(StmtKind::Return(value), start.merge(end)))
    }

    fn parse_import_stmt(&mut self) -> Result<Stmt, ParseError> {
        let start = self.peek_span();
        self.expect(&Token::Import)?;

        // PythScribe extension: `import "./styles.css"` — a bare string
        // literal names a side-effect-only asset import (CSS/SCSS/images/
        // etc.), emitted verbatim to JS. Not valid Python syntax; only this
        // exact shape (a single string literal, nothing else on the line)
        // takes this path — anything else falls through to the normal
        // `import module[, module ...]` grammar below.
        if let Token::String_(s) = self.peek().clone() {
            let end = self.peek_span();
            self.advance();
            // Only the exact shape `import "<string>"` (nothing else on the
            // line) takes this path — `import "./x.css" as y`, trailing
            // commas, etc. are malformed and must still be parse errors,
            // not silently truncated.
            // SEMICOLON terminates the statement just as NEWLINE does, inside a
            // one-line suite (`if dev: import "./debug.css"; run()`).
            if !self.at(&Token::Newline) && !self.at(&Token::Eof) && !self.at(&Token::Semicolon) {
                return Err(self.error(
                    "Unexpected token after side-effect import string literal; \
                     `import \"<path>\"` accepts no alias, no trailing comma, \
                     and no further names"
                        .to_string(),
                ));
            }
            self.expect_stmt_end()?;
            return Ok(Stmt::new(StmtKind::ImportSideEffect(s), start.merge(end)));
        }

        let mut names = Vec::new();
        loop {
            let (name, _) = self.expect_identifier()?;
            let mut full_name = name;
            while self.eat(&Token::Dot) {
                let (part, _) = self.expect_identifier()?;
                full_name = format!("{}.{}", full_name, part);
            }
            let alias = if self.eat(&Token::As) {
                let (alias, _) = self.expect_identifier()?;
                Some(alias)
            } else {
                None
            };
            names.push(ImportAlias {
                name: full_name,
                alias,
            });
            if !self.eat(&Token::Comma) {
                break;
            }
        }
        self.expect_stmt_end()?;
        Ok(Stmt::new(StmtKind::Import { names }, start))
    }

    fn parse_from_import_stmt(&mut self) -> Result<Stmt, ParseError> {
        let start = self.peek_span();
        self.expect(&Token::From)?;

        // Python relative imports: count leading dots as the level.
        // `from . import x`     → level=1, module=""
        // `from ..foo import x` → level=2, module="foo"
        // `from ...foo import x`→ level=3, module="foo" (lexer fuses ... into Ellipsis)
        // `from foo import x`   → level=0, module="foo"
        let mut level: u32 = 0;
        loop {
            if self.eat(&Token::Ellipsis) {
                level += 3;
            } else if self.eat(&Token::Dot) {
                level += 1;
            } else {
                break;
            }
        }

        // If level > 0, the module name is optional (`from . import x`).
        // If level == 0, an identifier is required.
        let full_module = if self.peek() == &Token::Import {
            // No identifier — must be a bare relative import (`from . import x`).
            if level == 0 {
                return Err(self.error("Expected module name after 'from'".to_string()));
            }
            String::new()
        } else {
            let (module, _) = self.expect_identifier()?;
            let mut full_module = module;
            while self.eat(&Token::Dot) {
                let (part, _) = self.expect_identifier()?;
                full_module = format!("{}.{}", full_module, part);
            }
            full_module
        };

        self.expect(&Token::Import)?;
        let mut names = Vec::new();
        // `from module import *` — represented by a single "*" sentinel alias.
        // Codegen no-ops the erased modules (typing/dataclasses/pydantic) and
        // emits a namespace import for the rest.
        if self.eat(&Token::Star) {
            names.push(ImportAlias {
                name: "*".to_string(),
                alias: None,
            });
        } else {
            // Python allows the imported-name list to be wrapped in
            // parentheses — `from x import (a, b as c,)` — which enables
            // multi-line lists (the lexer already suppresses NEWLINE inside
            // brackets) and a trailing comma. Purely syntactic: same AST as
            // the unparenthesized form. Plain `import (a)` stays an error.
            let parenthesized = self.eat(&Token::LParen);
            loop {
                let (name, _) = self.expect_identifier()?;
                let alias = if self.eat(&Token::As) {
                    let (alias, _) = self.expect_identifier()?;
                    Some(alias)
                } else {
                    None
                };
                names.push(ImportAlias { name, alias });
                if !self.eat(&Token::Comma) {
                    break;
                }
                // Trailing comma is only legal in the parenthesized form.
                if parenthesized && self.at(&Token::RParen) {
                    break;
                }
            }
            if parenthesized {
                self.expect(&Token::RParen)?;
            }
        }
        self.expect_stmt_end()?;
        Ok(Stmt::new(
            StmtKind::ImportFrom {
                module: full_module,
                names,
                level,
            },
            start,
        ))
    }

    fn parse_try_stmt(&mut self) -> Result<Stmt, ParseError> {
        let start = self.peek_span();
        self.expect(&Token::Try)?;
        self.expect(&Token::Colon)?;
        let body = self.parse_block()?;

        let mut handlers = Vec::new();
        let mut else_body = None;
        let mut finally_body = None;

        self.skip_newlines();
        while self.eat(&Token::Except) {
            let handler_start = self.peek_span();
            let exc_type = if !self.at(&Token::Colon) {
                Some(self.parse_expr()?)
            } else {
                None
            };
            let name = if self.eat(&Token::As) {
                let (n, _) = self.expect_identifier()?;
                Some(n)
            } else {
                None
            };
            self.expect(&Token::Colon)?;
            let handler_body = self.parse_block()?;
            handlers.push(ExceptHandler {
                exc_type,
                name,
                body: handler_body,
                span: handler_start,
            });
            self.skip_newlines();
        }

        if self.eat(&Token::Else) {
            self.expect(&Token::Colon)?;
            else_body = Some(self.parse_block()?);
            self.skip_newlines();
        }

        if self.eat(&Token::Finally) {
            self.expect(&Token::Colon)?;
            finally_body = Some(self.parse_block()?);
        }

        Ok(Stmt::new(
            StmtKind::Try {
                body,
                handlers,
                else_body,
                finally_body,
            },
            start,
        ))
    }

    fn parse_raise_stmt(&mut self) -> Result<Stmt, ParseError> {
        let start = self.peek_span();
        self.expect(&Token::Raise)?;
        // Same one-line-suite terminator as `return` — `if x: raise;` is legal.
        let value =
            if self.at(&Token::Newline) || self.at(&Token::Eof) || self.at(&Token::Semicolon) {
                None
            } else {
                Some(self.parse_expr()?)
            };
        // `raise X from Y` — explicit exception chaining (PEP 3134).
        let cause = if value.is_some() && self.at(&Token::From) {
            self.expect(&Token::From)?;
            Some(self.parse_expr()?)
        } else {
            None
        };
        self.expect_stmt_end()?;
        Ok(Stmt::new(StmtKind::Raise(value, cause), start))
    }

    fn parse_assert_stmt(&mut self) -> Result<Stmt, ParseError> {
        let start = self.peek_span();
        self.expect(&Token::Assert)?;
        let test = self.parse_expr()?;
        let msg = if self.eat(&Token::Comma) {
            Some(self.parse_expr()?)
        } else {
            None
        };
        self.expect_stmt_end()?;
        Ok(Stmt::new(StmtKind::Assert { test, msg }, start))
    }

    fn parse_global_stmt(&mut self) -> Result<Stmt, ParseError> {
        let start = self.peek_span();
        self.expect(&Token::Global)?;
        let mut names = Vec::new();
        loop {
            let (name, _) = self.expect_identifier()?;
            names.push(name);
            if !self.eat(&Token::Comma) {
                break;
            }
        }
        self.expect_stmt_end()?;
        Ok(Stmt::new(StmtKind::Global(names), start))
    }

    fn parse_nonlocal_stmt(&mut self) -> Result<Stmt, ParseError> {
        let start = self.peek_span();
        self.expect(&Token::Nonlocal)?;
        let mut names = Vec::new();
        loop {
            let (name, _) = self.expect_identifier()?;
            names.push(name);
            if !self.eat(&Token::Comma) {
                break;
            }
        }
        self.expect_stmt_end()?;
        Ok(Stmt::new(StmtKind::Nonlocal(names), start))
    }

    fn parse_del_stmt(&mut self) -> Result<Stmt, ParseError> {
        let start = self.peek_span();
        self.expect(&Token::Del)?;
        let mut targets = Vec::new();
        loop {
            // #101: `del d[k]` / `del obj.attr` need the postfix trailers
            // (subscript / attribute); parse_primary stopped at the bare
            // name, leaving `[k]` as a detached statement.
            targets.push(self.parse_postfix()?);
            if !self.eat(&Token::Comma) {
                break;
            }
        }
        self.expect_stmt_end()?;
        Ok(Stmt::new(StmtKind::Del(targets), start))
    }

    fn parse_with_stmt(&mut self, is_async: bool) -> Result<Stmt, ParseError> {
        let start = self.peek_span();
        self.expect(&Token::With)?;
        let mut items = Vec::new();
        loop {
            let context_expr = self.parse_expr()?;
            let optional_var = if self.eat(&Token::As) {
                Some(self.parse_primary()?)
            } else {
                None
            };
            items.push(WithItem {
                context_expr,
                optional_var,
            });
            if !self.eat(&Token::Comma) {
                break;
            }
        }
        self.expect(&Token::Colon)?;
        let body = self.parse_block()?;

        Ok(Stmt::new(
            StmtKind::With {
                items,
                body,
                is_async,
            },
            start,
        ))
    }

    fn parse_match_stmt(&mut self) -> Result<Stmt, ParseError> {
        let start = self.peek_span();
        // `match` is a soft keyword — it lexes as `Identifier("match")`, so
        // consume it directly rather than `expect(Token::Match)`.
        debug_assert!(self.at_soft_match());
        self.advance();
        let subject = self.parse_expr()?;
        self.expect(&Token::Colon)?;
        self.expect(&Token::Newline)?;
        self.expect(&Token::Indent)?;

        let mut cases = Vec::new();
        // #79: `case` is a SOFT keyword — it lexes as a plain Identifier
        // and is only special as the leading word of a clause directly
        // inside a `match` suite (mirroring CPython).
        while self.at_soft_case() {
            cases.push(self.parse_match_case()?);
            self.skip_newlines();
        }

        self.expect(&Token::Dedent)?;

        Ok(Stmt::new(StmtKind::Match { subject, cases }, start))
    }

    /// True when the next token is the soft keyword `case` (a plain
    /// Identifier whose text is "case") — see #79.
    fn at_soft_case(&self) -> bool {
        matches!(self.peek(), Token::Identifier(s) if s == "case")
    }

    /// True when the next token is the soft keyword `match` (Identifier "match").
    fn at_soft_match(&self) -> bool {
        matches!(self.peek(), Token::Identifier(s) if s == "match")
    }

    /// Disambiguate `match` (Identifier at statement start) as a match STATEMENT
    /// vs an ordinary identifier. A match statement is `match SUBJECT:` — the
    /// token after `match` must begin a subject expression (not an assignment,
    /// call, attribute, subscript, etc.), and the logical line must carry a
    /// top-level trailing colon. `match (x)` / `match [x]` are treated as a
    /// call / subscript (the common case); a parenthesised-subject match needs
    /// no parens (`match x:`), so nothing correct is lost.
    fn looks_like_match_stmt(&self) -> bool {
        let next = match self.tokens.get(self.pos + 1) {
            Some(st) => &st.token,
            None => return false,
        };
        // Tokens that mean `match` is being USED as a name.
        match next {
            Token::Eq
            | Token::Dot
            | Token::Comma
            | Token::Colon
            | Token::Semicolon
            | Token::Newline
            | Token::Eof
            | Token::RParen
            | Token::RBracket
            | Token::RBrace
            | Token::PlusEq
            | Token::MinusEq
            | Token::StarEq
            | Token::SlashEq
            | Token::PercentEq
            | Token::DoubleStarEq
            | Token::DoubleSlashEq
            | Token::AmpEq
            | Token::PipeEq
            | Token::CaretEq
            | Token::ShiftLeftEq
            | Token::ShiftRightEq
            | Token::Plus
            | Token::Minus
            | Token::Star
            | Token::Slash
            | Token::Percent
            | Token::DoubleSlash
            | Token::EqEq
            | Token::NotEq
            | Token::Lt
            | Token::Gt
            | Token::LtEq
            | Token::GtEq
            | Token::And
            | Token::Or
            | Token::In
            | Token::Is
            | Token::If => return false,
            // #288 (C): `match [7]:` / `match (x):` — a list/parenthesised
            // SUBJECT, previously auto-rejected as a subscript/call of a
            // variable named `match` ("Unexpected token: INDENT"). Still
            // ambiguous with a subscript statement (`match[1:]`), a call
            // (`match(x)`), or an annotated subscript target
            // (`match[7]: int`) — so require the logical line to END with a
            // top-level colon (a match suite header); anything else stays an
            // expression statement. CPython's PEG resolves the same shapes
            // in favour of the match statement only when the full
            // `match SUBJECT ':'` header fits.
            Token::LBracket | Token::LParen => {
                let mut depth = 0i32;
                let mut i = self.pos + 1;
                let mut trailing_toplevel_colon = false;
                while let Some(st) = self.tokens.get(i) {
                    match &st.token {
                        Token::Newline | Token::Eof => break,
                        Token::LParen | Token::LBracket | Token::LBrace => {
                            depth += 1;
                            trailing_toplevel_colon = false;
                        }
                        Token::RParen | Token::RBracket | Token::RBrace => {
                            depth -= 1;
                            trailing_toplevel_colon = false;
                        }
                        Token::Colon if depth == 0 => trailing_toplevel_colon = true,
                        _ => trailing_toplevel_colon = false,
                    }
                    i += 1;
                }
                return trailing_toplevel_colon;
            }
            _ => {}
        }
        // Otherwise require a top-level trailing colon on this logical line.
        let mut depth = 0i32;
        let mut i = self.pos + 1;
        while let Some(st) = self.tokens.get(i) {
            match &st.token {
                Token::Newline | Token::Eof => break,
                Token::LParen | Token::LBracket | Token::LBrace => depth += 1,
                Token::RParen | Token::RBracket | Token::RBrace => depth -= 1,
                Token::Colon if depth == 0 => return true,
                _ => {}
            }
            i += 1;
        }
        false
    }

    fn parse_match_case(&mut self) -> Result<MatchCase, ParseError> {
        if !self.at_soft_case() {
            return Err(self.error("Expected `case` clause in match statement".into()));
        }
        self.advance(); // consume the soft `case` identifier
        let mut pattern = self.parse_pattern()?;

        // #323: bare (unparenthesized) tuple pattern `case 1, 2:` — a
        // top-level comma makes the case a sequence pattern, identical to
        // the parenthesized `case (1, 2):` form. A trailing comma before the
        // guard/colon is allowed (`case 1, 2,:`).
        if self.at(&Token::Comma) {
            let mut patterns = vec![pattern];
            while self.eat(&Token::Comma) {
                if self.at(&Token::Colon) || self.at(&Token::If) {
                    break; // trailing comma
                }
                if self.eat(&Token::Star) {
                    if matches!(self.peek(), Token::Identifier(_)) {
                        let (name, _) = self.expect_identifier()?;
                        if name == "_" {
                            patterns.push(Pattern::Star(None));
                        } else {
                            patterns.push(Pattern::Star(Some(name)));
                        }
                    } else {
                        patterns.push(Pattern::Star(None));
                    }
                } else {
                    patterns.push(self.parse_pattern()?);
                }
            }
            pattern = Pattern::Sequence(patterns);
        }

        // Optional guard: `if condition`
        let guard = if self.eat(&Token::If) {
            Some(self.parse_expr()?)
        } else {
            None
        };

        self.expect(&Token::Colon)?;
        let body = self.parse_block()?;

        Ok(MatchCase {
            pattern,
            guard,
            body,
        })
    }

    fn parse_pattern(&mut self) -> Result<Pattern, ParseError> {
        let mut pattern = self.parse_pattern_atom()?;

        // OR pattern: `pat1 | pat2`
        if self.at(&Token::Pipe) {
            let mut alternatives = vec![pattern];
            while self.eat(&Token::Pipe) {
                alternatives.push(self.parse_pattern_atom()?);
            }
            pattern = Pattern::Or(alternatives);
        }

        // AS pattern: `pattern as name`
        if self.eat(&Token::As) {
            let (name, _) = self.expect_identifier()?;
            pattern = Pattern::As {
                pattern: Box::new(pattern),
                name,
            };
        }

        Ok(pattern)
    }

    fn parse_pattern_atom(&mut self) -> Result<Pattern, ParseError> {
        match self.peek().clone() {
            // Wildcard: _
            Token::Identifier(ref name) if name == "_" => {
                self.advance();
                Ok(Pattern::Wildcard)
            }
            // None/True/False literals
            Token::None_ => {
                self.advance();
                Ok(Pattern::Literal(Expr::none(self.prev_span())))
            }
            Token::True_ => {
                self.advance();
                Ok(Pattern::Literal(Expr::bool_lit(true, self.prev_span())))
            }
            Token::False => {
                self.advance();
                Ok(Pattern::Literal(Expr::bool_lit(false, self.prev_span())))
            }
            // Number literal
            Token::Integer(n) => {
                self.advance();
                Ok(Pattern::Literal(Expr::int(n, self.prev_span())))
            }
            Token::Minus => {
                self.advance();
                if let Token::Integer(n) = self.peek().clone() {
                    self.advance();
                    Ok(Pattern::Literal(Expr::int(-n, self.prev_span())))
                } else {
                    Err(self.error("Expected number after '-' in pattern".into()))
                }
            }
            // String literal
            Token::String_(ref s) => {
                let s = s.clone();
                self.advance();
                Ok(Pattern::Literal(Expr::string(s, self.prev_span())))
            }
            // Sequence pattern: [a, b, c]
            Token::LBracket => {
                self.advance();
                let mut patterns = Vec::new();
                while !self.at(&Token::RBracket) {
                    if self.eat(&Token::Star) {
                        // Star pattern: *rest
                        if self.at(&Token::Identifier(String::new()))
                            || matches!(self.peek(), Token::Identifier(_))
                        {
                            let (name, _) = self.expect_identifier()?;
                            if name == "_" {
                                patterns.push(Pattern::Star(None));
                            } else {
                                patterns.push(Pattern::Star(Some(name)));
                            }
                        } else {
                            patterns.push(Pattern::Star(None));
                        }
                    } else {
                        patterns.push(self.parse_pattern()?);
                    }
                    if !self.eat(&Token::Comma) {
                        break;
                    }
                }
                self.expect(&Token::RBracket)?;
                Ok(Pattern::Sequence(patterns))
            }
            // Mapping pattern: {key: pattern}
            Token::LBrace => {
                self.advance();
                let mut pairs = Vec::new();
                while !self.at(&Token::RBrace) {
                    let key = self.parse_expr()?;
                    self.expect(&Token::Colon)?;
                    let pattern = self.parse_pattern()?;
                    pairs.push((key, pattern));
                    if !self.eat(&Token::Comma) {
                        break;
                    }
                }
                self.expect(&Token::RBrace)?;
                Ok(Pattern::Mapping(pairs))
            }
            // Identifier — could be capture, class pattern, or value pattern
            Token::Identifier(ref name) => {
                let name = name.clone();
                self.advance();

                // Check for dotted value pattern: Color.RED
                if self.at(&Token::Dot) {
                    let mut expr = Expr::name(&name, self.prev_span());
                    while self.eat(&Token::Dot) {
                        let (attr, _) = self.expect_identifier()?;
                        let span = expr.span.merge(self.prev_span());
                        expr = Expr::new(
                            ExprKind::Attribute {
                                value: Box::new(expr),
                                attr,
                                optional: false,
                            },
                            span,
                        );
                    }
                    return Ok(Pattern::Value(expr));
                }

                // Check for class pattern: Point(x, y)
                if self.at(&Token::LParen) {
                    self.advance();
                    let mut args = Vec::new();
                    while !self.at(&Token::RParen) {
                        args.push(self.parse_pattern()?);
                        if !self.eat(&Token::Comma) {
                            break;
                        }
                    }
                    self.expect(&Token::RParen)?;
                    return Ok(Pattern::Class { cls: name, args });
                }

                // Simple capture
                Ok(Pattern::Capture(name))
            }
            // Parenthesized pattern (#323). CPython: `(p)` is grouping, but
            // `(p1, p2)` / `(p,)` / `()` is a sequence pattern — identical to
            // the `[...]` form. Parse comma-separated patterns (star patterns
            // allowed) and use the trailing/interior comma to distinguish a
            // 1-tuple `(p,)` from a mere grouping `(p)`.
            Token::LParen => {
                self.advance();
                let mut patterns = Vec::new();
                let mut saw_comma = false;
                while !self.at(&Token::RParen) {
                    if self.eat(&Token::Star) {
                        if matches!(self.peek(), Token::Identifier(_)) {
                            let (name, _) = self.expect_identifier()?;
                            if name == "_" {
                                patterns.push(Pattern::Star(None));
                            } else {
                                patterns.push(Pattern::Star(Some(name)));
                            }
                        } else {
                            patterns.push(Pattern::Star(None));
                        }
                    } else {
                        patterns.push(self.parse_pattern()?);
                    }
                    if self.eat(&Token::Comma) {
                        saw_comma = true;
                    } else {
                        break;
                    }
                }
                self.expect(&Token::RParen)?;
                if saw_comma || patterns.is_empty() {
                    Ok(Pattern::Sequence(patterns))
                } else {
                    Ok(patterns.into_iter().next().unwrap())
                }
            }
            _ => Err(self.error(format!("Unexpected token in pattern: {:?}", self.peek()))),
        }
    }

    fn parse_decorated(&mut self) -> Result<Stmt, ParseError> {
        let start = self.peek_span();
        let mut decorators = Vec::new();

        while self.eat(&Token::At) {
            let decorator = self.parse_expr()?;
            self.eat(&Token::Newline);
            self.skip_newlines();
            decorators.push(decorator);
        }

        let mut stmt = match self.peek() {
            Token::Def => self.parse_func_def(false)?,
            Token::Async => {
                self.advance();
                self.parse_func_def(true)?
            }
            Token::Class => self.parse_class_def()?,
            _ => return Err(self.error("Expected 'def' or 'class' after decorator".into())),
        };

        // Inject decorators
        match &mut stmt.kind {
            StmtKind::FuncDef { decorator_list, .. } => {
                *decorator_list = decorators;
            }
            StmtKind::ClassDef { decorator_list, .. } => {
                *decorator_list = decorators;
            }
            _ => unreachable!(),
        }
        stmt.span = start.merge(stmt.span);

        Ok(stmt)
    }

    fn parse_expr_or_assign_stmt(&mut self) -> Result<Stmt, ParseError> {
        let start = self.peek_span();
        let expr = self.parse_expr_list()?;

        // Check for augmented assignment: +=, -=, etc.
        if let Some(op) = self.peek_aug_assign() {
            self.advance();
            let value = self.parse_expr()?;
            let end = value.span;
            self.expect_stmt_end()?;
            return Ok(Stmt::new(
                StmtKind::AugAssign {
                    target: expr,
                    op,
                    value,
                },
                start.merge(end),
            ));
        }

        // Check for assignment: = (including chained `a = b = 5`, #99 —
        // every `=`-separated expr before the last one is a target).
        if self.eat(&Token::Eq) {
            let mut targets = vec![expr];
            let mut value = self.parse_expr_list()?;
            while self.eat(&Token::Eq) {
                targets.push(value);
                value = self.parse_expr_list()?;
            }
            let end = value.span;
            self.expect_stmt_end()?;
            return Ok(Stmt::new(
                StmtKind::Assign { targets, value },
                start.merge(end),
            ));
        }

        // Check for type-annotated assignment: x: type = value
        if self.eat(&Token::Colon) {
            let annotation = self.parse_expr()?;
            let value = if self.eat(&Token::Eq) {
                Some(self.parse_expr()?)
            } else {
                None
            };
            let end = value.as_ref().map(|v| v.span).unwrap_or(annotation.span);
            self.expect_stmt_end()?;
            return Ok(Stmt::new(
                StmtKind::AnnAssign {
                    target: expr,
                    annotation,
                    value,
                },
                start.merge(end),
            ));
        }

        // F7: Python-2 `print "x"` / `print x` statement. A bare `print`
        // (no `(`) directly followed on the same line by a value-starting
        // token is the Python-2 print statement, which would otherwise
        // parse as two silent expression statements (`print;` then the
        // value) and emit wrong-but-compiling output. Reject it explicitly.
        if let ExprKind::Name(n) = &expr.kind {
            if n == "print"
                && matches!(
                    self.peek(),
                    Token::String_(_)
                        | Token::FString(_)
                        | Token::Integer(_)
                        | Token::Float(_)
                        | Token::Identifier(_)
                        | Token::True_
                        | Token::False
                        | Token::None_
                        | Token::Minus
                )
            {
                return Err(self.error(
                    "Python 2-style print statement — use print(...) instead. \
                     In Python 3 (and PythScribe), print is a function: write \
                     `print(x)`, not `print x`."
                        .into(),
                ));
            }
        }

        let end = expr.span;
        self.expect_stmt_end()?;
        Ok(Stmt::new(StmtKind::Expr(expr), start.merge(end)))
    }

    fn peek_aug_assign(&self) -> Option<AugAssignOp> {
        match self.peek() {
            Token::PlusEq => Some(AugAssignOp::Add),
            Token::MinusEq => Some(AugAssignOp::Sub),
            Token::StarEq => Some(AugAssignOp::Mul),
            Token::SlashEq => Some(AugAssignOp::Div),
            Token::DoubleSlashEq => Some(AugAssignOp::FloorDiv),
            Token::PercentEq => Some(AugAssignOp::Mod),
            Token::DoubleStarEq => Some(AugAssignOp::Pow),
            Token::AmpEq => Some(AugAssignOp::BitAnd),
            Token::PipeEq => Some(AugAssignOp::BitOr),
            Token::CaretEq => Some(AugAssignOp::BitXor),
            Token::ShiftLeftEq => Some(AugAssignOp::ShiftLeft),
            Token::ShiftRightEq => Some(AugAssignOp::ShiftRight),
            _ => None,
        }
    }

    /// Parse the suite after a colon: an indented block, or (Python's
    /// one-line form) simple statement(s) on the same line.
    fn parse_block(&mut self) -> Result<Vec<Stmt>, ParseError> {
        // Gen-eval round-5: `if cond: return x` / `if v: f(v); g(v)` are
        // legal Python one-line suites and a common model-generated shape
        // (3 of 5 remaining compile-failure samples). If the token after
        // the colon is NOT a newline, parse `;`-separated simple
        // statements on this line instead of requiring NEWLINE INDENT.
        if !self.at(&Token::Newline) && !self.at(&Token::Indent) && !self.at(&Token::Eof) {
            let mut stmts = Vec::new();
            loop {
                stmts.push(self.parse_stmt()?);
                if self.eat(&Token::Semicolon) {
                    // Trailing `;` before the line break is legal Python.
                    if self.at(&Token::Newline) || self.at(&Token::Eof) {
                        break;
                    }
                    continue;
                }
                break;
            }
            self.eat(&Token::Newline);
            return Ok(stmts);
        }
        self.eat(&Token::Newline);
        self.skip_newlines();
        self.expect(&Token::Indent)?;
        let mut stmts = Vec::new();

        loop {
            self.skip_newlines();
            if self.at(&Token::Dedent) || self.at(&Token::Eof) {
                break;
            }
            match self.parse_stmt_line(&mut stmts) {
                Ok(()) => {}
                Err(e) => {
                    self.errors.push(e);
                    self.synchronize_block();
                }
            }
        }

        self.eat(&Token::Dedent);

        if stmts.is_empty() {
            return Err(self.error("Expected at least one statement in block".into()));
        }

        Ok(stmts)
    }

    // ── Expressions ───────────────────────────────────────

    fn parse_expr_list(&mut self) -> Result<Expr, ParseError> {
        // TUPLE_END: tokens that close a bare (unparenthesised) tuple, i.e. a
        // trailing comma is legal immediately before any of them. SEMICOLON
        // belongs here because a one-line suite ends a statement with `;`
        // exactly as a NEWLINE does: `if x: a,;` is `if x: (a,)`. Its absence
        // made the trailing comma try to parse `;` as another element
        // ("Unexpected token: ;") even though `a,` and `if x: a, b;` both
        // parse. Found by scripts/grammar-fuzz.py.
        const TUPLE_END: &[Token] = &[
            Token::Eq,
            Token::Newline,
            Token::Eof,
            Token::Semicolon,
            Token::RParen,
            Token::RBracket,
            Token::RBrace,
        ];
        let first = self.parse_expr()?;
        if self.at(&Token::Comma) && !self.at_any(TUPLE_END) {
            let start = first.span;
            let mut elts = vec![first];
            while self.eat(&Token::Comma) {
                if self.at_any(TUPLE_END) {
                    break;
                }
                elts.push(self.parse_expr()?);
            }
            let end = elts.last().map(|e| e.span).unwrap_or(start);
            Ok(Expr::new(ExprKind::Tuple(elts), start.merge(end)))
        } else {
            Ok(first)
        }
    }

    pub fn parse_expr(&mut self) -> Result<Expr, ParseError> {
        // Recursion-depth guard: every nested expression (parenthesised,
        // bracketed, braced, f-string interpolation, …) re-enters through
        // `parse_expr`, so a single guard here bounds expression nesting. The
        // `_guard` decrements on drop, covering the `?`-propagation paths below.
        let _guard = match DepthGuard::enter() {
            Some(g) => g,
            None => return Err(self.error_too_deep("expression")),
        };
        self.parse_named_expr()
    }

    /// Walrus operator: `name := expr`
    fn parse_named_expr(&mut self) -> Result<Expr, ParseError> {
        let expr = self.parse_ternary()?;
        if self.eat(&Token::ColonEq) {
            let value = self.parse_named_expr()?;
            let span = expr.span.merge(value.span);
            return Ok(Expr::new(
                ExprKind::NamedExpr {
                    target: Box::new(expr),
                    value: Box::new(value),
                },
                span,
            ));
        }
        Ok(expr)
    }

    fn parse_ternary(&mut self) -> Result<Expr, ParseError> {
        let expr = self.parse_lambda()?;

        if self.eat(&Token::If) {
            let test = self.parse_or()?;
            self.expect(&Token::Else)?;
            let else_body = self.parse_ternary()?;
            let span = expr.span.merge(else_body.span);
            return Ok(Expr::new(
                ExprKind::IfExpr {
                    test: Box::new(test),
                    body: Box::new(expr),
                    else_body: Box::new(else_body),
                },
                span,
            ));
        }

        Ok(expr)
    }

    fn parse_lambda(&mut self) -> Result<Expr, ParseError> {
        if self.at(&Token::Lambda) {
            let start = self.peek_span();
            self.advance();
            let params = if !self.at(&Token::Colon) {
                self.parse_lambda_params()?
            } else {
                vec![]
            };
            self.expect(&Token::Colon)?;
            let body = self.parse_ternary()?;
            let end = body.span;
            return Ok(Expr::new(
                ExprKind::Lambda {
                    params,
                    body: Box::new(body),
                },
                start.merge(end),
            ));
        }
        self.parse_pipeline()
    }

    /// Pipeline operator: `expr |> func` or `expr |> func(args)`
    fn parse_pipeline(&mut self) -> Result<Expr, ParseError> {
        let mut left = self.parse_nullish()?;
        while self.eat(&Token::PipeGt) {
            let right = self.parse_nullish()?;
            let span = left.span.merge(right.span);
            left = Expr::new(
                ExprKind::BinOp {
                    left: Box::new(left),
                    op: BinOp::Pipeline,
                    right: Box::new(right),
                },
                span,
            );
        }
        Ok(left)
    }

    /// Nullish coalescing: `expr ?? default`
    fn parse_nullish(&mut self) -> Result<Expr, ParseError> {
        let mut left = self.parse_or()?;
        while self.eat(&Token::QuestionQuestion) {
            let right = self.parse_or()?;
            let span = left.span.merge(right.span);
            left = Expr::new(
                ExprKind::BinOp {
                    left: Box::new(left),
                    op: BinOp::NullishCoalesce,
                    right: Box::new(right),
                },
                span,
            );
        }
        Ok(left)
    }

    fn parse_lambda_params(&mut self) -> Result<Vec<Param>, ParseError> {
        let mut params = Vec::new();
        loop {
            let start = self.peek_span();
            let (name, _) = self.expect_identifier()?;
            let default = if self.eat(&Token::Eq) {
                Some(self.parse_expr()?)
            } else {
                None
            };
            params.push(Param {
                name,
                annotation: None,
                default,
                is_args: false,
                is_kwargs: false,
                span: start,
            });
            if !self.eat(&Token::Comma) {
                break;
            }
            if self.at(&Token::Colon) {
                break;
            }
        }
        Ok(params)
    }

    fn parse_or(&mut self) -> Result<Expr, ParseError> {
        let mut left = self.parse_and()?;
        while self.eat(&Token::Or) {
            let right = self.parse_and()?;
            let span = left.span.merge(right.span);
            left = Expr::new(
                ExprKind::BinOp {
                    left: Box::new(left),
                    op: BinOp::Or,
                    right: Box::new(right),
                },
                span,
            );
        }
        Ok(left)
    }

    fn parse_and(&mut self) -> Result<Expr, ParseError> {
        let mut left = self.parse_not()?;
        while self.eat(&Token::And) {
            let right = self.parse_not()?;
            let span = left.span.merge(right.span);
            left = Expr::new(
                ExprKind::BinOp {
                    left: Box::new(left),
                    op: BinOp::And,
                    right: Box::new(right),
                },
                span,
            );
        }
        Ok(left)
    }

    fn parse_not(&mut self) -> Result<Expr, ParseError> {
        if self.eat(&Token::Not) {
            let start = self.peek_span();
            let operand = self.parse_not()?;
            let span = start.merge(operand.span);
            return Ok(Expr::new(
                ExprKind::UnaryOp {
                    op: UnaryOp::Not,
                    operand: Box::new(operand),
                },
                span,
            ));
        }
        self.parse_comparison()
    }

    fn parse_comparison(&mut self) -> Result<Expr, ParseError> {
        let left = self.parse_bitor()?;
        let mut comparisons = Vec::new();

        loop {
            let op = match self.peek() {
                Token::EqEq => Some(BinOp::Eq),
                Token::NotEq => Some(BinOp::NotEq),
                Token::Lt => Some(BinOp::Lt),
                Token::LtEq => Some(BinOp::LtEq),
                Token::Gt => Some(BinOp::Gt),
                Token::GtEq => Some(BinOp::GtEq),
                Token::In => Some(BinOp::In),
                Token::Is => {
                    self.advance();
                    if self.eat(&Token::Not) {
                        comparisons.push((BinOp::IsNot, self.parse_bitor()?));
                        continue;
                    }
                    comparisons.push((BinOp::Is, self.parse_bitor()?));
                    continue;
                }
                Token::Not => {
                    // "not in"
                    let saved = self.pos;
                    self.advance();
                    if self.eat(&Token::In) {
                        comparisons.push((BinOp::NotIn, self.parse_bitor()?));
                        continue;
                    }
                    self.pos = saved;
                    None
                }
                _ => None,
            };

            if let Some(op) = op {
                self.advance();
                comparisons.push((op, self.parse_bitor()?));
            } else {
                break;
            }
        }

        if comparisons.is_empty() {
            Ok(left)
        } else {
            let span = left.span.merge(comparisons.last().unwrap().1.span);
            Ok(Expr::new(
                ExprKind::Compare {
                    left: Box::new(left),
                    comparisons,
                },
                span,
            ))
        }
    }

    fn parse_bitor(&mut self) -> Result<Expr, ParseError> {
        let mut left = self.parse_bitxor()?;
        while self.eat(&Token::Pipe) {
            let right = self.parse_bitxor()?;
            let span = left.span.merge(right.span);
            left = Expr::new(
                ExprKind::BinOp {
                    left: Box::new(left),
                    op: BinOp::BitOr,
                    right: Box::new(right),
                },
                span,
            );
        }
        Ok(left)
    }

    fn parse_bitxor(&mut self) -> Result<Expr, ParseError> {
        let mut left = self.parse_bitand()?;
        while self.eat(&Token::Caret) {
            let right = self.parse_bitand()?;
            let span = left.span.merge(right.span);
            left = Expr::new(
                ExprKind::BinOp {
                    left: Box::new(left),
                    op: BinOp::BitXor,
                    right: Box::new(right),
                },
                span,
            );
        }
        Ok(left)
    }

    fn parse_bitand(&mut self) -> Result<Expr, ParseError> {
        let mut left = self.parse_shift()?;
        while self.eat(&Token::Amp) {
            let right = self.parse_shift()?;
            let span = left.span.merge(right.span);
            left = Expr::new(
                ExprKind::BinOp {
                    left: Box::new(left),
                    op: BinOp::BitAnd,
                    right: Box::new(right),
                },
                span,
            );
        }
        Ok(left)
    }

    fn parse_shift(&mut self) -> Result<Expr, ParseError> {
        let mut left = self.parse_additive()?;
        loop {
            let op = match self.peek() {
                Token::ShiftLeft => BinOp::ShiftLeft,
                Token::ShiftRight => BinOp::ShiftRight,
                _ => break,
            };
            self.advance();
            let right = self.parse_additive()?;
            let span = left.span.merge(right.span);
            left = Expr::new(
                ExprKind::BinOp {
                    left: Box::new(left),
                    op,
                    right: Box::new(right),
                },
                span,
            );
        }
        Ok(left)
    }

    fn parse_additive(&mut self) -> Result<Expr, ParseError> {
        let mut left = self.parse_multiplicative()?;
        loop {
            let op = match self.peek() {
                Token::Plus => BinOp::Add,
                Token::Minus => BinOp::Sub,
                _ => break,
            };
            self.advance();
            let right = self.parse_multiplicative()?;
            let span = left.span.merge(right.span);
            left = Expr::new(
                ExprKind::BinOp {
                    left: Box::new(left),
                    op,
                    right: Box::new(right),
                },
                span,
            );
        }
        Ok(left)
    }

    fn parse_multiplicative(&mut self) -> Result<Expr, ParseError> {
        let mut left = self.parse_unary()?;
        loop {
            let op = match self.peek() {
                Token::Star => BinOp::Mul,
                Token::Slash => BinOp::Div,
                Token::DoubleSlash => BinOp::FloorDiv,
                Token::Percent => BinOp::Mod,
                _ => break,
            };
            self.advance();
            let right = self.parse_unary()?;
            let span = left.span.merge(right.span);
            left = Expr::new(
                ExprKind::BinOp {
                    left: Box::new(left),
                    op,
                    right: Box::new(right),
                },
                span,
            );
        }
        Ok(left)
    }

    fn parse_unary(&mut self) -> Result<Expr, ParseError> {
        match self.peek() {
            Token::Minus => {
                let start = self.peek_span();
                self.advance();
                let operand = self.parse_unary()?;
                let span = start.merge(operand.span);
                Ok(Expr::new(
                    ExprKind::UnaryOp {
                        op: UnaryOp::Neg,
                        operand: Box::new(operand),
                    },
                    span,
                ))
            }
            Token::Plus => {
                let start = self.peek_span();
                self.advance();
                let operand = self.parse_unary()?;
                let span = start.merge(operand.span);
                Ok(Expr::new(
                    ExprKind::UnaryOp {
                        op: UnaryOp::Pos,
                        operand: Box::new(operand),
                    },
                    span,
                ))
            }
            Token::Tilde => {
                let start = self.peek_span();
                self.advance();
                let operand = self.parse_unary()?;
                let span = start.merge(operand.span);
                Ok(Expr::new(
                    ExprKind::UnaryOp {
                        op: UnaryOp::BitNot,
                        operand: Box::new(operand),
                    },
                    span,
                ))
            }
            Token::Await => {
                let start = self.peek_span();
                self.advance();
                let operand = self.parse_unary()?;
                let span = start.merge(operand.span);
                Ok(Expr::new(ExprKind::Await(Box::new(operand)), span))
            }
            _ => self.parse_power(),
        }
    }

    fn parse_power(&mut self) -> Result<Expr, ParseError> {
        let base = self.parse_postfix()?;
        if self.eat(&Token::DoubleStar) {
            let exp = self.parse_unary()?; // Right-associative
            let span = base.span.merge(exp.span);
            Ok(Expr::new(
                ExprKind::BinOp {
                    left: Box::new(base),
                    op: BinOp::Pow,
                    right: Box::new(exp),
                },
                span,
            ))
        } else {
            Ok(base)
        }
    }

    fn parse_postfix(&mut self) -> Result<Expr, ParseError> {
        let mut expr = self.parse_primary()?;

        loop {
            match self.peek() {
                Token::LParen => {
                    self.advance();
                    let (args, kwargs) = self.parse_call_args()?;
                    let end = self.peek_span();
                    self.expect(&Token::RParen)?;
                    let span = expr.span.merge(end);
                    expr = Expr::new(
                        ExprKind::Call {
                            func: Box::new(expr),
                            args,
                            kwargs,
                            optional: false,
                        },
                        span,
                    );
                }
                Token::LBracket => {
                    self.advance();
                    let index = self.parse_subscript_index()?;
                    let end = self.peek_span();
                    self.expect(&Token::RBracket)?;
                    let span = expr.span.merge(end);
                    expr = Expr::new(
                        ExprKind::Subscript {
                            value: Box::new(expr),
                            index: Box::new(index),
                            optional: false,
                        },
                        span,
                    );
                }
                Token::Dot => {
                    self.advance();
                    let (attr, attr_span) = self.expect_attr_name()?;
                    let span = expr.span.merge(attr_span);
                    expr = Expr::new(
                        ExprKind::Attribute {
                            value: Box::new(expr),
                            attr,
                            optional: false,
                        },
                        span,
                    );
                }
                // Optional chaining: ?. ?[ ?(
                Token::QuestionDot => {
                    self.advance();
                    // ?.attr or ?.[index] or ?.(args)
                    match self.peek() {
                        Token::LBracket => {
                            self.advance();
                            let index = self.parse_subscript_index()?;
                            let end = self.peek_span();
                            self.expect(&Token::RBracket)?;
                            let span = expr.span.merge(end);
                            expr = Expr::new(
                                ExprKind::Subscript {
                                    value: Box::new(expr),
                                    index: Box::new(index),
                                    optional: true,
                                },
                                span,
                            );
                        }
                        Token::LParen => {
                            self.advance();
                            let (args, kwargs) = self.parse_call_args()?;
                            let end = self.peek_span();
                            self.expect(&Token::RParen)?;
                            let span = expr.span.merge(end);
                            expr = Expr::new(
                                ExprKind::Call {
                                    func: Box::new(expr),
                                    args,
                                    kwargs,
                                    optional: true,
                                },
                                span,
                            );
                        }
                        _ => {
                            let (attr, attr_span) = self.expect_attr_name()?;
                            let span = expr.span.merge(attr_span);
                            expr = Expr::new(
                                ExprKind::Attribute {
                                    value: Box::new(expr),
                                    attr,
                                    optional: true,
                                },
                                span,
                            );
                        }
                    }
                }
                _ => break,
            }
        }

        Ok(expr)
    }

    fn parse_call_args(&mut self) -> Result<(Vec<Expr>, Vec<Keyword>), ParseError> {
        let mut args = Vec::new();
        let mut kwargs = Vec::new();

        if self.at(&Token::RParen) {
            return Ok((args, kwargs));
        }

        loop {
            // Check for **kwargs
            if self.eat(&Token::DoubleStar) {
                let value = self.parse_expr()?;
                kwargs.push(Keyword {
                    name: None,
                    value,
                    span: self.peek_span(),
                });
            }
            // Check for *args
            else if self.eat(&Token::Star) {
                let value = self.parse_expr()?;
                args.push(Expr::new(
                    ExprKind::Starred(Box::new(value.clone())),
                    value.span,
                ));
            }
            // Kwarg shorthand: =name means name=name
            else if self.at(&Token::Eq) {
                let eq_span = self.peek_span();
                self.advance();
                let (name, name_span) = self.expect_identifier()?;
                kwargs.push(Keyword {
                    name: Some(name.clone()),
                    value: Expr::new(ExprKind::Name(name), name_span),
                    span: eq_span.merge(name_span),
                });
            }
            // Check for keyword=value
            else {
                let expr = self.parse_expr()?;
                if self.eat(&Token::Eq) {
                    // keyword argument
                    if let ExprKind::Name(name) = &expr.kind {
                        let value = self.parse_expr()?;
                        kwargs.push(Keyword {
                            name: Some(name.clone()),
                            value,
                            span: expr.span,
                        });
                    } else {
                        return Err(self.error("Invalid keyword argument".into()));
                    }
                } else {
                    // Check if this is a generator expression
                    if self.at_comprehension_start() {
                        let generators = self.parse_comprehension_clauses()?;
                        let span = expr.span;
                        args.push(Expr::new(
                            ExprKind::GeneratorExp {
                                elt: Box::new(expr),
                                generators,
                            },
                            span,
                        ));
                    } else {
                        args.push(expr);
                    }
                }
            }

            if !self.eat(&Token::Comma) {
                break;
            }
            if self.at(&Token::RParen) {
                break;
            }
        }

        Ok((args, kwargs))
    }

    fn parse_subscript_index(&mut self) -> Result<Expr, ParseError> {
        let start = self.peek_span();

        // Check for slice syntax
        if self.at(&Token::Colon) {
            return self.parse_slice(None, start);
        }

        let first = self.parse_expr()?;

        if self.at(&Token::Colon) {
            return self.parse_slice(Some(first), start);
        }

        // Handle comma-separated indexes as tuple (e.g., Dict[str, int])
        if self.at(&Token::Comma) {
            let mut elements = vec![first];
            while self.eat(&Token::Comma) {
                if self.at(&Token::RBracket) {
                    break; // trailing comma
                }
                elements.push(self.parse_expr()?);
            }
            let end = elements.last().map(|e| e.span).unwrap_or(start);
            return Ok(Expr::new(ExprKind::Tuple(elements), start.merge(end)));
        }

        Ok(first)
    }

    fn parse_slice(&mut self, lower: Option<Expr>, start: Span) -> Result<Expr, ParseError> {
        self.expect(&Token::Colon)?;

        let upper = if !self.at(&Token::Colon) && !self.at(&Token::RBracket) {
            Some(Box::new(self.parse_expr()?))
        } else {
            None
        };

        let step = if self.eat(&Token::Colon) {
            if !self.at(&Token::RBracket) {
                Some(Box::new(self.parse_expr()?))
            } else {
                None
            }
        } else {
            None
        };

        let end = step
            .as_ref()
            .map(|e| e.span)
            .or(upper.as_ref().map(|e| e.span))
            .unwrap_or(start);

        Ok(Expr::new(
            ExprKind::Slice {
                lower: lower.map(Box::new),
                upper,
                step,
            },
            start.merge(end),
        ))
    }

    /// #109: consume any run of adjacent String_/FString tokens following
    /// an (f-)string literal, appending their content to `parts` (Python
    /// implicit literal concatenation, f-string participants included).
    fn parse_adjacent_string_parts(
        &mut self,
        parts: &mut Vec<pyths_syntax::ast::FStringPart>,
        start: Span,
    ) -> Result<(), ParseError> {
        loop {
            match self.peek().clone() {
                Token::String_(more) => {
                    parts.push(pyths_syntax::ast::FStringPart::Literal(more));
                    self.advance();
                }
                Token::FString(more) => {
                    self.advance();
                    parts.extend(parse_fstring_parts(&more, start)?);
                }
                _ => return Ok(()),
            }
        }
    }

    fn parse_primary(&mut self) -> Result<Expr, ParseError> {
        let start = self.peek_span();

        match self.peek().clone() {
            Token::Integer(n) => {
                self.advance();
                Ok(Expr::int(n, start))
            }
            Token::Float(n) => {
                self.advance();
                Ok(Expr::new(ExprKind::FloatLiteral(n), start))
            }
            Token::Imaginary(n) => {
                self.advance();
                Ok(Expr::new(ExprKind::ImagLiteral(n), start))
            }
            Token::String_(s) => {
                self.advance();
                // Python implicit string literal concatenation: `"a" "b"` → `"ab"`,
                // including f-string participants (#109): `'a' f"{x}b"` becomes ONE
                // f-string whose parts are the concatenation of every literal.
                let mut joined = s;
                loop {
                    match self.peek().clone() {
                        Token::String_(more) => {
                            joined.push_str(&more);
                            self.advance();
                        }
                        Token::FString(_) => {
                            // Promote the plain prefix to an f-string literal
                            // part and continue in f-string mode.
                            let mut parts = vec![pyths_syntax::ast::FStringPart::Literal(joined)];
                            self.parse_adjacent_string_parts(&mut parts, start)?;
                            return Ok(Expr::new(ExprKind::FString { parts }, start));
                        }
                        _ => break,
                    }
                }
                Ok(Expr::string(joined, start))
            }
            Token::FString(s) => {
                self.advance();
                let mut parts = parse_fstring_parts(&s, start)?;
                // #109: implicit concatenation continues across adjacent
                // String_/FString tokens.
                self.parse_adjacent_string_parts(&mut parts, start)?;
                Ok(Expr::new(ExprKind::FString { parts }, start))
            }
            Token::True_ => {
                self.advance();
                Ok(Expr::bool_lit(true, start))
            }
            Token::False => {
                self.advance();
                Ok(Expr::bool_lit(false, start))
            }
            Token::None_ => {
                self.advance();
                Ok(Expr::none(start))
            }
            Token::Identifier(name) => {
                self.advance();
                Ok(Expr::name(name, start))
            }
            Token::LParen => {
                self.advance();
                if self.at(&Token::RParen) {
                    let end = self.peek_span();
                    self.advance();
                    return Ok(Expr::new(ExprKind::Tuple(vec![]), start.merge(end)));
                }
                let expr = self.parse_expr()?;
                // Check for tuple
                if self.eat(&Token::Comma) {
                    let mut elts = vec![expr];
                    while !self.at(&Token::RParen) {
                        elts.push(self.parse_expr()?);
                        if !self.eat(&Token::Comma) {
                            break;
                        }
                    }
                    let end = self.peek_span();
                    self.expect(&Token::RParen)?;
                    return Ok(Expr::new(ExprKind::Tuple(elts), start.merge(end)));
                }
                // Check for generator expression
                if self.at_comprehension_start() {
                    let generators = self.parse_comprehension_clauses()?;
                    let end = self.peek_span();
                    self.expect(&Token::RParen)?;
                    return Ok(Expr::new(
                        ExprKind::GeneratorExp {
                            elt: Box::new(expr),
                            generators,
                        },
                        start.merge(end),
                    ));
                }
                let end = self.peek_span();
                self.expect(&Token::RParen)?;
                // Parenthesized expression — return the inner expr with updated span
                Ok(Expr::new(expr.kind, start.merge(end)))
            }
            Token::LBracket => {
                self.advance();
                if self.at(&Token::RBracket) {
                    let end = self.peek_span();
                    self.advance();
                    return Ok(Expr::new(ExprKind::List(vec![]), start.merge(end)));
                }
                let first = self.parse_expr()?;
                // Check for list comprehension
                if self.at_comprehension_start() {
                    let generators = self.parse_comprehension_clauses()?;
                    let end = self.peek_span();
                    self.expect(&Token::RBracket)?;
                    return Ok(Expr::new(
                        ExprKind::ListComp {
                            elt: Box::new(first),
                            generators,
                        },
                        start.merge(end),
                    ));
                }
                // Regular list
                let mut elts = vec![first];
                while self.eat(&Token::Comma) {
                    if self.at(&Token::RBracket) {
                        break;
                    }
                    elts.push(self.parse_expr()?);
                }
                let end = self.peek_span();
                self.expect(&Token::RBracket)?;
                Ok(Expr::new(ExprKind::List(elts), start.merge(end)))
            }
            Token::LBrace => {
                self.advance();
                if self.at(&Token::RBrace) {
                    let end = self.peek_span();
                    self.advance();
                    return Ok(Expr::new(
                        ExprKind::Dict { items: vec![] },
                        start.merge(end),
                    ));
                }
                // Check if first item is a spread (**expr)
                if self.at(&Token::DoubleStar) {
                    self.advance();
                    let spread_val = self.parse_expr()?;
                    let mut items = vec![DictItem::Spread(spread_val)];
                    while self.eat(&Token::Comma) {
                        if self.at(&Token::RBrace) {
                            break;
                        }
                        if self.eat(&Token::DoubleStar) {
                            items.push(DictItem::Spread(self.parse_expr()?));
                        } else {
                            let key = self.parse_expr()?;
                            self.expect(&Token::Colon)?;
                            let value = self.parse_expr()?;
                            items.push(DictItem::KeyValue { key, value });
                        }
                    }
                    let end = self.peek_span();
                    self.expect(&Token::RBrace)?;
                    return Ok(Expr::new(ExprKind::Dict { items }, start.merge(end)));
                }
                let first = self.parse_expr()?;
                if self.eat(&Token::Colon) {
                    // Dict literal or dict comprehension
                    let first_val = self.parse_expr()?;
                    if self.at_comprehension_start() {
                        let generators = self.parse_comprehension_clauses()?;
                        let end = self.peek_span();
                        self.expect(&Token::RBrace)?;
                        return Ok(Expr::new(
                            ExprKind::DictComp {
                                key: Box::new(first),
                                value: Box::new(first_val),
                                generators,
                            },
                            start.merge(end),
                        ));
                    }
                    let mut items = vec![DictItem::KeyValue {
                        key: first,
                        value: first_val,
                    }];
                    while self.eat(&Token::Comma) {
                        if self.at(&Token::RBrace) {
                            break;
                        }
                        if self.eat(&Token::DoubleStar) {
                            items.push(DictItem::Spread(self.parse_expr()?));
                        } else {
                            let key = self.parse_expr()?;
                            self.expect(&Token::Colon)?;
                            let value = self.parse_expr()?;
                            items.push(DictItem::KeyValue { key, value });
                        }
                    }
                    let end = self.peek_span();
                    self.expect(&Token::RBrace)?;
                    Ok(Expr::new(ExprKind::Dict { items }, start.merge(end)))
                } else {
                    // Set literal or set comprehension
                    if self.at_comprehension_start() {
                        let generators = self.parse_comprehension_clauses()?;
                        let end = self.peek_span();
                        self.expect(&Token::RBrace)?;
                        return Ok(Expr::new(
                            ExprKind::SetComp {
                                elt: Box::new(first),
                                generators,
                            },
                            start.merge(end),
                        ));
                    }
                    let mut elts = vec![first];
                    while self.eat(&Token::Comma) {
                        if self.at(&Token::RBrace) {
                            break;
                        }
                        elts.push(self.parse_expr()?);
                    }
                    let end = self.peek_span();
                    self.expect(&Token::RBrace)?;
                    Ok(Expr::new(ExprKind::Set(elts), start.merge(end)))
                }
            }
            Token::Star => {
                self.advance();
                let inner = self.parse_expr()?;
                let span = start.merge(inner.span);
                Ok(Expr::new(ExprKind::Starred(Box::new(inner)), span))
            }
            Token::Yield => {
                self.advance();
                if self.eat(&Token::From) {
                    let value = self.parse_expr()?;
                    let span = start.merge(value.span);
                    Ok(Expr::new(ExprKind::YieldFrom(Box::new(value)), span))
                } else if self.at(&Token::Newline)
                    || self.at(&Token::RParen)
                    || self.at(&Token::Eof)
                {
                    Ok(Expr::new(ExprKind::Yield(None), start))
                } else {
                    // `yield a, b` yields a tuple — same testlist rule as
                    // return (issue #200). `yield from` above stays single.
                    let value = self.parse_expr_list()?;
                    let span = start.merge(value.span);
                    Ok(Expr::new(ExprKind::Yield(Some(Box::new(value))), span))
                }
            }
            Token::Ellipsis => {
                self.advance();
                // Treat ... as a special name for now
                Ok(Expr::name("...", start))
            }
            _ => Err(self.error(format!("Unexpected token: {}", self.peek()))),
        }
    }

    fn parse_comprehension_clauses(&mut self) -> Result<Vec<Comprehension>, ParseError> {
        let mut generators = Vec::new();
        loop {
            // Accept either `for ...` or `async for ...` as the
            // start of a comprehension clause. Bare `async` is only
            // valid here when followed by `for`.
            let is_async = if self.peek() == &Token::Async {
                // Lookahead: confirm the next token is `for` before
                // consuming `async` — otherwise leave it for the
                // outer parser (where it might start an async-def).
                let saved = self.pos;
                self.advance();
                if self.peek() == &Token::For {
                    self.advance();
                    true
                } else {
                    self.pos = saved;
                    break;
                }
            } else if self.eat(&Token::For) {
                false
            } else {
                break;
            };
            let target = self.parse_target_list()?;
            self.expect(&Token::In)?;
            let iter = self.parse_or()?; // Not full expression to avoid ambiguity
            let mut ifs = Vec::new();
            while self.eat(&Token::If) {
                ifs.push(self.parse_or()?);
            }
            generators.push(Comprehension {
                target,
                iter,
                ifs,
                is_async,
            });
        }
        Ok(generators)
    }
}

/// Parse f-string contents into literal and expression parts.
fn parse_fstring_parts(s: &str, _base_span: Span) -> Result<Vec<FStringPart>, ParseError> {
    let mut parts = Vec::new();
    let mut chars = s.chars().peekable();
    let mut current_literal = String::new();

    while let Some(ch) = chars.next() {
        if ch == '{' {
            if chars.peek() == Some(&'{') {
                // Escaped brace
                chars.next();
                current_literal.push('{');
            } else {
                // Start of expression
                if !current_literal.is_empty() {
                    // #95: decode escapes in the LITERAL part only —
                    // after brace-splitting, so `\x7b` can't fabricate an
                    // expression part; expression bodies are re-lexed as
                    // normal source (their nested string literals decode
                    // in the sub-lexer).
                    parts.push(FStringPart::Literal(pyths_lexer::decode_py_escapes(
                        &std::mem::take(&mut current_literal),
                    )));
                }
                let mut expr_str = String::new();
                let mut depth = 1;
                while let Some(ch) = chars.next() {
                    if ch == '{' {
                        depth += 1;
                    } else if ch == '}' {
                        depth -= 1;
                        if depth == 0 {
                            break;
                        }
                    } else if ch == '\'' || ch == '"' {
                        // PBT-3 (PEP 701): a nested string literal inside the
                        // expression part — copy it verbatim so braces (and
                        // the outer quote) inside it don't affect depth:
                        // f'{d['}']}' selects key "}" like CPython.
                        expr_str.push(ch);
                        while let Some(c2) = chars.next() {
                            expr_str.push(c2);
                            if c2 == '\\' {
                                if let Some(c3) = chars.next() {
                                    expr_str.push(c3);
                                }
                            } else if c2 == ch {
                                break;
                            }
                        }
                        continue;
                    }
                    expr_str.push(ch);
                }
                // PBT-3: CPython rejects an unterminated expression part
                // (`f'{x`) with "expecting '}'" — the old lenient fallthrough
                // parsed the tail as an expression, silently accepting
                // malformed source the formal grammar (rightly) rejects.
                if depth != 0 {
                    return Err(ParseError {
                        message: "f-string: expecting '}'".to_string(),
                        span: _base_span,
                        notes: vec![],
                    });
                }
                // Split off Python format-spec (everything after ':') if
                // present at top-level depth. Examples: `value:.2f` →
                // expr `value`, spec `.2f`. The expression body is parsed
                // as Python; the spec is lowered into a JS expression
                // wrapping the parsed expr (e.g., `.2f` → `.toFixed(2)`).
                let (raw0, format_spec) = split_fstring_format_spec(&expr_str);
                // Pythonic-checks sweep: `!r` / `!s` conversions (PEP 3101)
                // and self-documenting `{expr=}` (3.8+ debug specifier).
                let (raw1, mut conversion) = split_fstring_conversion(raw0);
                let mut raw_expr = raw1;
                {
                    // Self-doc: expression text ends with a bare `=` (not
                    // `==`/`<=`/`>=`/`!=`). CPython emits the source text
                    // (incl. the `=` and surrounding whitespace) as a
                    // literal, then repr(value) — or str via the spec
                    // when one is given.
                    let trimmed = raw1.trim_end();
                    let is_selfdoc = trimmed.ends_with('=')
                        && !trimmed.ends_with("==")
                        && !trimmed.ends_with("<=")
                        && !trimmed.ends_with(">=")
                        && !trimmed.ends_with("!=");
                    if is_selfdoc {
                        // Literal = the raw source text verbatim (no escape
                        // decoding — it's expression text, like CPython).
                        parts.push(FStringPart::Literal(raw1.to_string()));
                        raw_expr = &raw1[..trimmed.len() - 1];
                        if conversion.is_none() && format_spec.is_none() {
                            conversion = Some('r');
                        }
                    }
                }
                // Gen-eval round-5: CPython allows whitespace padding
                // inside the braces (`f"{ expr }"`) — the sub-lexer would
                // otherwise see the leading spaces as INDENT. Trim before
                // lexing. (Self-doc `{expr=}` text was captured verbatim
                // above, so trimming here cannot corrupt it.)
                let raw_expr = raw_expr.trim();
                let tokens = pyths_lexer::lex(raw_expr).map_err(|e| ParseError {
                    message: format!("Error in f-string expression: {}", e.message),
                    span: _base_span,
                    notes: vec![],
                })?;
                let mut parser = Parser::new(tokens, raw_expr);
                let expr = parser.parse_expr().map_err(|e| ParseError {
                    message: format!("Error in f-string expression: {}", e.message),
                    span: _base_span,
                    notes: vec![],
                })?;
                // Apply conversion BEFORE the format spec (CPython order:
                // the spec formats the converted string).
                let expr = match conversion {
                    Some('r') => wrap_fstring_helper("pyRepr", expr, _base_span),
                    Some('s') => wrap_fstring_helper("pyStr", expr, _base_span),
                    _ => expr,
                };
                let final_expr = if let Some(spec_str) = format_spec {
                    apply_fstring_format_spec(expr, spec_str, _base_span)?
                } else {
                    expr
                };
                parts.push(FStringPart::Expr(final_expr));
            }
        } else if ch == '}' {
            if chars.peek() == Some(&'}') {
                chars.next();
                current_literal.push('}');
            } else {
                current_literal.push('}');
            }
        } else {
            current_literal.push(ch);
        }
    }

    if !current_literal.is_empty() {
        // #95: decode escapes in the trailing literal part (see above).
        parts.push(FStringPart::Literal(pyths_lexer::decode_py_escapes(
            &current_literal,
        )));
    }

    Ok(parts)
}

/// Split an f-string brace body into `(expression, optional format_spec)`.
/// Format-spec begins after the *first* unbracketed `:` at depth 0. The
/// expression itself may contain a `:` (e.g. dict/slice), so we only
/// split when not inside `[]`/`()`/`{}`.
fn split_fstring_format_spec(body: &str) -> (&str, Option<&str>) {
    let bytes = body.as_bytes();
    let mut depth_paren = 0i32;
    let mut depth_brack = 0i32;
    let mut depth_brace = 0i32;
    let mut i = 0usize;
    while i < bytes.len() {
        let b = bytes[i];
        match b {
            // PBT-3 (PEP 701): skip nested string literals — a `:` inside
            // one (f'{'a:b'}') is string content, not a format spec.
            b'\'' | b'"' => {
                let q = b;
                i += 1;
                while i < bytes.len() {
                    if bytes[i] == b'\\' {
                        i += 2;
                        continue;
                    }
                    if bytes[i] == q {
                        break;
                    }
                    i += 1;
                }
            }
            b'(' => depth_paren += 1,
            b')' => depth_paren -= 1,
            b'[' => depth_brack += 1,
            b']' => depth_brack -= 1,
            b'{' => depth_brace += 1,
            b'}' => depth_brace -= 1,
            b':' if depth_paren == 0 && depth_brack == 0 && depth_brace == 0 => {
                // f-string conversion `!r`/`!s`/`!a` is also legal Python
                // but we don't support it; for now only split on `:`.
                return (&body[..i], Some(&body[i + 1..]));
            }
            _ => {}
        }
        i += 1;
    }
    (body, None)
}

/// Split a trailing f-string conversion (`!r` / `!s` / `!a`) off the
/// expression body (after the format-spec split). Pythonic-checks sweep.
/// `!a` is left in place (unsupported — ascii() escaping is not
/// implemented; it will surface as a lex error like before).
fn split_fstring_conversion(body: &str) -> (&str, Option<char>) {
    let b = body.trim_end();
    let bytes = b.as_bytes();
    if b.len() >= 2 && bytes[b.len() - 2] == b'!' {
        let last = bytes[b.len() - 1];
        if last == b'r' || last == b's' {
            return (&b[..b.len() - 2], Some(last as char));
        }
    }
    (body, None)
}

/// Wrap `expr` in a call to a runtime helper (`pyRepr` / `pyStr`) — used
/// by f-string conversions. The codegen imports these names when it sees
/// them as bare Name nodes (same mechanism as pyFormatSpec/pyFixed).
fn wrap_fstring_helper(helper: &str, expr: Expr, span: Span) -> Expr {
    use pyths_syntax::ast::ExprKind as EK;
    Expr {
        kind: EK::Call {
            func: Box::new(Expr {
                kind: EK::Name(helper.to_string()),
                span,
            }),
            args: vec![expr],
            kwargs: vec![],
            optional: false,
        },
        span,
    }
}

/// Lower a Python format-spec to a JS expression that wraps the parsed
/// expression. Uses the PEP 3101 parser in `format_spec.rs`. For the
/// common simple cases (`.Nf`, `0Nd`, `,`) we emit direct JS forms
/// inline (no runtime helper needed) so the codegen output stays
/// readable. For complex specs (fill/align/sign/grouping/type combos)
/// we delegate to the runtime helper `pyFormatSpec(value, opts)`.
fn apply_fstring_format_spec(expr: Expr, spec_str: &str, span: Span) -> Result<Expr, ParseError> {
    use crate::format_spec::{is_noop, lower, parse, FormatSpec, FormatType};
    use pyths_syntax::ast::ExprKind as EK;

    // #108: dynamic format spec — nested braces (`{v:{w}}`, `{x:.{p}f}`)
    // mean the spec string is only known at runtime. Build the spec as
    // an FString and delegate the parse to the pyFormatDynamic runtime
    // helper (which mirrors format_spec.rs). Previously this fell into
    // the "unparseable → silently ignore" arm and printed the bare
    // value — wrong output with no diagnostic.
    if spec_str.contains('{') {
        return lower_dynamic_format_spec(expr, spec_str, span);
    }

    let spec = match parse(spec_str) {
        Some(s) if !is_noop(&s) => s,
        // Empty or unparseable spec — fall through (preserves prior
        // behavior of silently ignoring unrecognized specs).
        _ => return Ok(expr),
    };

    // Direct-emission fast paths for the most common shapes — keep the
    // generated JS small + the runtime helper unimported.
    if let Some(direct) = try_direct_emission(&spec, &expr, span) {
        return Ok(direct);
    }

    // Anything else goes through the runtime helper.
    let _ = (FormatSpec::default, FormatType::FixedLower);
    let _ = EK::IntLiteral as fn(i128) -> EK;
    Ok(lower(&spec, expr, span))
}

/// #108: lower a dynamic format spec (`{w}`, `.{p}f`, `{a}{w}`) to
/// `pyFormatDynamic(value, <spec-as-fstring>)`. The spec template is
/// itself parsed like a miniature f-string: literal runs + `{expr}`
/// replacement fields (one nesting level, per the PEP 3101 grammar —
/// nested fields cannot contain further format specs).
fn lower_dynamic_format_spec(expr: Expr, spec_str: &str, span: Span) -> Result<Expr, ParseError> {
    use pyths_syntax::ast::{ExprKind as EK, FStringPart};

    let mut parts: Vec<FStringPart> = Vec::new();
    let mut literal = String::new();
    let mut chars = spec_str.chars().peekable();
    while let Some(ch) = chars.next() {
        if ch == '{' {
            if !literal.is_empty() {
                parts.push(FStringPart::Literal(std::mem::take(&mut literal)));
            }
            let mut inner = String::new();
            let mut depth = 1usize;
            for c in chars.by_ref() {
                if c == '{' {
                    depth += 1;
                } else if c == '}' {
                    depth -= 1;
                    if depth == 0 {
                        break;
                    }
                }
                inner.push(c);
            }
            if depth != 0 {
                return Err(ParseError {
                    message: format!(
                        "Unterminated replacement field in f-string format spec: {:?}",
                        spec_str
                    ),
                    span,
                    notes: vec![],
                });
            }
            let tokens = pyths_lexer::lex(&inner).map_err(|e| ParseError {
                message: format!("Error in f-string format-spec expression: {}", e.message),
                span,
                notes: vec![],
            })?;
            let mut parser = Parser::new(tokens, &inner);
            let inner_expr = parser.parse_expr().map_err(|e| ParseError {
                message: format!("Error in f-string format-spec expression: {}", e.message),
                span,
                notes: vec![],
            })?;
            parts.push(FStringPart::Expr(inner_expr));
        } else {
            literal.push(ch);
        }
    }
    if !literal.is_empty() {
        parts.push(FStringPart::Literal(literal));
    }

    let spec_fstring = Expr {
        kind: EK::FString { parts },
        span,
    };
    Ok(Expr {
        kind: EK::Call {
            func: Box::new(Expr {
                kind: EK::Name("pyFormatDynamic".to_string()),
                span,
            }),
            args: vec![expr, spec_fstring],
            kwargs: vec![],
            optional: false,
        },
        span,
    })
}

/// Fast paths that emit JS inline without the runtime helper:
///   `.Nf` / `.NF` → `pyFixed(expr, N)` (CPython round-half-even)
/// Returns None when the spec needs the full helper (mixes flags).
/// (The former 0Nd/padStart and `,`/toLocaleString fast paths were
/// removed in the Pythonic-checks sweep — see NOTEs below.)
fn try_direct_emission(
    spec: &crate::format_spec::FormatSpec,
    expr: &Expr,
    span: Span,
) -> Option<Expr> {
    use crate::format_spec::{FormatType, Sign};
    use pyths_syntax::ast::ExprKind as EK;

    // Reject any combination — direct emission only for "pure" shapes.
    let mixed_flags = spec.fill.is_some()
        || spec.align.is_some()
        || matches!(spec.sign, Some(Sign::Plus | Sign::Space))
        || spec.alt_form;
    if mixed_flags {
        return None;
    }

    // .<N>f / .<N>F
    // #86: emits `pyFixed(expr, N)` — a runtime helper implementing
    // CPython's round-half-to-even on the exact decimal value of the
    // double. (Was `expr.toFixed(N)`, whose exact ties round away from
    // zero: f"{1.625:.2f}" gave '1.63' vs CPython's '1.62'.)
    if matches!(
        spec.ty,
        Some(FormatType::FixedLower | FormatType::FixedUpper)
    ) && !spec.zero_pad
        && spec.width.is_none()
        && spec.grouping.is_none()
    {
        if let Some(n) = spec.precision {
            return Some(Expr {
                kind: EK::Call {
                    func: Box::new(Expr {
                        kind: EK::Name("pyFixed".to_string()),
                        span,
                    }),
                    args: vec![
                        expr.clone(),
                        Expr {
                            kind: EK::IntLiteral(n as i128),
                            span,
                        },
                    ],
                    kwargs: vec![],
                    optional: false,
                },
                span,
            });
        }
    }

    // NOTE (Pythonic-checks sweep): the former `0Nd → String(x).padStart`
    // fast path was removed — it was sign-unaware (f"{-42:05}" gave
    // "00-42"; CPython gives "-0042"). Zero-pad now routes through
    // pyFormatSpec, whose `=`-align handling pads between sign and digits.

    // NOTE (Pythonic-checks sweep): the former `,`/`,d` →
    // toLocaleString("en-US") fast path was removed — on float values
    // toLocaleString truncates to 3 fraction digits (f"{1234.5678:,}"
    // gave "1,234.568"; CPython keeps "1,234.5678"). Grouping now routes
    // through pyFormatSpec, which separates the integer part only.

    None
}

/// Suggest a corrected keyword for common typos (edit distance 1-2).
pub fn suggest_keyword(name: &str) -> Option<&'static str> {
    match name {
        "defn" | "dfe" | "deff" | "ddef" => Some("def"),
        "clas" | "classs" | "calss" => Some("class"),
        "pritn" | "pirnt" | "prnt" | "prrint" => Some("print"),
        "retrun" | "retunr" | "reutrn" | "retrn" => Some("return"),
        "improt" | "ipmort" | "imoprt" | "imprt" => Some("import"),
        "ture" | "Ture" | "treu" => Some("True"),
        "flase" | "Flase" | "fasle" => Some("False"),
        "elseif" | "elseIf" | "else_if" => Some("elif"),
        "wihle" | "whlie" | "whiel" => Some("while"),
        "brek" | "braek" => Some("break"),
        "contnue" | "contniue" => Some("continue"),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse_ok(source: &str) -> Module {
        crate::parse(source).expect("Parse failed")
    }

    // #233: `match` is a soft keyword — usable as an identifier (function name,
    // variable, for-target, call) while still introducing a match statement as
    // the leading word of `match SUBJECT:`. Common with `re.match`/`re.search`.
    #[test]
    fn test_match_soft_keyword_as_identifier() {
        // as a function name
        assert!(matches!(
            &parse_ok("def match(t):\n    return t").body[0].kind,
            StmtKind::FuncDef { .. }
        ));
        // as an assignment target + attribute access
        let a = parse_ok("m = 1\nmatch = m\nx = match.start");
        assert_eq!(a.body.len(), 3);
        assert!(matches!(&a.body[1].kind, StmtKind::Assign { .. }));
        // as a for-loop target
        assert!(matches!(
            &parse_ok("for match in items:\n    pass").body[0].kind,
            StmtKind::For { .. }
        ));
        // as a call
        assert!(matches!(
            &parse_ok("y = match(text)").body[0].kind,
            StmtKind::Assign { .. }
        ));
        // still a match STATEMENT when it leads `match SUBJECT:`
        assert!(matches!(
            &parse_ok("match x:\n    case 1:\n        pass").body[0].kind,
            StmtKind::Match { .. }
        ));
    }

    // #288 (C): a list-literal / parenthesised SUBJECT is a match statement
    // when the logical line ends with a top-level colon — previously
    // auto-rejected as a subscript/call of a variable named `match`
    // ("Unexpected token: INDENT").
    #[test]
    fn test_match_bracketed_subject_vs_subscript() {
        assert!(matches!(
            &parse_ok("match [7]:\n    case [x]:\n        pass").body[0].kind,
            StmtKind::Match { .. }
        ));
        assert!(matches!(
            &parse_ok("match (8):\n    case y:\n        pass").body[0].kind,
            StmtKind::Match { .. }
        ));
        assert!(matches!(
            &parse_ok("match [1, 2, 3]:\n    case [a, *rest]:\n        pass").body[0].kind,
            StmtKind::Match { .. }
        ));
        // ...but `match` as a NAME keeps subscript / slice / call / annotated
        // forms as expression statements (no trailing top-level colon, or a
        // mid-line colon).
        assert!(matches!(
            &parse_ok("x = 1\nsub = match[1]").body[1].kind,
            StmtKind::Assign { .. }
        ));
        assert!(matches!(
            &parse_ok("sl = match[1:]").body[0].kind,
            StmtKind::Assign { .. }
        ));
        assert!(!matches!(
            &parse_ok("match(a, b)").body[0].kind,
            StmtKind::Match { .. }
        ));
        assert!(!matches!(
            &parse_ok("match[7] = 5").body[0].kind,
            StmtKind::Match { .. }
        ));
    }

    // #323: parenthesized tuple pattern `case (1, 2):` and the bare form
    // `case 1, 2:` both parse as Sequence patterns; `case (1):` stays a
    // grouping (the inner literal, not a 1-element sequence).
    #[test]
    fn test_case_tuple_patterns() {
        fn first_case_pattern(src: &str) -> Pattern {
            match &parse_ok(src).body[0].kind {
                StmtKind::Match { cases, .. } => cases[0].pattern.clone(),
                other => panic!("expected match, got {:?}", other),
            }
        }
        assert!(matches!(
            first_case_pattern("match v:\n    case (1, 2):\n        pass"),
            Pattern::Sequence(ref ps) if ps.len() == 2
        ));
        assert!(matches!(
            first_case_pattern("match v:\n    case 1, 2:\n        pass"),
            Pattern::Sequence(ref ps) if ps.len() == 2
        ));
        assert!(matches!(
            first_case_pattern("match v:\n    case (a,):\n        pass"),
            Pattern::Sequence(ref ps) if ps.len() == 1
        ));
        assert!(matches!(
            first_case_pattern("match v:\n    case ():\n        pass"),
            Pattern::Sequence(ref ps) if ps.is_empty()
        ));
        // `(1)` is a grouping — the inner literal, NOT a 1-tuple.
        assert!(matches!(
            first_case_pattern("match v:\n    case (1):\n        pass"),
            Pattern::Literal(_)
        ));
    }

    // #218: `;`-separated and trailing semicolons on a logical line, outside a
    // one-line suite (module level and inside a function/indented block).
    #[test]
    fn test_semicolon_separated_statements() {
        assert_eq!(parse_ok("x = 1; y = 2").body.len(), 2);
        assert_eq!(parse_ok("x = 5;").body.len(), 1); // trailing `;`
                                                      // inside an indented block
        let m = parse_ok("def f():\n    a = 1; b = 2\n    return a + b");
        match &m.body[0].kind {
            StmtKind::FuncDef { body, .. } => assert_eq!(body.len(), 3),
            _ => panic!("expected function def"),
        }
    }

    #[test]
    fn test_parse_print() {
        let module = parse_ok("print(\"hello world\")");
        assert_eq!(module.body.len(), 1);
        match &module.body[0].kind {
            StmtKind::Expr(Expr {
                kind: ExprKind::Call { func, args, .. },
                ..
            }) => {
                assert!(matches!(&func.kind, ExprKind::Name(n) if n == "print"));
                assert_eq!(args.len(), 1);
                assert!(matches!(&args[0].kind, ExprKind::StringLiteral(s) if s == "hello world"));
            }
            other => panic!("Expected call, got: {:?}", other),
        }
    }

    #[test]
    fn test_keyword_attr_names() {
        // JS-interop deviation (Track-A sweep): keywords are legal attribute
        // names after `.` / `?.` — `Promise...finally(cb)`, `"s".match(re)`,
        // JS objects with `class` / `import` members. Reserved elsewhere.
        for src in [
            "p.finally(cb)",
            "s.match(r)",
            "x?.finally",
            "a.class",
            "q.import",
            "obj.lambda.try.if",
        ] {
            parse_ok(src);
        }
        let m = parse_ok("p.finally(cb)");
        match &m.body[0].kind {
            StmtKind::Expr(Expr {
                kind: ExprKind::Call { func, .. },
                ..
            }) => {
                assert!(
                    matches!(&func.kind, ExprKind::Attribute { attr, .. } if attr == "finally")
                );
            }
            other => panic!("Expected call, got: {:?}", other),
        }
        // Keywords stay reserved outside attribute position.
        assert!(crate::parse("finally = 1").is_err());
        assert!(crate::parse("def class(): pass").is_err());
    }

    #[test]
    fn test_star_target_in_for_and_comprehension() {
        // Round-2 pythonic sweep: starred for-targets parse in plain for
        // statements and comprehension clauses.
        let module = parse_ok(
            "for a, *bs in [[1, 2, 3]]:
    print(a, bs)",
        );
        match &module.body[0].kind {
            StmtKind::For { target, .. } => match &target.kind {
                ExprKind::Tuple(elts) => {
                    assert!(matches!(&elts[0].kind, ExprKind::Name(n) if n == "a"));
                    assert!(matches!(&elts[1].kind, ExprKind::Starred(_)));
                }
                other => panic!("Expected tuple target, got: {:?}", other),
            },
            other => panic!("Expected for, got: {:?}", other),
        }
        let module = parse_ok("ys = [x + y for x, *rest in pairs for y in rest]");
        assert_eq!(module.body.len(), 1);
    }

    #[test]
    fn test_python2_print_statement_rejected() {
        // F7: `print "x"` (Python-2 syntax) must produce a targeted parse
        // error, not silently compile to `print; "x";`.
        let errs = crate::parse("print \"hello\"").expect_err("expected parse error");
        assert!(
            errs.iter().any(|e| e.message
                == "Python 2-style print statement — use print(...) instead. \
                    In Python 3 (and PythScribe), print is a function: write \
                    `print(x)`, not `print x`."),
            "expected the Python-2 print diagnostic, got: {:?}",
            errs.iter().map(|e| &e.message).collect::<Vec<_>>()
        );
    }

    #[test]
    fn test_python2_print_name_and_number_rejected() {
        // Non-string operands (`print x`, `print 1`) are also Python-2 syntax.
        assert!(crate::parse("print x").is_err());
        assert!(crate::parse("print 42").is_err());
        // A real Python-3 call must still parse cleanly.
        assert!(crate::parse("print(\"ok\")").is_ok());
        // A bare `print` (the function object) is legal and must not error.
        assert!(crate::parse("print").is_ok());
    }

    #[test]
    fn test_parse_assignment() {
        let module = parse_ok("x = 42");
        assert_eq!(module.body.len(), 1);
        match &module.body[0].kind {
            StmtKind::Assign { targets, value } => {
                assert!(matches!(&targets[0].kind, ExprKind::Name(n) if n == "x"));
                assert!(matches!(&value.kind, ExprKind::IntLiteral(42)));
            }
            other => panic!("Expected assign, got: {:?}", other),
        }
    }

    #[test]
    fn test_parse_func_def() {
        let module = parse_ok("def greet(name):\n    print(name)");
        assert_eq!(module.body.len(), 1);
        match &module.body[0].kind {
            StmtKind::FuncDef {
                name, params, body, ..
            } => {
                assert_eq!(name, "greet");
                assert_eq!(params.len(), 1);
                assert_eq!(params[0].name, "name");
                assert_eq!(body.len(), 1);
            }
            other => panic!("Expected funcdef, got: {:?}", other),
        }
    }

    #[test]
    fn test_parse_if_else() {
        let module = parse_ok("if x > 0:\n    y = 1\nelse:\n    y = 2");
        assert_eq!(module.body.len(), 1);
        match &module.body[0].kind {
            StmtKind::If {
                body, else_body, ..
            } => {
                assert_eq!(body.len(), 1);
                assert!(else_body.is_some());
            }
            other => panic!("Expected if, got: {:?}", other),
        }
    }

    #[test]
    fn test_parse_for_loop() {
        let module = parse_ok("for i in range(10):\n    print(i)");
        assert_eq!(module.body.len(), 1);
        assert!(matches!(&module.body[0].kind, StmtKind::For { .. }));
    }

    #[test]
    fn test_parse_list_literal() {
        let module = parse_ok("x = [1, 2, 3]");
        match &module.body[0].kind {
            StmtKind::Assign { value, .. } => {
                assert!(matches!(&value.kind, ExprKind::List(elts) if elts.len() == 3));
            }
            other => panic!("Expected assign, got: {:?}", other),
        }
    }

    #[test]
    fn test_parse_list_comprehension() {
        let module = parse_ok("x = [i * 2 for i in range(10)]");
        match &module.body[0].kind {
            StmtKind::Assign { value, .. } => {
                assert!(matches!(&value.kind, ExprKind::ListComp { .. }));
            }
            other => panic!("Expected assign with listcomp, got: {:?}", other),
        }
    }

    #[test]
    fn test_parse_class() {
        let module = parse_ok("class Dog:\n    def bark(self):\n        print(\"woof\")");
        assert_eq!(module.body.len(), 1);
        match &module.body[0].kind {
            StmtKind::ClassDef { name, body, .. } => {
                assert_eq!(name, "Dog");
                assert_eq!(body.len(), 1);
            }
            other => panic!("Expected classdef, got: {:?}", other),
        }
    }

    #[test]
    fn test_parse_binary_ops() {
        let module = parse_ok("x = 1 + 2 * 3");
        match &module.body[0].kind {
            StmtKind::Assign { value, .. } => {
                // Should be 1 + (2 * 3) due to precedence
                match &value.kind {
                    ExprKind::BinOp {
                        op: BinOp::Add,
                        right,
                        ..
                    } => {
                        assert!(matches!(
                            &right.kind,
                            ExprKind::BinOp { op: BinOp::Mul, .. }
                        ));
                    }
                    other => panic!("Expected binop, got: {:?}", other),
                }
            }
            other => panic!("Expected assign, got: {:?}", other),
        }
    }

    #[test]
    fn test_parse_fstring() {
        let module = parse_ok("x = f\"hello {name}\"");
        match &module.body[0].kind {
            StmtKind::Assign { value, .. } => match &value.kind {
                ExprKind::FString { parts } => {
                    assert_eq!(parts.len(), 2);
                    assert!(matches!(&parts[0], FStringPart::Literal(s) if s == "hello "));
                    assert!(matches!(&parts[1], FStringPart::Expr(_)));
                }
                other => panic!("Expected fstring, got: {:?}", other),
            },
            other => panic!("Expected assign, got: {:?}", other),
        }
    }

    #[test]
    fn test_case_is_soft_keyword() {
        // #79: `case` is only special inside a match suite — it must work
        // as an ordinary identifier everywhere else (CPython soft keyword).
        for src in [
            "case = 1",
            "def f(case):\n    return case",
            "def case():\n    return 1",
            "x = case + 1",
        ] {
            parse_ok(src);
        }
        // ... while match/case statements still parse.
        let module = parse_ok("match x:\n    case 1:\n        pass\n    case _:\n        pass");
        assert!(matches!(&module.body[0].kind, StmtKind::Match { cases, .. } if cases.len() == 2));
    }

    #[test]
    fn test_chained_assignment_parses() {
        // #99: `a = b = 5` — both leading exprs become targets.
        let module = parse_ok("a = b = 5");
        match &module.body[0].kind {
            StmtKind::Assign { targets, value } => {
                assert_eq!(targets.len(), 2);
                assert!(matches!(&targets[0].kind, ExprKind::Name(n) if n == "a"));
                assert!(matches!(&targets[1].kind, ExprKind::Name(n) if n == "b"));
                assert!(matches!(&value.kind, ExprKind::IntLiteral(5)));
            }
            other => panic!("Expected assign, got: {:?}", other),
        }
    }

    #[test]
    fn test_del_postfix_targets_parse() {
        // #101: del must accept subscript and attribute targets (they
        // previously split into `del d` + a detached `["k"];` statement).
        let module = parse_ok("del d[\"k\"]");
        match &module.body[0].kind {
            StmtKind::Del(targets) => {
                assert_eq!(targets.len(), 1);
                assert!(matches!(&targets[0].kind, ExprKind::Subscript { .. }));
            }
            other => panic!("Expected del, got: {:?}", other),
        }
        assert_eq!(parse_ok("del d[\"k\"]").body.len(), 1);

        let module2 = parse_ok("del obj.attr");
        match &module2.body[0].kind {
            StmtKind::Del(targets) => {
                assert!(matches!(&targets[0].kind, ExprKind::Attribute { .. }));
            }
            other => panic!("Expected del, got: {:?}", other),
        }
    }

    // --- Error recovery tests ---

    fn parse_errors(source: &str) -> Vec<crate::ParseError> {
        crate::parse(source).unwrap_err()
    }

    #[test]
    fn test_recovery_multiple_errors_in_block() {
        // Two errors inside a def body; both should be reported
        let source = "def foo():\n    x = $\n    y = @#\n    z = 1";
        let errors = parse_errors(source);
        assert!(
            errors.len() >= 2,
            "Expected at least 2 errors, got {}",
            errors.len()
        );
    }

    #[test]
    fn test_recovery_if_body_errors() {
        // Error inside an if body; rest of module should still try to parse
        let source = "if True:\n    x = $\nz = 3";
        let errors = parse_errors(source);
        assert!(!errors.is_empty());
    }

    #[test]
    fn test_recovery_class_body() {
        // Error in one method; rest of class should still parse
        let source =
            "class Foo:\n    def bar(self):\n        x = $\n    def baz(self):\n        y = 1";
        let errors = parse_errors(source);
        assert!(!errors.is_empty());
    }

    #[test]
    fn test_recovery_nested_blocks() {
        // Error deep in nesting; outer statements should still parse
        let source = "def outer():\n    if True:\n        x = $\n    y = 1\nz = 2";
        let errors = parse_errors(source);
        assert!(!errors.is_empty());
    }

    #[test]
    fn test_recovery_preserves_valid_statements() {
        // Module-level recovery: first stmt bad, second and third ok
        let source = "x = $\ny = 2\nz = 3";
        let errors = parse_errors(source);
        // Should have error for $, but still try to parse y and z
        assert!(!errors.is_empty());
    }

    #[test]
    fn test_lex_and_parse_errors_combined() {
        // Bad character (lex error) + parse error from the same source
        let source = "x = $\ndef foo(\n    pass";
        let errors = parse_errors(source);
        // Should get lex error for $ and parse error for incomplete def
        assert!(
            errors.len() >= 2,
            "Expected at least 2 errors, got {}: {:?}",
            errors.len(),
            errors
        );
    }

    #[test]
    fn test_recovery_empty_block_still_errors() {
        // Empty block should still produce an error
        let source = "def foo():\n\nz = 1";
        let errors = parse_errors(source);
        assert!(!errors.is_empty());
    }

    #[test]
    fn test_existing_module_recovery_preserved() {
        // Module-level recovery (existing behavior) still works
        let source = "x = 1\n@@@\ny = 2";
        let errors = parse_errors(source);
        assert!(!errors.is_empty());
    }

    // --- Contextual error message tests ---

    #[test]
    fn test_missing_colon_hint_if() {
        let source = "if x > 0\n    y = 1";
        let errors = parse_errors(source);
        assert!(!errors.is_empty());
        let has_hint = errors
            .iter()
            .any(|e| e.notes.iter().any(|n| n.contains("add ':'")));
        assert!(has_hint, "Expected colon hint in notes, got: {:?}", errors);
    }

    #[test]
    fn test_missing_colon_hint_def() {
        let source = "def foo()\n    pass";
        let errors = parse_errors(source);
        assert!(!errors.is_empty());
        let has_hint = errors
            .iter()
            .any(|e| e.notes.iter().any(|n| n.contains("add ':'")));
        assert!(has_hint, "Expected colon hint in notes, got: {:?}", errors);
    }

    #[test]
    fn test_missing_colon_hint_class() {
        let source = "class Foo\n    pass";
        let errors = parse_errors(source);
        assert!(!errors.is_empty());
        let has_hint = errors
            .iter()
            .any(|e| e.notes.iter().any(|n| n.contains("add ':'")));
        assert!(has_hint, "Expected colon hint in notes, got: {:?}", errors);
    }

    #[test]
    fn test_missing_colon_hint_for() {
        let source = "for i in range(10)\n    print(i)";
        let errors = parse_errors(source);
        assert!(!errors.is_empty());
        let has_hint = errors
            .iter()
            .any(|e| e.notes.iter().any(|n| n.contains("add ':'")));
        assert!(has_hint, "Expected colon hint in notes, got: {:?}", errors);
    }

    #[test]
    fn test_missing_colon_hint_while() {
        let source = "while True\n    pass";
        let errors = parse_errors(source);
        assert!(!errors.is_empty());
        let has_hint = errors
            .iter()
            .any(|e| e.notes.iter().any(|n| n.contains("add ':'")));
        assert!(has_hint, "Expected colon hint in notes, got: {:?}", errors);
    }

    #[test]
    fn test_missing_rparen_hint() {
        let source = "def foo(x\n    pass";
        let errors = parse_errors(source);
        assert!(!errors.is_empty());
        let has_hint = errors
            .iter()
            .any(|e| e.notes.iter().any(|n| n.contains("unmatched '('")));
        assert!(has_hint, "Expected rparen hint in notes, got: {:?}", errors);
    }

    #[test]
    fn test_suggest_keyword_def() {
        let source = "defn foo():\n    pass";
        let errors = parse_errors(source);
        assert!(!errors.is_empty());
        let has_suggestion = errors
            .iter()
            .any(|e| e.notes.iter().any(|n| n.contains("did you mean 'def'")));
        assert!(
            has_suggestion,
            "Expected 'def' suggestion, got: {:?}",
            errors
        );
    }

    #[test]
    fn test_suggest_keyword_return() {
        let source = "def foo():\n    retrun 1";
        let errors = parse_errors(source);
        assert!(!errors.is_empty());
        let has_suggestion = errors
            .iter()
            .any(|e| e.notes.iter().any(|n| n.contains("did you mean 'return'")));
        assert!(
            has_suggestion,
            "Expected 'return' suggestion, got: {:?}",
            errors
        );
    }

    #[test]
    fn test_improved_indent_message() {
        // Inconsistent indentation should include expected/actual space counts
        let source = "if True:\n    x = 1\n   y = 2";
        let errors = parse_errors(source);
        assert!(!errors.is_empty());
        let has_detail = errors
            .iter()
            .any(|e| e.message.contains("expected") && e.message.contains("found"));
        assert!(
            has_detail,
            "Expected detailed indent message, got: {:?}",
            errors
        );
    }

    #[test]
    fn test_notes_render_in_diagnostic() {
        // Verify that notes survive through ParseError to string representation
        let error = crate::ParseError {
            message: "test error".to_string(),
            span: pyths_syntax::span::Span::new(0, 1),
            notes: vec!["this is a hint".to_string()],
        };
        let display = format!("{}", error);
        assert!(
            display.contains("this is a hint"),
            "Notes should appear in display: {}",
            display
        );
    }

    // --- A2: side-effect string-literal import (`import "./styles.css"`) ---

    #[test]
    fn test_parse_import_side_effect_string() {
        let module = parse_ok("import \"./styles.css\"");
        assert_eq!(module.body.len(), 1);
        match &module.body[0].kind {
            StmtKind::ImportSideEffect(path) => assert_eq!(path, "./styles.css"),
            other => panic!("Expected ImportSideEffect, got: {:?}", other),
        }
    }

    #[test]
    fn test_parse_import_side_effect_arbitrary_extension() {
        // Codegen/parser don't validate the extension — bundler's job.
        let module = parse_ok("import \"./logo.png\"");
        match &module.body[0].kind {
            StmtKind::ImportSideEffect(path) => assert_eq!(path, "./logo.png"),
            other => panic!("Expected ImportSideEffect, got: {:?}", other),
        }
    }

    #[test]
    fn test_parse_regular_import_still_works() {
        // Normal `import module[, module ...]` grammar must not be loosened
        // or otherwise disturbed by the new bare-string form.
        let module = parse_ok("import os, sys as system");
        match &module.body[0].kind {
            StmtKind::Import { names } => {
                assert_eq!(names.len(), 2);
                assert_eq!(names[0].name, "os");
                assert_eq!(names[1].name, "sys");
                assert_eq!(names[1].alias.as_deref(), Some("system"));
            }
            other => panic!("Expected Import, got: {:?}", other),
        }
    }

    #[test]
    fn test_parse_from_import_still_works() {
        let module = parse_ok("from math import sqrt");
        assert!(matches!(&module.body[0].kind, StmtKind::ImportFrom { .. }));
    }

    // --- Parenthesized from-import list (`from x import (a, b,)`) ---

    /// Extract (module, names-as-(name, alias) pairs, level) from a module
    /// whose single statement is an ImportFrom — for AST-equality assertions
    /// that ignore spans.
    fn import_from_parts(module: &Module) -> (String, Vec<(String, Option<String>)>, u32) {
        match &module.body[0].kind {
            StmtKind::ImportFrom {
                module,
                names,
                level,
            } => (
                module.clone(),
                names
                    .iter()
                    .map(|a| (a.name.clone(), a.alias.clone()))
                    .collect(),
                *level,
            ),
            other => panic!("Expected ImportFrom, got: {:?}", other),
        }
    }

    #[test]
    fn test_parse_from_import_parenthesized_single_line() {
        let module = parse_ok("from x import (a, b)");
        let (mod_name, names, level) = import_from_parts(&module);
        assert_eq!(mod_name, "x");
        assert_eq!(level, 0);
        assert_eq!(
            names,
            vec![("a".to_string(), None), ("b".to_string(), None)]
        );
    }

    #[test]
    fn test_parse_from_import_parenthesized_same_ast_as_unparenthesized() {
        // Parens are purely syntactic — identical AST either way.
        let with_parens = parse_ok("from x import (a, b as c)");
        let without = parse_ok("from x import a, b as c");
        assert_eq!(import_from_parts(&with_parens), import_from_parts(&without));
    }

    #[test]
    fn test_parse_from_import_parenthesized_multiline_trailing_comma() {
        let src = "from .fixtures import (\n    PIECES,\n    KICKS,\n    SEED as seed,\n)";
        let module = parse_ok(src);
        let (mod_name, names, level) = import_from_parts(&module);
        assert_eq!(mod_name, "fixtures");
        assert_eq!(level, 1);
        assert_eq!(
            names,
            vec![
                ("PIECES".to_string(), None),
                ("KICKS".to_string(), None),
                ("SEED".to_string(), Some("seed".to_string())),
            ]
        );
    }

    #[test]
    fn test_parse_from_import_unclosed_paren_is_error() {
        let errors = parse_errors("from x import (a, b");
        assert!(!errors.is_empty(), "Expected error for unclosed paren");
    }

    #[test]
    fn test_parse_from_import_trailing_comma_without_parens_is_error() {
        // Trailing comma is only legal inside parentheses (matches CPython).
        let errors = parse_errors("from x import a, b,");
        assert!(
            !errors.is_empty(),
            "Expected error for trailing comma without parens"
        );
    }

    #[test]
    fn test_parse_plain_import_parenthesized_is_error() {
        // Python does not allow parens on a plain `import` statement.
        let errors = parse_errors("import (a)");
        assert!(!errors.is_empty(), "Expected error for 'import (a)'");
    }

    #[test]
    fn test_parse_import_side_effect_with_alias_is_error() {
        // `import "<string>" as x` is not a supported shape — a side-effect
        // import binds no name, so `as` makes no sense here.
        let errors = parse_errors("import \"./styles.css\" as x");
        assert!(
            !errors.is_empty(),
            "Expected a parse error for 'import \"...\" as x'"
        );
    }

    #[test]
    fn test_parse_import_side_effect_with_trailing_comma_is_error() {
        // Multiple bare-string imports on one line are not supported.
        let errors = parse_errors("import \"./a.css\", \"./b.css\"");
        assert!(
            !errors.is_empty(),
            "Expected a parse error for comma-separated string imports"
        );
    }

    #[test]
    fn test_parse_import_mixed_string_and_name_is_error() {
        // `import "x", os` — malformed mixed form must still error.
        let errors = parse_errors("import \"./a.css\", os");
        assert!(
            !errors.is_empty(),
            "Expected a parse error for mixed string/name import"
        );
    }

    #[test]
    fn test_bare_return_and_raise_accept_semicolon_terminator() {
        // Regression, found by scripts/grammar-fuzz.py (differential fuzzing
        // against grammar/pyths.lark).
        //
        // A one-line suite separates simple statements with `;`, so a bare
        // `return` / `raise` inside one is terminated by SEMICOLON rather than
        // NEWLINE. parse_return_stmt / parse_raise_stmt previously only
        // recognised NEWLINE and EOF as "no value here", so they fell through
        // to parse_expr() and failed with "Unexpected token: ;" — even though
        // `pass;`, `break;`, `continue;`, `del a;` and `return 1;` all worked.
        //
        // B12 note: `return` must be inside a function (a bare module-level
        // `return` is now correctly rejected as "'return' outside function"),
        // so the return cases are wrapped in a `def`. This test isolates the
        // `;`-terminator behavior, not scope validity. `raise` is legal at
        // module scope and stays there.
        for src in [
            "def f():\n    if x: return;",
            "if x: raise;",
            "def f():\n    if x: return; y = 1",
            "if x: raise; y = 1",
            "def f():\n    while x:\n        if y: return;\n",
            "def f():\n    if x: return;\n",
        ] {
            assert!(
                crate::parse(src).is_ok(),
                "expected `{}` to parse (bare return/raise before `;`)",
                src.escape_debug()
            );
        }

        // The value-carrying forms must keep working, and `;` must still not
        // be swallowed as an expression.
        for src in [
            "def f():\n    if x: return 1;",
            "if x: raise E;",
            "if x: raise E from C;",
        ] {
            assert!(crate::parse(src).is_ok(), "expected `{}` to parse", src);
        }
    }

    #[test]
    fn test_bare_tuple_in_return_and_yield() {
        // Issue #200: `return a, b` / `yield a, b` are testlists (bare tuples).
        // parse_return_stmt / the yield atom used parse_expr (stops at the first
        // comma) instead of parse_expr_list.
        for src in [
            "def g():\n    return 1, 2\n",
            "def g():\n    return a, b, c\n",
            "def g():\n    yield 1, 2\n",
            "def g():\n    x = 1\n    return x, x + 1\n",
        ] {
            assert!(
                crate::parse(src).is_ok(),
                "expected `{}` to parse",
                src.escape_debug()
            );
        }
        // The single-value and bare forms must still parse (no over-consumption).
        for src in [
            "def g():\n    return 5\n",
            "def g():\n    return\n",
            "def g():\n    yield\n",
        ] {
            assert!(
                crate::parse(src).is_ok(),
                "expected `{}` to parse",
                src.escape_debug()
            );
        }
    }

    #[test]
    fn test_semicolon_terminates_bare_tuple_and_side_effect_import() {
        // Regression, found by scripts/grammar-fuzz.py. Same root cause as the
        // bare-return case above: SEMICOLON is a statement terminator inside a
        // one-line suite, but several terminator sets only listed NEWLINE/EOF.
        for src in [
            // trailing comma on a bare tuple, terminated by `;`
            "if x: a,;",
            "if x: a,; b = 1",
            "if x: a, b,;",
            // side-effect import terminated by `;`
            "if dev: import \"./debug.css\";",
            "if dev: import \"./debug.css\"; run()",
        ] {
            assert!(
                crate::parse(src).is_ok(),
                "expected `{}` to parse",
                src.escape_debug()
            );
        }

        // The guard must NOT have been widened: a side-effect import still
        // takes no alias, no trailing comma, and no further names.
        for src in [
            "import \"./a.css\" as x",
            "import \"./a.css\", \"./b.css\"",
            "import \"./a.css\", os",
        ] {
            assert!(
                !parse_errors(src).is_empty(),
                "expected `{}` to still be rejected",
                src.escape_debug()
            );
        }
    }

    #[test]
    fn test_simple_statements_require_a_terminator() {
        // Regression, found by scripts/grammar-fuzz.py (direction B).
        //
        // Every simple-statement parser ended with `self.eat(&Token::Newline)`,
        // which is a NO-OP when the next token is not a newline. The statement
        // loop then started a fresh statement wherever the previous one
        // stopped, so the parser accepted token soup with NO separator at all —
        // each of these silently parsed as two statements. grammar/pyths.lark
        // (`simple_stmt: small_stmt _NEWLINE`) always rejected them, so the
        // authoritative parser was the over-accepting one.
        for src in [
            "x = 1 y = 2",
            "pass pass",
            "a b",
            "import os x = 1",
            "return 1 return 2",
            // A NAME adjacent to a STRING is not implicit concatenation:
            // only string-to-string is. This used to parse as `abc` followed
            // by a second statement `"z"`.
            "greet = abc\"z\"",
            "del a del b",
            "global g x = 1",
        ] {
            assert!(
                !parse_errors(src).is_empty(),
                "expected `{}` to be REJECTED (no statement terminator)",
                src.escape_debug()
            );
        }

        // ...while every legitimate terminator still works: NEWLINE, `;` in a
        // one-line suite, DEDENT at the end of a block, and EOF.
        for src in [
            "x = 1\ny = 2",
            "if x: y = 1; z = 2",
            "def f():\n    x = 1\ny = 2",
            "x = 1",
            "g = f\"Hi {name}\"",
            "g = \"a\" \"b\"", // real implicit string concatenation
            "from a import b\nx = 1",
            "@app.route(\"/x\")\ndef f():\n    pass",
        ] {
            assert!(
                crate::parse(src).is_ok(),
                "expected `{}` to parse",
                src.escape_debug()
            );
        }
    }

    #[test]
    fn test_multiline_string_statement_is_terminated() {
        // REGRESSION (#193). The #190/#191 tightening above is correct and
        // stays, but it exposed a latent LEXER bug: the closing line of a
        // triple-quoted string never emitted the statement's NEWLINE (the
        // `ends_inside_string` test was off by one, so a closing line was
        // misclassified as an interior line). With the terminator check now
        // strict, EVERY multi-line string followed by another statement was
        // rejected — which broke real code (reference-app's WasmDemo.psc).
        //
        // These all parse in CPython, so they must parse here.
        for src in [
            // The minimal repro.
            "W = \"\"\"\nfoo\n\"\"\"\nX = 1",
            // Module docstring followed by code.
            "\"\"\"\ndoc\n\"\"\"\nx = 1",
            // Triple-single-quoted sibling.
            "a = '''\nx\n'''\nb = 2",
            // f-string sibling (#109).
            "g = f\"\"\"\nval {n}\n\"\"\"\nh = 3",
            // Closing line carrying trailing tokens.
            "print(\"\"\"a\nb\"\"\")\nx = 1",
            // Inside a function body, and inside a nested block.
            "def f():\n    \"\"\"\n    doc\n    \"\"\"\n    return 1\ny = 2",
            "if True:\n    s = \"\"\"\n  a\n\"\"\"\n    t = 1",
            // Inside brackets (no NEWLINE should be emitted there at all).
            "L = [\n    \"\"\"\na\n\"\"\",\n    \"b\",\n]\nm = 1",
            // String content that LOOKS like code must not disturb the
            // statement stream that follows it.
            "W = \"\"\"\ndef g():\n    return 1\n\"\"\"\nX = 1",
            // Two adjacent multi-line strings, then a statement.
            "p = \"\"\"\n1\n\"\"\"\nq = \"\"\"\n2\n\"\"\"\nr = 3",
            // Last statement in the file — with and without a trailing newline.
            "x = 1\nW = \"\"\"\nfoo\n\"\"\"",
            "x = 1\nW = \"\"\"\nfoo\n\"\"\"\n",
        ] {
            assert!(
                crate::parse(src).is_ok(),
                "expected `{}` to parse: {:?}",
                src.escape_debug(),
                crate::parse(src).err()
            );
        }

        // And the tightening is NOT weakened by the fix: a multi-line string
        // followed by a statement ON THE SAME LINE is still token soup.
        assert!(
            !parse_errors("W = \"\"\"\nfoo\n\"\"\" X = 1").is_empty(),
            "no terminator after the closing quotes — must still be REJECTED"
        );
    }

    // ── B12: invalid-Python that previously parsed but miscompiled ──────────

    fn err_contains(source: &str, needle: &str) {
        let errs = crate::parse(source).unwrap_err();
        assert!(
            errs.iter().any(|e| e.message.contains(needle)),
            "source {source:?} should be rejected with a message containing {needle:?}; got {:?}",
            errs.iter().map(|e| &e.message).collect::<Vec<_>>()
        );
    }

    #[test]
    fn test_b12_duplicate_params_rejected() {
        err_contains("def f(a, a):\n    return a", "duplicate parameter");
        err_contains("def f(a, *a):\n    return a", "duplicate parameter");
        err_contains("def f(a, **a):\n    return a", "duplicate parameter");
    }

    #[test]
    fn test_b12_non_default_after_default_rejected() {
        err_contains("def f(a=1, b):\n    return a", "non-default parameter");
        err_contains("def f(a, b=1, c):\n    return a", "non-default parameter");
    }

    #[test]
    fn test_b12_param_after_kwargs_rejected() {
        err_contains("def f(**k, a):\n    return a", "parameter after **kwargs");
        err_contains("def f(**k, *a):\n    return a", "parameter after **kwargs");
    }

    #[test]
    fn test_b12_module_level_return_break_continue_rejected() {
        err_contains("return 5", "'return' outside function");
        err_contains("break", "'break' outside loop");
        err_contains("continue", "'continue' outside loop");
        // In a class body (not a function / loop) too.
        err_contains(
            "class C:\n    x = 1\n    return 1",
            "'return' outside function",
        );
        err_contains("def f():\n    x = 1\n    break", "'break' outside loop");
    }

    #[test]
    fn test_b12_valid_param_forms_still_accepted() {
        // Keyword-only required param after a default is legal (after `*`).
        assert!(crate::parse("def f(a=1, *, b):\n    return b").is_ok());
        // Required keyword-only after *args.
        assert!(crate::parse("def f(a, *args, b):\n    return b").is_ok());
        // All-defaults, **kwargs-last, positional-only slash.
        assert!(crate::parse("def f(a=1, b=2):\n    return a").is_ok());
        assert!(crate::parse("def f(a, **k):\n    return a").is_ok());
        assert!(crate::parse("def f(a, b, /, c):\n    return c").is_ok());
    }

    #[test]
    fn test_b12_valid_return_break_continue_still_accepted() {
        assert!(crate::parse("def f():\n    return 1").is_ok());
        assert!(crate::parse("for i in range(3):\n    break").is_ok());
        assert!(crate::parse("for i in range(3):\n    continue").is_ok());
        assert!(crate::parse("def f():\n    while True:\n        break").is_ok());
        // A nested function does NOT inherit the enclosing loop for break, but a
        // return inside it is fine.
        assert!(crate::parse("for i in range(2):\n    def g():\n        return i").is_ok());
    }
}
