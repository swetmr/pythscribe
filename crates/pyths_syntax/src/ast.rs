use crate::operators::{AugAssignOp, BinOp, UnaryOp};
use crate::span::Span;

/// A complete module (one .ps file).
#[derive(Debug, Clone, PartialEq)]
pub struct Module {
    pub body: Vec<Stmt>,
    pub span: Span,
}

/// Statements.
#[derive(Debug, Clone, PartialEq)]
pub struct Stmt {
    pub kind: StmtKind,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq)]
pub enum StmtKind {
    /// Expression statement (e.g., function call)
    Expr(Expr),

    /// Variable assignment: `x = expr` or `x: type = expr`
    Assign { targets: Vec<Expr>, value: Expr },

    /// Augmented assignment: `x += expr`
    AugAssign {
        target: Expr,
        op: AugAssignOp,
        value: Expr,
    },

    /// Function definition
    FuncDef {
        name: String,
        params: Vec<Param>,
        body: Vec<Stmt>,
        decorator_list: Vec<Expr>,
        return_type: Option<Expr>,
        is_async: bool,
    },

    /// Class definition
    ClassDef {
        name: String,
        bases: Vec<Expr>,
        body: Vec<Stmt>,
        decorator_list: Vec<Expr>,
    },

    /// Return statement
    Return(Option<Expr>),

    /// If / elif / else
    If {
        test: Expr,
        body: Vec<Stmt>,
        elif_clauses: Vec<(Expr, Vec<Stmt>)>,
        else_body: Option<Vec<Stmt>>,
    },

    /// While loop
    While {
        test: Expr,
        body: Vec<Stmt>,
        else_body: Option<Vec<Stmt>>,
    },

    /// For loop
    For {
        target: Expr,
        iter: Expr,
        body: Vec<Stmt>,
        else_body: Option<Vec<Stmt>>,
        is_async: bool,
    },

    /// Break
    Break,

    /// Continue
    Continue,

    /// Pass
    Pass,

    /// Import: `import module`
    Import { names: Vec<ImportAlias> },

    /// Side-effect import of a bare string literal: `import "./styles.css"`.
    ///
    /// PythScribe language extension (not valid Python syntax — Python has
    /// no bare-string-literal import form). Used for asset imports (CSS,
    /// SCSS, images, etc.) that exist purely for their bundler side effect
    /// and bind no name. Emitted verbatim to JS as `import "<path>";`; the
    /// string content is opaque to the compiler (no extension validation —
    /// that's the bundler's job).
    ImportSideEffect(String),

    /// From import: `from module import name`.
    ///
    /// `level` encodes the number of leading dots for Python relative imports:
    /// - `level = 0`: absolute import — `from foo.bar import x`
    /// - `level = 1`: sibling import — `from .foo import x` or `from . import x`
    /// - `level = 2+`: parent imports — `from ..foo import x`, `from ... import x`
    ///
    /// When `level > 0` and `module` is empty, the import targets the
    /// `level`th ancestor package itself (`from . import x`).
    ImportFrom {
        module: String,
        names: Vec<ImportAlias>,
        level: u32,
    },

    /// Try / except / else / finally
    Try {
        body: Vec<Stmt>,
        handlers: Vec<ExceptHandler>,
        else_body: Option<Vec<Stmt>>,
        finally_body: Option<Vec<Stmt>>,
    },

    /// Raise statement: `raise`, `raise X`, or `raise X from Y`
    /// (second field is the optional `from Y` cause).
    Raise(Option<Expr>, Option<Expr>),

    /// Assert statement
    Assert { test: Expr, msg: Option<Expr> },

    /// Global declaration
    Global(Vec<String>),

    /// Nonlocal declaration
    Nonlocal(Vec<String>),

    /// Delete statement
    Del(Vec<Expr>),

    /// With statement
    With {
        items: Vec<WithItem>,
        body: Vec<Stmt>,
        is_async: bool,
    },

    /// Annotated assignment: `x: int = 5` or `x: int`
    AnnAssign {
        target: Expr,
        annotation: Expr,
        value: Option<Expr>,
    },

    /// Match/case statement
    Match {
        subject: Expr,
        cases: Vec<MatchCase>,
    },
}

/// Expressions.
#[derive(Debug, Clone, PartialEq)]
pub struct Expr {
    pub kind: ExprKind,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq)]
pub enum ExprKind {
    /// Integer literal
    IntLiteral(i128),

    /// Float literal
    FloatLiteral(f64),

    /// Imaginary literal (`2j`, `3.5j`): the f64 is the magnitude of the
    /// imaginary part. `3 + 4j` is just a binop over an int and this node —
    /// no special grammar beyond the literal (#283). `cmath` is out of scope.
    ImagLiteral(f64),

    /// String literal
    StringLiteral(String),

    /// Bytes literal: `b'...'` — decoded code points narrowed to bytes.
    BytesLiteral(Vec<u8>),

    /// F-string: `f"hello {name}"`
    FString { parts: Vec<FStringPart> },

    /// Boolean literal
    BoolLiteral(bool),

    /// None literal
    NoneLiteral,

    /// Identifier / name
    Name(String),

    /// Binary operation: `a + b`
    BinOp {
        left: Box<Expr>,
        op: BinOp,
        right: Box<Expr>,
    },

    /// Unary operation: `-x`, `not x`
    UnaryOp { op: UnaryOp, operand: Box<Expr> },

    /// Comparison chain: `a < b < c`
    Compare {
        left: Box<Expr>,
        comparisons: Vec<(BinOp, Expr)>,
    },

    /// Function call: `f(args)` or optional `f?(args)`
    Call {
        func: Box<Expr>,
        args: Vec<Expr>,
        kwargs: Vec<Keyword>,
        optional: bool,
    },

    /// Attribute access: `x.attr` or optional `x?.attr`
    Attribute {
        value: Box<Expr>,
        attr: String,
        optional: bool,
    },

    /// Index/subscript: `x[index]` or optional `x?[index]`
    Subscript {
        value: Box<Expr>,
        index: Box<Expr>,
        optional: bool,
    },

    /// Slice: `start:stop:step`
    Slice {
        lower: Option<Box<Expr>>,
        upper: Option<Box<Expr>>,
        step: Option<Box<Expr>>,
    },

    /// List literal: `[1, 2, 3]`
    List(Vec<Expr>),

    /// Tuple literal: `(1, 2, 3)`
    Tuple(Vec<Expr>),

    /// Dict literal: `{k: v, **spread}`
    Dict { items: Vec<DictItem> },

    /// Set literal: `{1, 2, 3}`
    Set(Vec<Expr>),

    /// List comprehension: `[x for x in xs if x > 0]`
    ListComp {
        elt: Box<Expr>,
        generators: Vec<Comprehension>,
    },

    /// Dict comprehension
    DictComp {
        key: Box<Expr>,
        value: Box<Expr>,
        generators: Vec<Comprehension>,
    },

    /// Set comprehension
    SetComp {
        elt: Box<Expr>,
        generators: Vec<Comprehension>,
    },

    /// Generator expression
    GeneratorExp {
        elt: Box<Expr>,
        generators: Vec<Comprehension>,
    },

    /// Lambda: `lambda x: x + 1`
    Lambda { params: Vec<Param>, body: Box<Expr> },

    /// Ternary: `x if cond else y`
    IfExpr {
        test: Box<Expr>,
        body: Box<Expr>,
        else_body: Box<Expr>,
    },

    /// Starred expression: `*args`
    Starred(Box<Expr>),

    /// Await expression
    Await(Box<Expr>),

    /// Yield expression
    Yield(Option<Box<Expr>>),

    /// Yield from expression
    YieldFrom(Box<Expr>),

    /// Walrus operator: `x := expr`
    NamedExpr { target: Box<Expr>, value: Box<Expr> },
}

/// An item in a dict literal: either a key-value pair or a spread (`**expr`).
#[derive(Debug, Clone, PartialEq)]
pub enum DictItem {
    /// `key: value`
    KeyValue { key: Expr, value: Expr },
    /// `**expr`
    Spread(Expr),
}

/// A part of an f-string.
#[derive(Debug, Clone, PartialEq)]
pub enum FStringPart {
    Literal(String),
    Expr(Expr),
}

/// A function parameter.
#[derive(Debug, Clone, PartialEq)]
pub struct Param {
    pub name: String,
    pub annotation: Option<Box<Expr>>,
    pub default: Option<Expr>,
    pub is_args: bool,
    pub is_kwargs: bool,
    pub span: Span,
}

/// A keyword argument in a call: `key=value`.
#[derive(Debug, Clone, PartialEq)]
pub struct Keyword {
    pub name: Option<String>, // None for **kwargs
    pub value: Expr,
    pub span: Span,
}

/// Comprehension generator: `for x in xs if cond`.
#[derive(Debug, Clone, PartialEq)]
pub struct Comprehension {
    pub target: Expr,
    pub iter: Expr,
    pub ifs: Vec<Expr>,
    pub is_async: bool,
}

/// Exception handler in try/except.
#[derive(Debug, Clone, PartialEq)]
pub struct ExceptHandler {
    pub exc_type: Option<Expr>,
    pub name: Option<String>,
    pub body: Vec<Stmt>,
    pub span: Span,
}

/// With item: `expr as name`.
#[derive(Debug, Clone, PartialEq)]
pub struct WithItem {
    pub context_expr: Expr,
    pub optional_var: Option<Expr>,
}

/// Import alias: `name as alias`.
#[derive(Debug, Clone, PartialEq)]
pub struct ImportAlias {
    pub name: String,
    pub alias: Option<String>,
}

/// A single case branch in a match statement.
#[derive(Debug, Clone, PartialEq)]
pub struct MatchCase {
    pub pattern: Pattern,
    pub guard: Option<Expr>,
    pub body: Vec<Stmt>,
}

/// Pattern for match/case.
#[derive(Debug, Clone, PartialEq)]
pub enum Pattern {
    /// Wildcard: `case _:`
    Wildcard,
    /// Capture variable: `case x:`
    Capture(String),
    /// Literal: `case 42:`, `case "hello":`, `case True:`
    Literal(Expr),
    /// Class pattern: `case Point(x, y):`
    Class { cls: String, args: Vec<Pattern> },
    /// Sequence pattern: `case [a, b, c]:`
    Sequence(Vec<Pattern>),
    /// Mapping pattern: `case {"key": value}:`
    Mapping(Vec<(Expr, Pattern)>),
    /// OR pattern: `case 1 | 2 | 3:`
    Or(Vec<Pattern>),
    /// AS pattern: `case pattern as name:`
    As { pattern: Box<Pattern>, name: String },
    /// Star pattern in sequence: `case [first, *rest]:`
    Star(Option<String>),
    /// Value pattern (dotted name): `case Color.RED:`
    Value(Expr),
}

// Convenience constructors
impl Expr {
    pub fn new(kind: ExprKind, span: Span) -> Self {
        Self { kind, span }
    }

    pub fn name(s: impl Into<String>, span: Span) -> Self {
        Self::new(ExprKind::Name(s.into()), span)
    }

    pub fn int(v: i128, span: Span) -> Self {
        Self::new(ExprKind::IntLiteral(v), span)
    }

    pub fn string(s: impl Into<String>, span: Span) -> Self {
        Self::new(ExprKind::StringLiteral(s.into()), span)
    }

    pub fn bool_lit(v: bool, span: Span) -> Self {
        Self::new(ExprKind::BoolLiteral(v), span)
    }

    pub fn none(span: Span) -> Self {
        Self::new(ExprKind::NoneLiteral, span)
    }
}

impl Stmt {
    pub fn new(kind: StmtKind, span: Span) -> Self {
        Self { kind, span }
    }
}
