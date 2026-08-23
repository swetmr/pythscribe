/// Binary operators.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BinOp {
    // Arithmetic
    Add,
    Sub,
    Mul,
    Div,
    FloorDiv,
    Mod,
    Pow,
    /// `@` — matrix multiplication (PEP 465): dispatches
    /// `__matmul__`/`__rmatmul__` at runtime (no builtin operand support).
    MatMul,
    // Comparison
    Eq,
    NotEq,
    Lt,
    LtEq,
    Gt,
    GtEq,
    // Logical
    And,
    Or,
    // Membership
    In,
    NotIn,
    // Identity
    Is,
    IsNot,
    // Bitwise
    BitAnd,
    BitOr,
    BitXor,
    ShiftLeft,
    ShiftRight,
    // PythScribe extensions
    NullishCoalesce, // ??
    Pipeline,        // |>
}

/// Unary operators.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UnaryOp {
    Neg,
    Pos,
    Not,
    BitNot,
}

/// Augmented assignment operators (+=, -=, etc.).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AugAssignOp {
    Add,
    Sub,
    Mul,
    Div,
    FloorDiv,
    Mod,
    Pow,
    /// `@=` — in-place matmul (lowers through `__matmul__`, like CPython
    /// falling back when no `__imatmul__` is defined).
    MatMul,
    BitAnd,
    BitOr,
    BitXor,
    ShiftLeft,
    ShiftRight,
}
