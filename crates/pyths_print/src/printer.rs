use pyths_syntax::ast::*;
use pyths_syntax::operators::{AugAssignOp, BinOp, UnaryOp};

pub struct PsPrinter {
    out: String,
    indent: usize,
    /// B17: set when the printer knowingly emitted source it cannot represent
    /// faithfully — currently only an f-string whose canonical inner content
    /// contains BOTH a `"` and a `'''`, which neither delimiter can quote.
    /// `expr` walks by `&self`, so this needs interior mutability. Callers that
    /// require valid output (`canonicalize`) must consult `is_malformed()` and
    /// surface an error instead of returning the garbage.
    malformed: std::cell::Cell<bool>,
}

/// Precedence levels (higher = tighter binding).
/// Mirrors Python's operator precedence table.
fn binop_prec(op: &BinOp) -> u8 {
    match op {
        BinOp::Or => 1,
        BinOp::And => 2,
        // comparison ops are not associative; handled specially in Compare
        BinOp::Eq
        | BinOp::NotEq
        | BinOp::Lt
        | BinOp::LtEq
        | BinOp::Gt
        | BinOp::GtEq
        | BinOp::In
        | BinOp::NotIn
        | BinOp::Is
        | BinOp::IsNot => 4,
        BinOp::BitOr => 5,
        BinOp::BitXor => 6,
        BinOp::BitAnd => 7,
        BinOp::ShiftLeft | BinOp::ShiftRight => 8,
        BinOp::Add | BinOp::Sub => 9,
        BinOp::Mul | BinOp::Div | BinOp::FloorDiv | BinOp::Mod => 10,
        BinOp::Pow => 12,
        // PythScribe extensions: low precedence (below `or`)
        BinOp::NullishCoalesce => 3,
        BinOp::Pipeline => 0,
    }
}

/// The "effective precedence" of an expression, used to decide whether
/// a child needs parentheses when placed inside a parent operator.
///
/// Returns `None` for atoms (literals, names, calls, subscripts, attributes,
/// comprehensions, collections, lambda, etc.) — these never need wrapping.
/// Returns `Some(prec)` for compound expressions where the grammar requires
/// parens to override precedence.
fn expr_prec(e: &Expr) -> Option<u8> {
    match &e.kind {
        ExprKind::BinOp { op, .. } => Some(binop_prec(op)),
        ExprKind::UnaryOp { .. } => Some(11), // between Mul and Pow
        ExprKind::Compare { .. } => Some(4),
        ExprKind::IfExpr { .. } => Some(0), // lowest — always parenthesize if nested
        ExprKind::Lambda { .. } => Some(0),
        ExprKind::NamedExpr { .. } => Some(0),
        ExprKind::Yield { .. } | ExprKind::YieldFrom { .. } => Some(0),
        ExprKind::Await { .. } => Some(13), // tightest among compound
        // atoms
        _ => None,
    }
}

/// Escape a DECODED string value for a double-quoted `.ps` literal.
///
/// #95 changed the lexer to decode escape sequences at the boundary
/// (source `"a\nb"` now stores a real newline byte), so the printer's
/// job inverted: it must RE-ESCAPE on output. `\\ \" \n \t \r` print by
/// name; other control chars print as `\xNN`; everything else verbatim.
/// The result re-lexes (and re-decodes) to exactly the input value —
/// the Iron Rule round-trip.
fn escape_py_string(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for ch in s.chars() {
        match ch {
            '\\' => out.push_str("\\\\"),
            '"' => out.push_str("\\\""),
            '\n' => out.push_str("\\n"),
            '\t' => out.push_str("\\t"),
            '\r' => out.push_str("\\r"),
            c if (c as u32) < 0x20 || c as u32 == 0x7f => {
                out.push_str(&format!("\\x{:02x}", c as u32));
            }
            c => out.push(c),
        }
    }
    out
}

/// Canonicalize a string literal for output. Since #95 the AST stores
/// the DECODED value, so the canonical form is always a double-quoted
/// literal with full re-escaping (delimiter conflicts are impossible —
/// any `"` in the value prints as `\"`).
fn quote_string_literal(s: &str) -> String {
    format!("\"{}\"", escape_py_string(s))
}

impl Default for PsPrinter {
    fn default() -> Self {
        Self::new()
    }
}

impl PsPrinter {
    pub fn new() -> Self {
        PsPrinter {
            out: String::new(),
            indent: 0,
            malformed: std::cell::Cell::new(false),
        }
    }

    /// B17: `true` if any emitted construct could not be represented faithfully
    /// (see the `malformed` field). Callers that need valid `.ps` source must
    /// treat this as an error rather than trusting `finish()`.
    pub fn is_malformed(&self) -> bool {
        self.malformed.get()
    }

    pub fn emit_module(&mut self, m: &Module) {
        for stmt in &m.body {
            self.emit_stmt(stmt);
        }
    }

    pub fn finish(self) -> String {
        self.out
    }

    // ── private helpers ───────────────────────────────────────────────────────

    fn line(&mut self, text: &str) {
        let indent = "    ".repeat(self.indent);
        self.out.push_str(&indent);
        self.out.push_str(text);
        self.out.push('\n');
    }

    fn emit_block(&mut self, body: &[Stmt]) {
        self.indent += 1;
        for stmt in body {
            self.emit_stmt(stmt);
        }
        self.indent -= 1;
    }

    fn emit_stmt(&mut self, s: &Stmt) {
        match &s.kind {
            StmtKind::Pass => self.line("pass"),
            StmtKind::Break => self.line("break"),
            StmtKind::Continue => self.line("continue"),

            StmtKind::Expr(e) => {
                let text = self.expr(e);
                self.line(&text);
            }

            StmtKind::Assign { targets, value } => {
                let lhs: Vec<String> = targets.iter().map(|t| self.expr(t)).collect();
                let rhs = self.expr(value);
                self.line(&format!("{} = {}", lhs.join(" = "), rhs));
            }

            StmtKind::AugAssign { target, op, value } => {
                let lhs = self.expr(target);
                let rhs = self.expr(value);
                let op_str = Self::augassign_str(op);
                self.line(&format!("{} {}= {}", lhs, op_str, rhs));
            }

            StmtKind::AnnAssign {
                target,
                annotation,
                value,
            } => {
                let t = self.expr(target);
                let ann = self.expr(annotation);
                if let Some(v) = value {
                    let val = self.expr(v);
                    self.line(&format!("{}: {} = {}", t, ann, val));
                } else {
                    self.line(&format!("{}: {}", t, ann));
                }
            }

            StmtKind::Return(val) => {
                if let Some(e) = val {
                    let text = self.expr(e);
                    self.line(&format!("return {}", text));
                } else {
                    self.line("return");
                }
            }

            StmtKind::Raise(val, cause) => {
                if let Some(e) = val {
                    let text = self.expr(e);
                    if let Some(c) = cause {
                        let cause_text = self.expr(c);
                        self.line(&format!("raise {} from {}", text, cause_text));
                    } else {
                        self.line(&format!("raise {}", text));
                    }
                } else {
                    self.line("raise");
                }
            }

            StmtKind::FuncDef {
                name,
                params,
                body,
                decorator_list,
                return_type,
                is_async,
            } => {
                for dec in decorator_list {
                    let d = self.expr(dec);
                    self.line(&format!("@{}", d));
                }
                let params_str: Vec<String> = params.iter().map(|p| self.param(p)).collect();
                let prefix = if *is_async { "async def" } else { "def" };
                let ret = match return_type {
                    Some(rt) => format!(" -> {}", self.expr(rt)),
                    None => String::new(),
                };
                self.line(&format!(
                    "{} {}({}){}:",
                    prefix,
                    name,
                    params_str.join(", "),
                    ret
                ));
                self.emit_block(body);
            }

            StmtKind::ClassDef {
                name,
                bases,
                body,
                decorator_list,
            } => {
                for dec in decorator_list {
                    let d = self.expr(dec);
                    self.line(&format!("@{}", d));
                }
                let bases_str: Vec<String> = bases.iter().map(|b| self.expr(b)).collect();
                if bases_str.is_empty() {
                    self.line(&format!("class {}:", name));
                } else {
                    self.line(&format!("class {}({}):", name, bases_str.join(", ")));
                }
                self.emit_block(body);
            }

            StmtKind::If {
                test,
                body,
                elif_clauses,
                else_body,
            } => {
                let t = self.expr(test);
                self.line(&format!("if {}:", t));
                self.emit_block(body);
                for (elif_test, elif_body) in elif_clauses {
                    let et = self.expr(elif_test);
                    self.line(&format!("elif {}:", et));
                    self.emit_block(elif_body);
                }
                if let Some(eb) = else_body {
                    self.line("else:");
                    self.emit_block(eb);
                }
            }

            StmtKind::While {
                test,
                body,
                else_body,
            } => {
                let t = self.expr(test);
                self.line(&format!("while {}:", t));
                self.emit_block(body);
                if let Some(eb) = else_body {
                    self.line("else:");
                    self.emit_block(eb);
                }
            }

            StmtKind::For {
                target,
                iter,
                body,
                else_body,
                is_async,
            } => {
                let tgt = self.expr(target);
                let it = self.expr(iter);
                let prefix = if *is_async { "async for" } else { "for" };
                self.line(&format!("{} {} in {}:", prefix, tgt, it));
                self.emit_block(body);
                if let Some(eb) = else_body {
                    self.line("else:");
                    self.emit_block(eb);
                }
            }

            StmtKind::Import { names } => {
                let parts: Vec<String> = names
                    .iter()
                    .map(|a| {
                        if let Some(alias) = &a.alias {
                            format!("{} as {}", a.name, alias)
                        } else {
                            a.name.clone()
                        }
                    })
                    .collect();
                self.line(&format!("import {}", parts.join(", ")));
            }

            StmtKind::ImportSideEffect(path) => {
                self.line(&format!("import {}", quote_string_literal(path)));
            }

            StmtKind::ImportFrom {
                module,
                names,
                level,
            } => {
                let dots = ".".repeat(*level as usize);
                let parts: Vec<String> = names
                    .iter()
                    .map(|a| {
                        if let Some(alias) = &a.alias {
                            format!("{} as {}", a.name, alias)
                        } else {
                            a.name.clone()
                        }
                    })
                    .collect();
                self.line(&format!(
                    "from {}{} import {}",
                    dots,
                    module,
                    parts.join(", ")
                ));
            }

            StmtKind::Try {
                body,
                handlers,
                else_body,
                finally_body,
            } => {
                self.line("try:");
                self.emit_block(body);
                for h in handlers {
                    match (&h.exc_type, &h.name) {
                        (Some(exc), Some(n)) => {
                            let e = self.expr(exc);
                            self.line(&format!("except {} as {}:", e, n));
                        }
                        (Some(exc), None) => {
                            let e = self.expr(exc);
                            self.line(&format!("except {}:", e));
                        }
                        (None, _) => {
                            self.line("except:");
                        }
                    }
                    self.emit_block(&h.body);
                }
                if let Some(eb) = else_body {
                    self.line("else:");
                    self.emit_block(eb);
                }
                if let Some(fb) = finally_body {
                    self.line("finally:");
                    self.emit_block(fb);
                }
            }

            StmtKind::Assert { test, msg } => {
                let t = self.expr(test);
                if let Some(m) = msg {
                    let ms = self.expr(m);
                    self.line(&format!("assert {}, {}", t, ms));
                } else {
                    self.line(&format!("assert {}", t));
                }
            }

            StmtKind::Global(names) => {
                self.line(&format!("global {}", names.join(", ")));
            }

            StmtKind::Nonlocal(names) => {
                self.line(&format!("nonlocal {}", names.join(", ")));
            }

            StmtKind::Del(targets) => {
                let parts: Vec<String> = targets.iter().map(|t| self.expr(t)).collect();
                self.line(&format!("del {}", parts.join(", ")));
            }

            StmtKind::With {
                items,
                body,
                is_async,
            } => {
                let parts: Vec<String> = items
                    .iter()
                    .map(|wi| {
                        let ctx = self.expr(&wi.context_expr);
                        if let Some(var) = &wi.optional_var {
                            format!("{} as {}", ctx, self.expr(var))
                        } else {
                            ctx
                        }
                    })
                    .collect();
                let prefix = if *is_async { "async with" } else { "with" };
                self.line(&format!("{} {}:", prefix, parts.join(", ")));
                self.emit_block(body);
            }

            StmtKind::Match { subject, cases } => {
                let s = self.expr(subject);
                self.line(&format!("match {}:", s));
                // case lines are at indent+1; case bodies at indent+2
                self.indent += 1;
                for case in cases {
                    let pat = self.pattern(&case.pattern);
                    let guard = match &case.guard {
                        Some(g) => format!(" if {}", self.expr(g)),
                        None => String::new(),
                    };
                    self.line(&format!("case {}{}:", pat, guard));
                    self.emit_block(&case.body);
                }
                self.indent -= 1;
            }
        }
    }

    /// Emit an expression that will be used as the base of a postfix operation
    /// (attribute access, subscript, call). Only atoms and other postfix
    /// expressions (Name, Call, Attribute, Subscript, literals, collections)
    /// can stand alone; anything else (BinOp, UnaryOp, Compare, IfExpr,
    /// Lambda, Yield, Await) must be wrapped in parens so that the postfix
    /// operator binds to the whole expression.
    fn postfix_expr(&self, e: &Expr) -> String {
        let needs_parens = match &e.kind {
            // Atoms — no parens needed
            ExprKind::Name(_)
            | ExprKind::IntLiteral(_)
            | ExprKind::FloatLiteral(_)
            | ExprKind::ImagLiteral(_)
            | ExprKind::BoolLiteral(_)
            | ExprKind::NoneLiteral
            | ExprKind::StringLiteral(_)
            | ExprKind::FString { .. }
            | ExprKind::List(_)
            | ExprKind::Tuple(_)
            | ExprKind::Dict { .. }
            | ExprKind::Set(_)
            | ExprKind::ListComp { .. }
            | ExprKind::DictComp { .. }
            | ExprKind::SetComp { .. }
            | ExprKind::GeneratorExp { .. } => false,
            // Postfix chains — no parens needed
            ExprKind::Call { .. } | ExprKind::Attribute { .. } | ExprKind::Subscript { .. } => {
                false
            }
            // Anything compound — needs parens
            _ => true,
        };
        let s = self.expr(e);
        if needs_parens {
            format!("({})", s)
        } else {
            s
        }
    }

    /// Emit an expression, parenthesizing if `parent_prec` demands it.
    ///
    /// `parent_prec`: the precedence of the *parent* context. Pass `None`
    /// at top-level (statement context) — nothing is ever parenthesized at
    /// statement level.
    ///
    /// `is_right_of_left_assoc`: true when this child is the **right**
    /// operand of a left-associative binary operator *with the same
    /// precedence* as the child. E.g. in `a - (b - c)`, the `(b - c)` is
    /// the right child of `-` (prec=9), and `b - c` also has prec=9, so we
    /// must parenthesize to preserve right-associativity.
    fn expr_with_prec(
        &self,
        e: &Expr,
        parent_prec: Option<u8>,
        is_right_of_left_assoc: bool,
    ) -> String {
        let s = self.expr(e);
        let child_prec = expr_prec(e);
        let needs_parens = match (child_prec, parent_prec) {
            (Some(cp), Some(pp)) => {
                if is_right_of_left_assoc {
                    cp <= pp
                } else {
                    cp < pp
                }
            }
            // child is atom or parent is top-level: no parens
            _ => false,
        };
        if needs_parens {
            format!("({})", s)
        } else {
            s
        }
    }

    fn expr(&self, e: &Expr) -> String {
        match &e.kind {
            ExprKind::IntLiteral(n) => n.to_string(),
            ExprKind::FloatLiteral(f) => {
                // Ensure there's a decimal point for canonical form
                let s = format!("{}", f);
                if s.contains('.') || s.contains('e') || s.contains('E') {
                    s
                } else {
                    format!("{}.0", s)
                }
            }
            ExprKind::ImagLiteral(f) => format!("{}j", f),
            ExprKind::BoolLiteral(b) => {
                if *b {
                    "True".to_string()
                } else {
                    "False".to_string()
                }
            }
            ExprKind::NoneLiteral => "None".to_string(),
            ExprKind::Name(n) => n.clone(),
            ExprKind::StringLiteral(s) => quote_string_literal(s),

            ExprKind::FString { parts } => {
                // Canonical form: prefer f"..." (double-quoted), but fall back to
                // f'''...''' when the emitted inner content contains a bare `"`.
                //
                // Why inner content can contain `"`:
                //   - Literal parts may have verbatim `\"` stored by the lexer.
                //   - Expression parts may contain string literals after format-spec
                //     lowering (e.g. `{n:04d}` → `{String(n).padStart(4, "0")}`).
                //     The `"0"` in the emitted expression would terminate f"..." early.
                //
                // Build the full inner content first, then decide on delimiter.
                // Literal parts: `{` → `{{`, `}` → `}}`, everything else verbatim.
                // Expression parts: wrapped in `{...}`, emitted by self.expr().
                let inner: String = parts
                    .iter()
                    .map(|p| match p {
                        FStringPart::Literal(s) => {
                            // #95: literal parts hold DECODED values — re-escape
                            // (backslash/quote/control chars) then brace-escape.
                            let mut escaped = String::with_capacity(s.len());
                            for ch in escape_py_string(s).chars() {
                                match ch {
                                    '{' => escaped.push_str("{{"),
                                    '}' => escaped.push_str("}}"),
                                    c => escaped.push(c),
                                }
                            }
                            escaped
                        }
                        FStringPart::Expr(e) => format!("{{{}}}", self.expr(e)),
                    })
                    .collect();

                // Does the emitted inner content contain a bare `"` that would
                // terminate f"..." prematurely? Scan the raw inner string.
                // Note: within `{...}` expression spans, `"` IS problematic for
                // f"..." — the lexer sees it as closing the outer f-string.
                // We conservatively flag any `"` in inner (whether inside `{}`
                // or not) as requiring the triple-single-quote form.
                let has_bare_dquote = inner.contains('"');

                if !has_bare_dquote {
                    // No `"` anywhere in inner — f"..." is safe.
                    format!("f\"{inner}\"")
                } else if !inner.contains("'''") {
                    // Has `"` but no `'''` — use f'''...''' which allows `"` freely.
                    format!("f'''{inner}'''")
                } else {
                    // B17: inner contains both `"` and `'''` — pathological.
                    // This cannot be emitted in either f"..." or f'''...'''
                    // form without escaping the printer does not model. Flag
                    // the output as malformed so `canonicalize` returns an error
                    // rather than silently handing back invalid source. The
                    // emitted text below is a best-effort placeholder that is
                    // never trusted once `is_malformed()` is set.
                    self.malformed.set(true);
                    format!("f'''{inner}'''")
                }
            }

            ExprKind::BinOp { left, op, right } => {
                let prec = binop_prec(op);
                // Pow (**) is right-associative; all others are left-associative.
                // Left operand: never needs extra parens for same-prec (both assoc cases agree).
                let l = self.expr_with_prec(left, Some(prec), false);
                let r = if matches!(op, BinOp::Pow) {
                    // right of ** (right-assoc): parens only when strictly lower prec
                    self.expr_with_prec(right, Some(prec), false)
                } else {
                    // right of left-assoc op: parens when same OR lower prec
                    self.expr_with_prec(right, Some(prec), true)
                };
                let op_str = Self::binop_str(op);
                format!("{} {} {}", l, op_str, r)
            }

            ExprKind::UnaryOp { op, operand } => {
                // Unary ops bind tightly. The operand needs parens if it is a
                // BinOp/Compare/etc. with lower precedence than the unary op.
                let unary_prec: u8 = 11;
                let o = self.expr_with_prec(operand, Some(unary_prec), false);
                match op {
                    UnaryOp::Neg => format!("-{}", o),
                    UnaryOp::Pos => format!("+{}", o),
                    UnaryOp::Not => format!("not {}", o),
                    UnaryOp::BitNot => format!("~{}", o),
                }
            }

            ExprKind::Compare { left, comparisons } => {
                // Comparison chains: `a < b < c`. The left operand and each rhs
                // operand are rendered without extra parens (the whole Compare node
                // already has a defined precedence).
                let mut s = self.expr_with_prec(left, Some(4), false);
                for (op, rhs) in comparisons {
                    s.push(' ');
                    s.push_str(Self::binop_str(op));
                    s.push(' ');
                    s.push_str(&self.expr_with_prec(rhs, Some(4), false));
                }
                s
            }

            ExprKind::Call {
                func,
                args,
                kwargs,
                optional,
            } => {
                // When calling a complex expression (not a simple atom), parenthesize it.
                // e.g. `(a + b)(x)` vs `a + b(x)`.
                // Postfix (call/attr/subscript) has highest precedence; any non-atom func
                // must be wrapped. But atoms (Name, Attribute-chain, Subscript, Call) are fine.
                let f = self.postfix_expr(func);
                let mut parts: Vec<String> = args.iter().map(|a| self.expr(a)).collect();
                for kw in kwargs {
                    match &kw.name {
                        Some(name) => parts.push(format!("{}={}", name, self.expr(&kw.value))),
                        None => parts.push(format!("**{}", self.expr(&kw.value))),
                    }
                }
                let open = if *optional { "?(" } else { "(" };
                format!("{}{}{})", f, open, parts.join(", "))
            }

            ExprKind::Attribute {
                value,
                attr,
                optional,
            } => {
                // The base of an attribute must be parenthesized if it's not a postfix atom.
                let v = self.postfix_expr(value);
                if *optional {
                    format!("{}?.{}", v, attr)
                } else {
                    format!("{}.{}", v, attr)
                }
            }

            ExprKind::Subscript {
                value,
                index,
                optional,
            } => {
                // Same as Attribute: the base must be a postfix atom.
                let v = self.postfix_expr(value);
                let i = self.expr(index);
                if *optional {
                    format!("{}?[{}]", v, i)
                } else {
                    format!("{}[{}]", v, i)
                }
            }

            ExprKind::Slice { lower, upper, step } => {
                let lo = lower.as_ref().map(|e| self.expr(e)).unwrap_or_default();
                let hi = upper.as_ref().map(|e| self.expr(e)).unwrap_or_default();
                match step {
                    Some(st) => format!("{}:{}:{}", lo, hi, self.expr(st)),
                    None => format!("{}:{}", lo, hi),
                }
            }

            ExprKind::List(elts) => {
                let parts: Vec<String> = elts.iter().map(|e| self.expr(e)).collect();
                format!("[{}]", parts.join(", "))
            }

            ExprKind::Tuple(elts) => {
                let parts: Vec<String> = elts.iter().map(|e| self.expr(e)).collect();
                if parts.len() == 1 {
                    format!("({},)", parts[0])
                } else {
                    format!("({})", parts.join(", "))
                }
            }

            ExprKind::Dict { items } => {
                let parts: Vec<String> = items
                    .iter()
                    .map(|item| match item {
                        DictItem::KeyValue { key, value } => {
                            format!("{}: {}", self.expr(key), self.expr(value))
                        }
                        DictItem::Spread(e) => format!("**{}", self.expr(e)),
                    })
                    .collect();
                format!("{{{}}}", parts.join(", "))
            }

            ExprKind::Set(elts) => {
                let parts: Vec<String> = elts.iter().map(|e| self.expr(e)).collect();
                format!("{{{}}}", parts.join(", "))
            }

            ExprKind::ListComp { elt, generators } => {
                let e = self.expr(elt);
                let gens = self.comprehensions(generators);
                format!("[{} {}]", e, gens)
            }

            ExprKind::DictComp {
                key,
                value,
                generators,
            } => {
                let k = self.expr(key);
                let v = self.expr(value);
                let gens = self.comprehensions(generators);
                format!("{{{}: {} {}}}", k, v, gens)
            }

            ExprKind::SetComp { elt, generators } => {
                let e = self.expr(elt);
                let gens = self.comprehensions(generators);
                format!("{{{} {}}}", e, gens)
            }

            ExprKind::GeneratorExp { elt, generators } => {
                let e = self.expr(elt);
                let gens = self.comprehensions(generators);
                format!("({} {})", e, gens)
            }

            ExprKind::Lambda { params, body } => {
                let params_str: Vec<String> = params.iter().map(|p| self.param(p)).collect();
                let b = self.expr(body);
                format!("lambda {}: {}", params_str.join(", "), b)
            }

            ExprKind::IfExpr {
                test,
                body,
                else_body,
            } => {
                let t = self.expr(test);
                let b = self.expr(body);
                let eb = self.expr(else_body);
                format!("{} if {} else {}", b, t, eb)
            }

            ExprKind::Starred(inner) => {
                format!("*{}", self.expr(inner))
            }

            ExprKind::Await(inner) => {
                format!("await {}", self.expr(inner))
            }

            ExprKind::Yield(val) => match val {
                Some(v) => format!("yield {}", self.expr(v)),
                None => "yield".to_string(),
            },

            ExprKind::YieldFrom(val) => {
                format!("yield from {}", self.expr(val))
            }

            ExprKind::NamedExpr { target, value } => {
                format!("{} := {}", self.expr(target), self.expr(value))
            }
        }
    }

    fn param(&self, p: &Param) -> String {
        let mut s = String::new();
        if p.is_args {
            s.push('*');
        } else if p.is_kwargs {
            s.push_str("**");
        }
        s.push_str(&p.name);
        if let Some(ann) = &p.annotation {
            s.push_str(&format!(": {}", self.expr(ann)));
        }
        if let Some(def) = &p.default {
            s.push_str(&format!("={}", self.expr(def)));
        }
        s
    }

    fn comprehensions(&self, generators: &[Comprehension]) -> String {
        generators
            .iter()
            .map(|g| {
                let prefix = if g.is_async { "async for" } else { "for" };
                let tgt = self.expr(&g.target);
                let it = self.expr(&g.iter);
                let mut s = format!("{} {} in {}", prefix, tgt, it);
                for cond in &g.ifs {
                    s.push_str(&format!(" if {}", self.expr(cond)));
                }
                s
            })
            .collect::<Vec<_>>()
            .join(" ")
    }

    fn pattern(&self, p: &Pattern) -> String {
        match p {
            Pattern::Wildcard => "_".to_string(),
            Pattern::Capture(name) => name.clone(),
            Pattern::Literal(e) => self.expr(e),
            Pattern::Class { cls, args } => {
                let parts: Vec<String> = args.iter().map(|a| self.pattern(a)).collect();
                format!("{}({})", cls, parts.join(", "))
            }
            Pattern::Sequence(pats) => {
                let parts: Vec<String> = pats.iter().map(|a| self.pattern(a)).collect();
                format!("[{}]", parts.join(", "))
            }
            Pattern::Mapping(pairs) => {
                let parts: Vec<String> = pairs
                    .iter()
                    .map(|(k, v)| format!("{}: {}", self.expr(k), self.pattern(v)))
                    .collect();
                format!("{{{}}}", parts.join(", "))
            }
            Pattern::Or(pats) => {
                let parts: Vec<String> = pats.iter().map(|a| self.pattern(a)).collect();
                parts.join(" | ")
            }
            Pattern::As { pattern, name } => {
                format!("{} as {}", self.pattern(pattern), name)
            }
            Pattern::Star(name) => match name {
                Some(n) => format!("*{}", n),
                None => "*_".to_string(),
            },
            Pattern::Value(e) => self.expr(e),
        }
    }

    fn binop_str(op: &BinOp) -> &'static str {
        match op {
            BinOp::Add => "+",
            BinOp::Sub => "-",
            BinOp::Mul => "*",
            BinOp::Div => "/",
            BinOp::FloorDiv => "//",
            BinOp::Mod => "%",
            BinOp::Pow => "**",
            BinOp::Eq => "==",
            BinOp::NotEq => "!=",
            BinOp::Lt => "<",
            BinOp::LtEq => "<=",
            BinOp::Gt => ">",
            BinOp::GtEq => ">=",
            BinOp::And => "and",
            BinOp::Or => "or",
            BinOp::In => "in",
            BinOp::NotIn => "not in",
            BinOp::Is => "is",
            BinOp::IsNot => "is not",
            BinOp::BitAnd => "&",
            BinOp::BitOr => "|",
            BinOp::BitXor => "^",
            BinOp::ShiftLeft => "<<",
            BinOp::ShiftRight => ">>",
            BinOp::NullishCoalesce => "??",
            BinOp::Pipeline => "|>",
        }
    }

    fn augassign_str(op: &AugAssignOp) -> &'static str {
        match op {
            AugAssignOp::Add => "+",
            AugAssignOp::Sub => "-",
            AugAssignOp::Mul => "*",
            AugAssignOp::Div => "/",
            AugAssignOp::FloorDiv => "//",
            AugAssignOp::Mod => "%",
            AugAssignOp::Pow => "**",
            AugAssignOp::BitAnd => "&",
            AugAssignOp::BitOr => "|",
            AugAssignOp::BitXor => "^",
            AugAssignOp::ShiftLeft => "<<",
            AugAssignOp::ShiftRight => ">>",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pyths_syntax::span::Span;

    fn print_src(src: &str) -> String {
        let m = pyths_parser::parse(src).expect("parse");
        super::super::print_module(&m)
    }

    #[test]
    fn empty_module_prints_empty() {
        let m = Module {
            body: vec![],
            span: Span::dummy(),
        };
        let mut p = PsPrinter::new();
        p.emit_module(&m);
        assert_eq!(p.finish(), "");
    }

    #[test]
    fn prints_simple_assign() {
        assert_eq!(print_src("x=1\n"), "x = 1\n");
    }

    #[test]
    fn prints_pass_and_return() {
        assert_eq!(
            print_src("def f():\n  return 2\n"),
            "def f():\n    return 2\n"
        );
    }

    #[test]
    fn normalizes_indentation_to_four_spaces() {
        assert_eq!(print_src("if a:\n      b\n"), "if a:\n    b\n");
    }

    #[test]
    fn prints_call_with_kwargs() {
        assert_eq!(print_src("f(1, x=2)\n"), "f(1, x=2)\n");
    }

    #[test]
    fn prints_binop() {
        assert_eq!(print_src("y = a + b\n"), "y = a + b\n");
    }

    #[test]
    fn prints_list_and_attribute() {
        assert_eq!(print_src("z = [a.b, c]\n"), "z = [a.b, c]\n");
    }

    #[test]
    fn prints_none_true_false() {
        assert_eq!(print_src("x = None\n"), "x = None\n");
        assert_eq!(print_src("x = True\n"), "x = True\n");
        assert_eq!(print_src("x = False\n"), "x = False\n");
    }

    #[test]
    fn prints_string_literal_double_quoted() {
        assert_eq!(print_src("s = \"hello\"\n"), "s = \"hello\"\n");
    }

    #[test]
    fn prints_decorator_on_def() {
        assert_eq!(
            print_src("@decorator\ndef f():\n    pass\n"),
            "@decorator\ndef f():\n    pass\n"
        );
    }

    #[test]
    fn prints_ann_assign() {
        assert_eq!(print_src("x: int = 5\n"), "x: int = 5\n");
    }

    #[test]
    fn prints_aug_assign() {
        assert_eq!(print_src("x += 1\n"), "x += 1\n");
    }

    #[test]
    fn prints_return_none() {
        assert_eq!(
            print_src("def f():\n    return\n"),
            "def f():\n    return\n"
        );
    }

    #[test]
    fn prints_match_case_with_space() {
        let out = print_src("match x:\n  case 1:\n    pass\n");
        assert!(
            out.contains("case 1:"),
            "expected 'case 1:' with a space, got:\n{}",
            out
        );
        // wildcard pattern must also have the space
        let out2 = print_src("match x:\n  case _:\n    pass\n");
        assert!(
            out2.contains("case _:"),
            "expected 'case _:' with a space, got:\n{}",
            out2
        );
    }
}
