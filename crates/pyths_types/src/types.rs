use pyths_syntax::ast::{Expr, ExprKind};
use pyths_syntax::operators::{BinOp, UnaryOp};

/// Internal type representation for the type checker.
#[derive(Debug, Clone, PartialEq)]
pub enum Type {
    Int,
    Float,
    Str,
    Bool,
    NoneType,
    List(Box<Type>),
    Dict(Box<Type>, Box<Type>),
    Optional(Box<Type>),
    Tuple(Vec<Type>),
    Set(Box<Type>),
    Union(Vec<Type>),
    Named(String),
    Callable(Vec<Type>, Box<Type>),
    /// Generic type parameter inside a stub-declared signature. Bound
    /// to a concrete type at each call site via [`unify`] and
    /// [`substitute`] in the checker. Outside of generic resolution,
    /// a free `TypeVar` behaves like `Any`.
    TypeVar(String),
    Any,
    Void,
}

/// Whether a name (typically a single capital letter like `T`, `U`,
/// `K`, `V`, `E`, or a suffixed form like `T_co` / `T_contra`) is a
/// conventional generic type-variable name. Stubs use this convention
/// in lieu of explicit `TypeVar("T")` declarations.
pub fn is_type_var_name(name: &str) -> bool {
    // Single uppercase ASCII letter.
    if name.len() == 1 && name.chars().next().is_some_and(|c| c.is_ascii_uppercase()) {
        return true;
    }
    // PEP 484 variance suffixes: T_co (covariant), T_contra (contravariant).
    if let Some(stem) = name
        .strip_suffix("_co")
        .or_else(|| name.strip_suffix("_contra"))
    {
        return stem.len() == 1 && stem.chars().next().is_some_and(|c| c.is_ascii_uppercase());
    }
    false
}

impl std::fmt::Display for Type {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Type::Int => write!(f, "int"),
            Type::Float => write!(f, "float"),
            Type::Str => write!(f, "str"),
            Type::Bool => write!(f, "bool"),
            Type::NoneType => write!(f, "None"),
            Type::List(inner) => write!(f, "List[{}]", inner),
            Type::Dict(k, v) => write!(f, "Dict[{}, {}]", k, v),
            Type::Optional(inner) => write!(f, "Optional[{}]", inner),
            Type::Tuple(elts) => {
                let parts: Vec<_> = elts.iter().map(|t| t.to_string()).collect();
                write!(f, "Tuple[{}]", parts.join(", "))
            }
            Type::Set(inner) => write!(f, "Set[{}]", inner),
            Type::Union(elts) => {
                let parts: Vec<_> = elts.iter().map(|t| t.to_string()).collect();
                write!(f, "{}", parts.join(" | "))
            }
            Type::Named(name) => write!(f, "{}", name),
            Type::Callable(params, ret) => {
                let parts: Vec<_> = params.iter().map(|t| t.to_string()).collect();
                write!(f, "Callable[[{}], {}]", parts.join(", "), ret)
            }
            Type::TypeVar(name) => write!(f, "{}", name),
            Type::Any => write!(f, "Any"),
            Type::Void => write!(f, "void"),
        }
    }
}

/// Resolve a Python type annotation Expr to an internal Type.
pub fn resolve_type(annotation: &Expr) -> Type {
    match &annotation.kind {
        ExprKind::Name(n) => match n.as_str() {
            "int" => Type::Int,
            "float" => Type::Float,
            "str" => Type::Str,
            "bool" => Type::Bool,
            "None" => Type::NoneType,
            "Any" => Type::Any,
            // Bare collection annotations (no subscript) — Tier 2/4/5 surface.
            // Element/key/value types default to Any so generic uses still
            // work; users wanting precise typing should write `list[int]`,
            // `dict[str, int]`, `tuple[int, str]`.
            "list" | "List" => Type::List(Box::new(Type::Any)),
            "dict" | "Dict" => Type::Dict(Box::new(Type::Any), Box::new(Type::Any)),
            "tuple" | "Tuple" => Type::Tuple(vec![]),
            "set" | "Set" => Type::Set(Box::new(Type::Any)),
            other => {
                if is_type_var_name(other) {
                    Type::TypeVar(other.to_string())
                } else if other.chars().next().is_some_and(|c| c.is_uppercase()) {
                    Type::Named(other.to_string())
                } else {
                    Type::Any
                }
            }
        },
        ExprKind::NoneLiteral => Type::NoneType,
        ExprKind::Subscript { value, index, .. } => {
            if let ExprKind::Name(n) = &value.kind {
                match n.as_str() {
                    "List" | "list" => Type::List(Box::new(resolve_type(index))),
                    "Dict" | "dict" => {
                        if let ExprKind::Tuple(elts) = &index.kind {
                            let k = elts.first().map(resolve_type).unwrap_or(Type::Any);
                            let v = elts.get(1).map(resolve_type).unwrap_or(Type::Any);
                            Type::Dict(Box::new(k), Box::new(v))
                        } else {
                            Type::Dict(Box::new(resolve_type(index)), Box::new(Type::Any))
                        }
                    }
                    "Optional" => Type::Optional(Box::new(resolve_type(index))),
                    "Set" | "set" => Type::Set(Box::new(resolve_type(index))),
                    "Tuple" | "tuple" => {
                        if let ExprKind::Tuple(elts) = &index.kind {
                            Type::Tuple(elts.iter().map(resolve_type).collect())
                        } else {
                            Type::Tuple(vec![resolve_type(index)])
                        }
                    }
                    "Union" => {
                        if let ExprKind::Tuple(elts) = &index.kind {
                            Type::Union(elts.iter().map(resolve_type).collect())
                        } else {
                            resolve_type(index)
                        }
                    }
                    "Callable" => {
                        if let ExprKind::Tuple(elts) = &index.kind {
                            if elts.len() == 2 {
                                let ret = resolve_type(&elts[1]);
                                let params = if let ExprKind::List(params) = &elts[0].kind {
                                    params.iter().map(resolve_type).collect()
                                } else {
                                    vec![resolve_type(&elts[0])]
                                };
                                Type::Callable(params, Box::new(ret))
                            } else {
                                Type::Any
                            }
                        } else {
                            Type::Any
                        }
                    }
                    _ => Type::Any,
                }
            } else {
                Type::Any
            }
        }
        _ => Type::Any,
    }
}

/// Check if `source` type is assignable to `target` type.
pub fn is_assignable(target: &Type, source: &Type) -> bool {
    // Any is compatible with everything
    if matches!(target, Type::Any) || matches!(source, Type::Any) {
        return true;
    }

    // Free TypeVars (not yet bound) behave like Any for compatibility
    // checks. Binding happens at call sites via `unify`; once a TypeVar
    // is bound, it's been substituted away and no longer appears here.
    if matches!(target, Type::TypeVar(_)) || matches!(source, Type::TypeVar(_)) {
        return true;
    }

    // Exact match
    if target == source {
        return true;
    }

    // NoneType → Optional[T] is ok
    if matches!(source, Type::NoneType) && matches!(target, Type::Optional(_)) {
        return true;
    }

    // Int → Float is ok (numeric widening)
    if matches!(target, Type::Float) && matches!(source, Type::Int) {
        return true;
    }

    // Bool → Int is ok (bool is a subtype of int in Python)
    if matches!(target, Type::Int) && matches!(source, Type::Bool) {
        return true;
    }

    // T → Optional[T] is ok
    if let Type::Optional(inner) = target {
        if is_assignable(inner, source) {
            return true;
        }
    }

    // T → Union[..., T, ...] is ok
    if let Type::Union(variants) = target {
        if variants.iter().any(|v| is_assignable(v, source)) {
            return true;
        }
    }

    // Union[T] → T: any variant assignable to target
    if let Type::Union(variants) = source {
        if variants.iter().all(|v| is_assignable(target, v)) {
            return true;
        }
    }

    // Named types match by name
    if let (Type::Named(a), Type::Named(b)) = (target, source) {
        return a == b;
    }

    // List[T] → List[U] if T assignable to U
    if let (Type::List(t), Type::List(s)) = (target, source) {
        return is_assignable(t, s);
    }

    // Dict[K1, V1] → Dict[K2, V2]
    if let (Type::Dict(tk, tv), Type::Dict(sk, sv)) = (target, source) {
        return is_assignable(tk, sk) && is_assignable(tv, sv);
    }

    // Set[T] → Set[U]
    if let (Type::Set(t), Type::Set(s)) = (target, source) {
        return is_assignable(t, s);
    }

    false
}

/// Infer the type of a literal expression.
pub fn infer_literal_type(expr: &Expr) -> Option<Type> {
    match &expr.kind {
        ExprKind::IntLiteral(_) => Some(Type::Int),
        ExprKind::FloatLiteral(_) => Some(Type::Float),
        ExprKind::StringLiteral(_) => Some(Type::Str),
        ExprKind::FString { .. } => Some(Type::Str),
        ExprKind::BoolLiteral(_) => Some(Type::Bool),
        ExprKind::NoneLiteral => Some(Type::NoneType),
        ExprKind::List(elts) => {
            let inner = infer_homogeneous_type(elts);
            Some(Type::List(Box::new(inner)))
        }
        ExprKind::Dict { .. } => Some(Type::Dict(Box::new(Type::Any), Box::new(Type::Any))),
        ExprKind::Set(elts) => {
            let inner = infer_homogeneous_type(elts);
            Some(Type::Set(Box::new(inner)))
        }
        ExprKind::Tuple(elts) => {
            let types: Vec<_> = elts
                .iter()
                .map(|e| infer_literal_type(e).unwrap_or(Type::Any))
                .collect();
            Some(Type::Tuple(types))
        }
        _ => None,
    }
}

/// Infer the result type of a binary operation.
pub fn infer_binop_type(left: &Type, op: BinOp, right: &Type) -> Type {
    match op {
        // `@` (matmul) is pure dunder dispatch on user classes; no builtin
        // operand types, so nothing useful to infer.
        BinOp::MatMul => Type::Any,
        // Arithmetic operators
        BinOp::Add | BinOp::Sub | BinOp::Mul | BinOp::Pow => {
            match (left, right) {
                // String concatenation
                (Type::Str, Type::Str) if op == BinOp::Add => Type::Str,
                // String repetition
                (Type::Str, Type::Int) if op == BinOp::Mul => Type::Str,
                (Type::Int, Type::Str) if op == BinOp::Mul => Type::Str,
                // List concatenation
                (Type::List(t), Type::List(_)) if op == BinOp::Add => Type::List(t.clone()),
                // List repetition
                (Type::List(t), Type::Int) if op == BinOp::Mul => Type::List(t.clone()),
                // Numeric: float wins
                (Type::Float, _) | (_, Type::Float) => Type::Float,
                (Type::Int, Type::Int) => Type::Int,
                (Type::Bool, Type::Bool) => Type::Int,
                _ => Type::Any,
            }
        }
        BinOp::Div => {
            // Division always returns float in Python 3
            match (left, right) {
                (Type::Int | Type::Float, Type::Int | Type::Float) => Type::Float,
                _ => Type::Any,
            }
        }
        BinOp::FloorDiv | BinOp::Mod => {
            match (left, right) {
                (Type::Float, _) | (_, Type::Float) => Type::Float,
                (Type::Int, Type::Int) => Type::Int,
                // String formatting
                (Type::Str, _) if op == BinOp::Mod => Type::Str,
                _ => Type::Any,
            }
        }
        // Comparison operators always return bool
        BinOp::Eq
        | BinOp::NotEq
        | BinOp::Lt
        | BinOp::LtEq
        | BinOp::Gt
        | BinOp::GtEq
        | BinOp::In
        | BinOp::NotIn
        | BinOp::Is
        | BinOp::IsNot => Type::Bool,
        // Logical operators
        BinOp::And | BinOp::Or => {
            // `and` returns the first falsy or last truthy value
            // `or` returns the first truthy or last falsy value
            // Approximate: if both same type, return that type
            if left == right {
                left.clone()
            } else {
                Type::Any
            }
        }
        // Bitwise operators on ints
        BinOp::BitAnd | BinOp::BitOr | BinOp::BitXor | BinOp::ShiftLeft | BinOp::ShiftRight => {
            match (left, right) {
                (Type::Int, Type::Int) => Type::Int,
                (Type::Bool, Type::Bool) => Type::Bool,
                _ => Type::Any,
            }
        }
        // PythScribe extensions — return Any since we can't infer further
        BinOp::NullishCoalesce | BinOp::Pipeline => Type::Any,
    }
}

/// Infer the result type of a unary operation.
pub fn infer_unaryop_type(op: UnaryOp, operand: &Type) -> Type {
    match op {
        UnaryOp::Neg | UnaryOp::Pos => match operand {
            Type::Int => Type::Int,
            Type::Float => Type::Float,
            Type::Bool => Type::Int,
            _ => Type::Any,
        },
        UnaryOp::Not => Type::Bool,
        UnaryOp::BitNot => match operand {
            Type::Int => Type::Int,
            Type::Bool => Type::Int,
            _ => Type::Any,
        },
    }
}

/// Try to infer a common element type from a list of expressions.
fn infer_homogeneous_type(elts: &[Expr]) -> Type {
    if elts.is_empty() {
        return Type::Any;
    }
    let first = infer_literal_type(&elts[0]);
    if let Some(ref t) = first {
        if elts
            .iter()
            .skip(1)
            .all(|e| infer_literal_type(e).as_ref() == Some(t))
        {
            return t.clone();
        }
    }
    Type::Any
}

/// Unwrap Optional[T] to T for type narrowing purposes.
pub fn unwrap_optional(ty: &Type) -> Type {
    match ty {
        Type::Optional(inner) => *inner.clone(),
        other => other.clone(),
    }
}

// ============================================================================
// Generic type-variable inference
// ============================================================================

use std::collections::HashMap;

/// Map from TypeVar name to its bound concrete type.
pub type Bindings = HashMap<String, Type>;

/// Return `true` if `ty` (transitively) contains any free TypeVar.
pub fn contains_type_var(ty: &Type) -> bool {
    match ty {
        Type::TypeVar(_) => true,
        Type::List(inner) | Type::Set(inner) | Type::Optional(inner) => contains_type_var(inner),
        Type::Dict(k, v) => contains_type_var(k) || contains_type_var(v),
        Type::Tuple(elts) | Type::Union(elts) => elts.iter().any(contains_type_var),
        Type::Callable(params, ret) => {
            params.iter().any(contains_type_var) || contains_type_var(ret)
        }
        _ => false,
    }
}

/// Substitute bound TypeVars in `ty` with their concrete types.
/// Unbound TypeVars pass through unchanged.
pub fn substitute(ty: &Type, bindings: &Bindings) -> Type {
    match ty {
        Type::TypeVar(name) => bindings.get(name).cloned().unwrap_or_else(|| ty.clone()),
        Type::List(inner) => Type::List(Box::new(substitute(inner, bindings))),
        Type::Set(inner) => Type::Set(Box::new(substitute(inner, bindings))),
        Type::Optional(inner) => Type::Optional(Box::new(substitute(inner, bindings))),
        Type::Dict(k, v) => Type::Dict(
            Box::new(substitute(k, bindings)),
            Box::new(substitute(v, bindings)),
        ),
        Type::Tuple(elts) => Type::Tuple(elts.iter().map(|t| substitute(t, bindings)).collect()),
        Type::Union(elts) => Type::Union(elts.iter().map(|t| substitute(t, bindings)).collect()),
        Type::Callable(params, ret) => Type::Callable(
            params.iter().map(|t| substitute(t, bindings)).collect(),
            Box::new(substitute(ret, bindings)),
        ),
        // Concrete leaves pass through.
        other => other.clone(),
    }
}

/// Walk `target` (which may contain TypeVars) against `source` (concrete),
/// recording each TypeVar's binding. Returns `false` if a TypeVar would
/// have to bind to two incompatible types; this signals a generic mismatch
/// the caller may want to flag — though our current usage just falls back
/// to `Any`, since the rest of the type checker is intentionally lenient.
pub fn unify(target: &Type, source: &Type, bindings: &mut Bindings) -> bool {
    match (target, source) {
        (Type::TypeVar(name), _) => {
            if let Some(existing) = bindings.get(name).cloned() {
                // Already bound — check the source is compatible. If
                // both directions assignable, leave the existing binding;
                // if not, widen to a Union as a best-effort.
                if is_assignable(&existing, source) {
                    true
                } else if is_assignable(source, &existing) {
                    bindings.insert(name.clone(), source.clone());
                    true
                } else {
                    // Incompatible bindings — widen to Union so callers
                    // get a still-useful (if loose) inferred type.
                    bindings.insert(name.clone(), Type::Union(vec![existing, source.clone()]));
                    true
                }
            } else {
                bindings.insert(name.clone(), source.clone());
                true
            }
        }
        // Concrete structural matches recurse.
        (Type::List(t), Type::List(s)) => unify(t, s, bindings),
        (Type::Set(t), Type::Set(s)) => unify(t, s, bindings),
        (Type::Optional(t), Type::Optional(s)) => unify(t, s, bindings),
        (Type::Dict(tk, tv), Type::Dict(sk, sv)) => {
            unify(tk, sk, bindings) && unify(tv, sv, bindings)
        }
        (Type::Tuple(ts), Type::Tuple(ss)) if ts.len() == ss.len() => {
            ts.iter().zip(ss.iter()).all(|(t, s)| unify(t, s, bindings))
        }
        (Type::Callable(tp, tr), Type::Callable(sp, sr)) if tp.len() == sp.len() => {
            tp.iter().zip(sp.iter()).all(|(t, s)| unify(t, s, bindings)) && unify(tr, sr, bindings)
        }
        // T → Optional[T]: bind T to source (the source already wraps).
        (Type::Optional(inner), _) => unify(inner, source, bindings),
        // Anything else: no new bindings to record, defer to is_assignable
        // semantics (which already covers Any and concrete-equality cases).
        _ => true,
    }
}
