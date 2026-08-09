use std::cell::Cell;
use std::collections::{HashMap, HashSet};

use pyths_syntax::ast::*;
use pyths_syntax::operators::*;

use crate::builtins::{builtin_func_mapping, BuiltinMapping};
use crate::method_lowering::{is_simple_receiver, method_lowering, InlineSpec, MethodLowering};
use crate::react;
use crate::sourcemap::{self, SourceMapBuilder};

/// Maximum expression-tree nesting depth the JS emitter will walk. Left-
/// associative chains (`1+1+…`, `a[0][0]…`, `a.b.b…`, `1<1<1…`) are parsed
/// *iteratively* — so they slip past the parser's recursion guard — but the
/// codegen tree-walk (`emit_expr`) recurses down their spine and would overflow
/// the native stack. This bound matches the parser's `MAX_PARSE_DEPTH` so both
/// front and back ends reject the same "too deeply nested" input with a clean
/// diagnostic. Reached comfortably on the large compile stack (see
/// `pyths_cli::main`); far above any real expression.
pub const MAX_EMIT_DEPTH: usize = 1000;

thread_local! {
    /// Current `emit_expr` recursion depth on the active thread.
    static EMIT_DEPTH: Cell<usize> = const { Cell::new(0) };
    /// Source byte-offset of the first expression that exceeded
    /// `MAX_EMIT_DEPTH` during the current codegen, if any. The emit walk
    /// returns `()` (it writes into a buffer), so it can't propagate a
    /// `Result`; instead it records the overflow out-of-band here and emits a
    /// runtime-throwing placeholder, and the driver drains this with
    /// [`take_emit_overflow`] to surface a clean compile-time diagnostic.
    static EMIT_OVERFLOW: Cell<Option<usize>> = const { Cell::new(None) };
}

/// Take (and clear) the codegen depth-overflow marker recorded since the last
/// call. `Some(offset)` means an expression nested deeper than
/// [`MAX_EMIT_DEPTH`] was encountered at that source byte offset; the emitted
/// JS contains a runtime-throwing placeholder in its place, so the caller
/// should discard the output and report a clean error.
pub fn take_emit_overflow() -> Option<usize> {
    EMIT_OVERFLOW.with(|c| c.take())
}

/// RAII depth counter for the codegen tree-walk. Mirrors the parser's
/// `DepthGuard`: increments on `enter`, decrements on drop (covering every
/// return path).
struct EmitDepthGuard;

impl EmitDepthGuard {
    fn enter() -> Option<EmitDepthGuard> {
        let depth = EMIT_DEPTH.with(|d| {
            let next = d.get() + 1;
            d.set(next);
            next
        });
        if depth > MAX_EMIT_DEPTH {
            EMIT_DEPTH.with(|d| d.set(d.get() - 1));
            None
        } else {
            Some(EmitDepthGuard)
        }
    }
}

impl Drop for EmitDepthGuard {
    fn drop(&mut self) {
        EMIT_DEPTH.with(|d| d.set(d.get().saturating_sub(1)));
    }
}

// ── Dataclass validation structures ──────────────────────

/// A value in a choices constraint.
#[derive(Debug, Clone)]
enum ChoiceValue {
    Str(String),
    Int(i128),
    Float(f64),
}

/// A field in a @dataclass class.
struct DataclassField<'a> {
    name: String,
    annotation: Option<&'a Expr>,
    default: Option<&'a Expr>,
    constraints: FieldConstraints,
}

/// Numeric/string constraints from Field(...).
#[derive(Default)]
struct FieldConstraints {
    gt: Option<f64>,
    ge: Option<f64>,
    lt: Option<f64>,
    le: Option<f64>,
    min_length: Option<usize>,
    max_length: Option<usize>,
    pattern: Option<String>,
    default_factory: Option<String>,
    // string validators
    email: bool,
    url: bool,
    uuid: bool,
    starts_with: Option<String>,
    ends_with: Option<String>,
    includes: Option<String>,
    // string transforms
    trim: bool,
    to_lower: bool,
    to_upper: bool,
    // number validators
    positive: bool,
    negative: bool,
    nonnegative: bool,
    multiple_of: Option<f64>,
    finite: bool,
    // enum
    choices: Vec<ChoiceValue>,
}

/// Options parsed from @dataclass(frozen=True, ...).
#[derive(Default)]
struct DataclassOptions {
    frozen: bool,
    coerce: bool,
    collect_errors: bool,
}

/// Resolved type check for a field annotation.
enum TypeCheck {
    Int,
    Float,
    Str,
    Bool,
    List(Option<Box<TypeCheck>>),
    // Key/value types parsed and stored, but not yet consumed by the
    // emitter (dict checks are structural — `instanceof Object`). The
    // fields are pre-wired for the planned generic-dict element-type
    // emission; suppressing the dead-code warning until then.
    #[allow(dead_code)]
    Dict(Option<Box<TypeCheck>>, Option<Box<TypeCheck>>),
    Instance(String),
    Optional(Box<TypeCheck>),
    None,
}

/// Resolve a Python type annotation AST node to a TypeCheck.
fn resolve_type_check(annotation: &Expr) -> TypeCheck {
    match &annotation.kind {
        ExprKind::Name(n) => match n.as_str() {
            "int" => TypeCheck::Int,
            "float" => TypeCheck::Float,
            "str" => TypeCheck::Str,
            "bool" => TypeCheck::Bool,
            "list" => TypeCheck::List(None),
            "dict" => TypeCheck::Dict(None, None),
            other => {
                if other.chars().next().is_some_and(|c| c.is_uppercase()) {
                    TypeCheck::Instance(other.to_string())
                } else {
                    TypeCheck::None
                }
            }
        },
        ExprKind::Subscript { value, index, .. } => {
            if let ExprKind::Name(n) = &value.kind {
                match n.as_str() {
                    "list" | "List" => {
                        let inner = resolve_type_check(index);
                        TypeCheck::List(Some(Box::new(inner)))
                    }
                    "dict" | "Dict" => {
                        if let ExprKind::Tuple(elts) = &index.kind {
                            let k = elts.first().map(|e| Box::new(resolve_type_check(e)));
                            let v = elts.get(1).map(|e| Box::new(resolve_type_check(e)));
                            TypeCheck::Dict(k, v)
                        } else {
                            TypeCheck::Dict(Some(Box::new(resolve_type_check(index))), None)
                        }
                    }
                    "Optional" => {
                        let inner = resolve_type_check(index);
                        TypeCheck::Optional(Box::new(inner))
                    }
                    _ => TypeCheck::None,
                }
            } else {
                TypeCheck::None
            }
        }
        _ => TypeCheck::None,
    }
}

/// Parse @dataclass or @dataclass(frozen=True) decorator.
/// Returns (is_dataclass, options).
fn parse_dataclass_decorator(decorator: &Expr) -> (bool, DataclassOptions) {
    match &decorator.kind {
        ExprKind::Name(n) if n == "dataclass" => (true, DataclassOptions::default()),
        ExprKind::Call { func, kwargs, .. } => {
            if let ExprKind::Name(n) = &func.kind {
                if n == "dataclass" {
                    let mut opts = DataclassOptions::default();
                    for kw in kwargs {
                        if let Some(name) = &kw.name {
                            match name.as_str() {
                                "frozen" => {
                                    if let ExprKind::BoolLiteral(v) = &kw.value.kind {
                                        opts.frozen = *v;
                                    }
                                }
                                "coerce" => {
                                    if let ExprKind::BoolLiteral(v) = &kw.value.kind {
                                        opts.coerce = *v;
                                    }
                                }
                                "collect_errors" => {
                                    if let ExprKind::BoolLiteral(v) = &kw.value.kind {
                                        opts.collect_errors = *v;
                                    }
                                }
                                _ => {}
                            }
                        }
                    }
                    return (true, opts);
                }
            }
            (false, DataclassOptions::default())
        }
        _ => (false, DataclassOptions::default()),
    }
}

/// Parse Field(...) or field(...) kwargs into constraints.
fn parse_field_constraints(kwargs: &[Keyword]) -> (FieldConstraints, Option<&Expr>) {
    let mut c = FieldConstraints::default();
    let mut default = None;
    for kw in kwargs {
        if let Some(name) = &kw.name {
            match name.as_str() {
                "gt" => {
                    c.gt = expr_to_f64(&kw.value);
                }
                "ge" => {
                    c.ge = expr_to_f64(&kw.value);
                }
                "lt" => {
                    c.lt = expr_to_f64(&kw.value);
                }
                "le" => {
                    c.le = expr_to_f64(&kw.value);
                }
                "min_length" => {
                    c.min_length = expr_to_usize(&kw.value);
                }
                "max_length" => {
                    c.max_length = expr_to_usize(&kw.value);
                }
                "pattern" => {
                    if let ExprKind::StringLiteral(s) = &kw.value.kind {
                        c.pattern = Some(s.clone());
                    }
                }
                "default" => {
                    default = Some(&kw.value);
                }
                "default_factory" => {
                    if let ExprKind::Name(n) = &kw.value.kind {
                        c.default_factory = Some(n.clone());
                    }
                }
                // string validators
                "email" => c.email = expr_to_bool(&kw.value),
                "url" => c.url = expr_to_bool(&kw.value),
                "uuid" => c.uuid = expr_to_bool(&kw.value),
                "starts_with" => {
                    if let ExprKind::StringLiteral(s) = &kw.value.kind {
                        c.starts_with = Some(s.clone());
                    }
                }
                "ends_with" => {
                    if let ExprKind::StringLiteral(s) = &kw.value.kind {
                        c.ends_with = Some(s.clone());
                    }
                }
                "includes" => {
                    if let ExprKind::StringLiteral(s) = &kw.value.kind {
                        c.includes = Some(s.clone());
                    }
                }
                // string transforms
                "trim" => c.trim = expr_to_bool(&kw.value),
                "to_lower" => c.to_lower = expr_to_bool(&kw.value),
                "to_upper" => c.to_upper = expr_to_bool(&kw.value),
                // number validators
                "positive" => c.positive = expr_to_bool(&kw.value),
                "negative" => c.negative = expr_to_bool(&kw.value),
                "nonnegative" => c.nonnegative = expr_to_bool(&kw.value),
                "multiple_of" => c.multiple_of = expr_to_f64(&kw.value),
                "finite" => c.finite = expr_to_bool(&kw.value),
                // enum choices
                "choices" => {
                    if let ExprKind::List(elts) = &kw.value.kind {
                        for elt in elts {
                            match &elt.kind {
                                ExprKind::StringLiteral(s) => {
                                    c.choices.push(ChoiceValue::Str(s.clone()))
                                }
                                ExprKind::IntLiteral(n) => c.choices.push(ChoiceValue::Int(*n)),
                                ExprKind::FloatLiteral(n) => c.choices.push(ChoiceValue::Float(*n)),
                                _ => {}
                            }
                        }
                    }
                }
                _ => {}
            }
        }
    }
    (c, default)
}

fn expr_to_f64(expr: &Expr) -> Option<f64> {
    match &expr.kind {
        ExprKind::IntLiteral(n) => Some(*n as f64),
        ExprKind::FloatLiteral(n) => Some(*n),
        ExprKind::UnaryOp {
            op: UnaryOp::Neg,
            operand,
        } => expr_to_f64(operand).map(|v| -v),
        _ => None,
    }
}

fn expr_to_usize(expr: &Expr) -> Option<usize> {
    if let ExprKind::IntLiteral(n) = &expr.kind {
        Some(*n as usize)
    } else {
        None
    }
}

fn expr_to_bool(expr: &Expr) -> bool {
    matches!(&expr.kind, ExprKind::BoolLiteral(true))
}

/// Collect dataclass fields from the class body.
fn collect_dataclass_fields<'a>(body: &'a [Stmt]) -> Vec<DataclassField<'a>> {
    let cap = body
        .iter()
        .filter(|s| matches!(s.kind, StmtKind::AnnAssign { .. }))
        .count();
    let mut fields = Vec::with_capacity(cap);
    for stmt in body {
        if let StmtKind::AnnAssign {
            target,
            annotation,
            value,
        } = &stmt.kind
        {
            if let ExprKind::Name(field_name) = &target.kind {
                // Check if value is Field(...) or field(...)
                if let Some(val) = value {
                    if let ExprKind::Call {
                        func, kwargs, args, ..
                    } = &val.kind
                    {
                        if let ExprKind::Name(fn_name) = &func.kind {
                            if fn_name == "Field" || fn_name == "field" {
                                let (constraints, default) = parse_field_constraints(kwargs);
                                // Also check first positional arg as default
                                let default = default.or_else(|| args.first());
                                fields.push(DataclassField {
                                    name: field_name.clone(),
                                    annotation: Some(annotation),
                                    default,
                                    constraints,
                                });
                                continue;
                            }
                        }
                    }
                }
                fields.push(DataclassField {
                    name: field_name.clone(),
                    annotation: Some(annotation),
                    default: value.as_ref(),
                    constraints: FieldConstraints::default(),
                });
            }
        }
    }
    fields
}

/// Lightweight type classification used by the JS codegen to decide
/// when to route operations through Python-faithful runtime helpers
/// instead of the bare JS operator.
///
/// This is intentionally coarser than `pyths_types::Type` — the codegen
/// only needs to know "collection vs primitive vs unknown" for the
/// JS-quirk-fixing call sites (`==` deep compare, `if []` truthiness,
/// `[] + []` spread concat). Full type inference lives in
/// `pyths_types`; we mirror just enough here for emit-time decisions.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum JsInferredType {
    /// `int`, `bool`, `str`, `None` — JS truthiness and `===` match
    /// Python semantics, so these skip the helper wrap.
    Primitive,
    /// Provably `float` (float literal, `float`-annotated, true-division
    /// result, or arithmetic among floats). A float is always a JS
    /// `Number` — never promoted to BigInt — so arithmetic on two Floats
    /// can skip the arbitrary-precision helper and emit a bare JS op
    /// (P2 native fast path). Treated exactly like `Primitive` everywhere
    /// else (non-collection, `===` equality, no truthiness wrap).
    Float,
    List,
    Dict,
    Set,
    Tuple,
    /// We can't pin the type from the expression shape — could be
    /// anything. Conservative call sites (`if x:`) wrap in `pyBool`;
    /// the equality path leaves `===` for these since wrapping every
    /// `==` would be a perf hit on numerics that flow through `Name`s.
    Unknown,
}

impl JsInferredType {
    fn is_collection(self) -> bool {
        matches!(
            self,
            JsInferredType::List
                | JsInferredType::Dict
                | JsInferredType::Set
                | JsInferredType::Tuple
        )
    }

    /// `int`/`bool`/`str`/`None` (Primitive) or `float` (Float) — the
    /// scalar kinds that share JS truthiness / `===` semantics.
    fn is_scalar(self) -> bool {
        matches!(self, JsInferredType::Primitive | JsInferredType::Float)
    }
}

/// Arbitrary-precision-faithful arithmetic helpers (inline mirror of
/// `runtime/src/operators.js`). A Python `int` is a JS `Number` while it
/// fits the safe-integer range and a `BigInt` once it would overflow
/// 2**53; `__intBin` computes integer ops exactly (promoting to BigInt on
/// overflow, normalizing back to Number when the result fits). Emitted by
/// `emit_inline_runtime` when any arithmetic helper is needed.
const PY_ARITH_JS: &str = r#"const __MAX_SAFE = 9007199254740991n;
const __isFloat = (x) => typeof x === "number" && !Number.isSafeInteger(x);
const __toBig = (x) => (typeof x === "bigint" ? x : BigInt(x));
const __norm = (big) => (big >= -__MAX_SAFE && big <= __MAX_SAFE ? Number(big) : big);
function __intBin(a, b, numOp, bigOp) {
    if (typeof a === "number" && typeof b === "number") {
        const r = numOp(a, b);
        if (Number.isSafeInteger(r)) return r;
        return __norm(bigOp(BigInt(a), BigInt(b)));
    }
    return __norm(bigOp(__toBig(a), __toBig(b)));
}
function __zde(msg) { const e = new Error(msg); e.name = "ZeroDivisionError"; return e; }
function __ofe(msg) { const e = new Error(msg); e.name = "OverflowError"; return e; }
const __reqNum = (x) => {
    if (typeof x === "bigint") {
        const n = Number(x);
        if (!isFinite(n)) throw __ofe("int too large to convert to float");
        return n;
    }
    return Number(x);
};
const __numeric = (x) => typeof x === "number" || typeof x === "bigint";
function __opTypeName(v) {
    if (v === null || v === undefined) return "NoneType";
    switch (typeof v) {
        case "boolean": return "bool";
        case "number": return Number.isInteger(v) ? "int" : "float";
        case "bigint": return "int";
        case "string": return "str";
    }
    if (Array.isArray(v)) return v.__pytuple__ ? "tuple" : "list";
    if (v instanceof Set) return "set";
    if (v instanceof Map) return "dict";
    const c = v && v.constructor;
    return (c && (c.__name__ || c.name)) || "object";
}
function __arithNoneGuard(op, a, b) {
    if (a == null || b == null) {
        const e = new Error(`unsupported operand type(s) for ${op}: '${__opTypeName(a)}' and '${__opTypeName(b)}'`);
        e.name = "TypeError"; throw e;
    }
}
function __arithTypeErr(msg) { const e = new Error(msg); e.name = "TypeError"; return e; }
const __arithNumOk = (x) => typeof x === "number" || typeof x === "bigint" || typeof x === "boolean";
function __reqArithNum(op, a, b) {
    if (!__arithNumOk(a) || !__arithNumOk(b)) {
        throw __arithTypeErr(`unsupported operand type(s) for ${op}: '${__opTypeName(a)}' and '${__opTypeName(b)}'`);
    }
}
// Wave-15 F4: bool ⊂ int — coerce bool operands (when the other side is
// numeric/bool) before arithmetic so bool+BigInt stays exact.
const __boolNum = (x) => (x ? 1 : 0);
function pyAdd(a, b, fctx) {
    if (typeof a === "boolean" && __arithNumOk(b)) a = __boolNum(a);
    if (typeof b === "boolean" && __arithNumOk(a)) b = __boolNum(b);
    if (__numeric(a) && __numeric(b)) {
        if (fctx || __isFloat(a) || __isFloat(b)) return __reqNum(a) + __reqNum(b);
        return __intBin(a, b, (x, y) => x + y, (x, y) => x + y);
    }
    if (a != null && typeof a.__add__ === "function") return a.__add__(b);
    if (b != null && typeof b.__radd__ === "function") return b.__radd__(a);
    if (Array.isArray(a) && Array.isArray(b)) {
        const at = !!a.__pytuple__, bt = !!b.__pytuple__;
        if (at !== bt) throw new TypeError(`can only concatenate ${at ? "tuple" : "list"} (not "${bt ? "tuple" : "list"}") to ${at ? "tuple" : "list"}`);
        const r = [...a, ...b];
        if (at) Object.defineProperty(r, "__pytuple__", { value: true, enumerable: false });
        return r;
    }
    __arithNoneGuard("+", a, b);
    if ((typeof a === "string") !== (typeof b === "string")) throw new TypeError('can only concatenate str to str');
    return a + b;
}
function pySub(a, b) {
    if (typeof a === "boolean" && __arithNumOk(b)) a = __boolNum(a);
    if (typeof b === "boolean" && __arithNumOk(a)) b = __boolNum(b);
    if (__numeric(a) && __numeric(b)) {
        if (__isFloat(a) || __isFloat(b)) return Number(a) - Number(b);
        return __intBin(a, b, (x, y) => x - y, (x, y) => x - y);
    }
    if (a instanceof Set && b instanceof Set) { const out = new (a.constructor)(a); for (const v of b) out.delete(v); return out; }
    if (a != null && typeof a.__sub__ === "function") return a.__sub__(b);
    if (b != null && typeof b.__rsub__ === "function") return b.__rsub__(a);
    __reqArithNum("-", a, b);
    return Number(a) - Number(b);
}
function pyMul(a, b, fctx) {
    if (__numeric(a) && __numeric(b)) {
        if (fctx || __isFloat(a) || __isFloat(b)) return __reqNum(a) * __reqNum(b);
        return __intBin(a, b, (x, y) => x * y, (x, y) => x * y);
    }
    if (a != null && typeof a.__mul__ === "function") return a.__mul__(b);
    if (b != null && typeof b.__rmul__ === "function") return b.__rmul__(a);
    if (typeof a === "string" && (typeof b === "number" || typeof b === "bigint")) return a.repeat(Number(b));
    if ((typeof a === "number" || typeof a === "bigint") && typeof b === "string") return b.repeat(Number(a));
    if (Array.isArray(a) && (typeof b === "number" || typeof b === "bigint")) {
        const result = []; const n = Number(b);
        for (let i = 0; i < n; i++) result.push(...a);
        return result;
    }
    __arithNoneGuard("*", a, b);
    return a * b;
}
function pyDiv(a, b, floatDiv) {
    if (typeof a === "boolean" && __arithNumOk(b)) a = __boolNum(a);
    if (typeof b === "boolean" && __arithNumOk(a)) b = __boolNum(b);
    if ((!__numeric(a) || !__numeric(b)) && a != null && typeof a.__truediv__ === "function") return a.__truediv__(b);
    __reqArithNum("/", a, b);
    const bn = __reqNum(b);
    if (bn === 0) throw __zde((floatDiv || __isFloat(a) || __isFloat(b)) ? "float division by zero" : "division by zero");
    return __reqNum(a) / bn;
}
function pyFloorDiv(a, b) {
    if (typeof a === "boolean" && __arithNumOk(b)) a = __boolNum(a);
    if (typeof b === "boolean" && __arithNumOk(a)) b = __boolNum(b);
    if ((!__numeric(a) || !__numeric(b)) && a != null && typeof a.__floordiv__ === "function") return a.__floordiv__(b);
    __reqArithNum("//", a, b);
    if (__isFloat(a) || __isFloat(b)) {
        const x = Number(a), y = Number(b);
        if (y === 0) throw __zde("float floor division by zero");
        const mod = x % y;
        let div = (x - mod) / y;
        if (mod !== 0 && (y < 0) !== (mod < 0)) div -= 1;
        let fd = Math.floor(div);
        if (div - fd > 0.5) fd += 1;
        return fd;
    }
    if (Number(b) === 0) throw __zde("integer division or modulo by zero");
    return __intBin(a, b, (x, y) => Math.floor(x / y), (x, y) => { let q = x / y; if (x % y !== 0n && (x < 0n) !== (y < 0n)) q -= 1n; return q; });
}
function pyMod(a, b) {
    if (typeof a === "boolean" && __arithNumOk(b)) a = __boolNum(a);
    if (typeof b === "boolean" && __arithNumOk(a)) b = __boolNum(b);
    if ((!__numeric(a) || !__numeric(b)) && a != null && typeof a.__mod__ === "function") return a.__mod__(b);
    // Honest unsupported-feature error: printf-style %-formatting is a
    // surface PythScribe does not implement (known limitation; use f-strings).
    if (typeof a === "string") {
        const e = new Error("printf-style %-formatting is not supported by PythScribe; use an f-string");
        e.name = "NotImplementedError"; throw e;
    }
    __reqArithNum("%", a, b);
    if (__isFloat(a) || __isFloat(b)) {
        const bf = Number(b);
        if (bf === 0) throw __zde("float modulo by zero");
        return ((Number(a) % bf) + bf) % bf;
    }
    if (Number(b) === 0) throw __zde("integer division or modulo by zero");
    return __intBin(a, b, (x, y) => ((x % y) + y) % y, (x, y) => ((x % y) + y) % y);
}
function pyPow(a, b, fctx) {
    if (typeof a === "boolean" && __arithNumOk(b)) a = __boolNum(a);
    if (typeof b === "boolean" && __arithNumOk(a)) b = __boolNum(b);
    if (__numeric(a) && __numeric(b)) {
        if (fctx || __isFloat(a) || __isFloat(b) || (typeof b === "bigint" ? b < 0n : b < 0)) {
            const r = __reqNum(a) ** __reqNum(b);
            if (!isFinite(r) && isFinite(__reqNum(a)) && isFinite(__reqNum(b))) throw __ofe("(34, 'Result too large')");
            return r;
        }
        return __intBin(a, b, (x, y) => x ** y, (x, y) => x ** y);
    }
    if (a != null && typeof a.__pow__ === "function") return a.__pow__(b);
    __reqArithNum("** or pow()", a, b);
    return Number(a) ** Number(b);
}
"#;

/// Cooperative-MRO object-model helpers, emitted when a pure-PythScribe
/// class hierarchy is present. `__pyC3` computes the C3 linearization (the
/// same merge CPython uses); `__pyClass` installs `__mro__`/`__bases__` and
/// mixes in methods from non-first bases (first MRO definer wins);
/// `__pySuper` returns a cooperative super proxy that dispatches to the
/// next class *after the defining class* in the instance's MRO — so
/// diamonds chain L→R→Base, matching Python.
const PY_OBJECT_MODEL_JS: &str = r#"const __PY_MIXIN = Symbol("pyMixin");
class PyObject {
    constructor(...args) {
        const cls = new.target;
        const mro = cls && cls.__mro__ ? cls.__mro__ : [cls];
        for (const c of mro) {
            if (c && c.prototype && Object.prototype.hasOwnProperty.call(c.prototype, "__init__")) {
                c.prototype.__init__.apply(this, args);
                return;
            }
        }
    }
    __init__() {}
}
PyObject.__mro__ = [PyObject];
function __pyC3(cls, bases) {
    const seqs = bases.map((b) => (b && b.__mro__ ? b.__mro__.slice() : [b]));
    if (bases.length) seqs.push(bases.slice());
    const result = [cls];
    while (true) {
        const nonEmpty = seqs.filter((s) => s.length > 0);
        if (nonEmpty.length === 0) break;
        let cand = null;
        for (const s of nonEmpty) {
            const head = s[0];
            const inTail = nonEmpty.some((o) => o.indexOf(head) > 0);
            if (!inTail) { cand = head; break; }
        }
        if (cand === null) {
            const e = new Error("Cannot create a consistent method resolution order (MRO)");
            e.name = "TypeError"; throw e;
        }
        result.push(cand);
        for (const s of nonEmpty) { if (s[0] === cand) s.shift(); }
    }
    return result;
}
function __pyClass(cls, bases) {
    cls.__bases__ = bases;
    const mro = __pyC3(cls, bases);
    if (mro.indexOf(PyObject) < 0) mro.push(PyObject);
    cls.__mro__ = mro;
    const proto = cls.prototype;
    // Diamond-safe method flattening: a name resolves to the FIRST class in the
    // MRO whose *body* defines it. Flattened copies carried on a base (tagged in
    // that base's __PY_MIXIN set) do NOT count as the base defining the method,
    // so B's copy of A.who can't mask C's genuine override in class D(B, C).
    const bodyNames = new Set(Object.getOwnPropertyNames(proto));
    bodyNames.delete("constructor");
    const finalized = new Set(bodyNames);
    const copied = new Set();
    for (let i = 1; i < mro.length; i++) {
        const base = mro[i];
        if (!base || !base.prototype || base === PyObject) continue;
        const baseMixed = Object.prototype.hasOwnProperty.call(base.prototype, __PY_MIXIN)
            ? base.prototype[__PY_MIXIN]
            : null;
        for (const name of Object.getOwnPropertyNames(base.prototype)) {
            if (name === "constructor") continue;
            if (finalized.has(name)) continue;
            if (baseMixed && baseMixed.has(name)) continue;
            Object.defineProperty(proto, name, Object.getOwnPropertyDescriptor(base.prototype, name));
            finalized.add(name);
            copied.add(name);
        }
    }
    Object.defineProperty(proto, __PY_MIXIN, { value: copied, enumerable: false, writable: true, configurable: true });
    if (typeof proto.__iter__ === "function" && !proto[Symbol.iterator]) {
        Object.defineProperty(proto, Symbol.iterator, {
            value() {
                const it = this.__iter__();
                if (it != null && typeof it.next === "function") return it;
                return it[Symbol.iterator]();
            },
            writable: true, configurable: true,
        });
    }
    if (typeof proto.__next__ === "function" && typeof proto.next !== "function") {
        Object.defineProperty(proto, "next", {
            value() {
                try { return { value: this.__next__(), done: false }; }
                catch (e) { if (e && e.name === "StopIteration") return { value: undefined, done: true }; throw e; }
            },
            writable: true, configurable: true,
        });
    }
    return cls;
}
function __pySuper(startCls, inst) {
    const ctor = inst.constructor;
    const mro = ctor && ctor.__mro__ ? ctor.__mro__ : [ctor];
    const idx = mro.indexOf(startCls);
    const after = idx >= 0 ? mro.slice(idx + 1) : [];
    return new Proxy(Object.create(null), {
        get(_t, prop) {
            for (const c of after) {
                if (!c || !c.prototype) continue;
                if (!Object.prototype.hasOwnProperty.call(c.prototype, prop)) continue;
                const mixed = Object.prototype.hasOwnProperty.call(c.prototype, __PY_MIXIN)
                    ? c.prototype[__PY_MIXIN]
                    : null;
                if (mixed && typeof prop === "string" && mixed.has(prop)) continue;
                const val = c.prototype[prop];
                return typeof val === "function" ? val.bind(inst) : val;
            }
            return undefined;
        },
    });
}
function __pyIsInstance(obj, cls) {
    // Builtin TYPE names arrive as string sentinels (the codegen lowers
    // isinstance(x, list) to __pyIsInstance(x, "list") — `list` has no JS
    // value). Documented residual: whole floats report as int, and unmarked
    // (derived) tuples report as list — both fundamental to representing
    // Python ints/floats/tuples as bare JS numbers/arrays at runtime.
    // (#215: `isinstance(True, int)` now correctly True — bool ⊆ int.)
    if (typeof cls === "string") {
        switch (cls) {
            case "list": return Array.isArray(obj) && !obj.__pytuple__;
            case "tuple": return Array.isArray(obj) && !!obj.__pytuple__;
            case "str": return typeof obj === "string";
            case "bool": return typeof obj === "boolean";
            // bool is a subclass of int in Python, so a boolean is an int.
            case "int": return typeof obj === "boolean" || typeof obj === "bigint" || (typeof obj === "number" && Number.isInteger(obj));
            case "float": return typeof obj === "number" && !Number.isInteger(obj);
            case "dict": return obj !== null && typeof obj === "object" && (Object.getPrototypeOf(obj) === Object.prototype || obj instanceof Map);
            case "set": return obj instanceof Set;
            case "NoneType": return obj === null || obj === undefined;
        }
        return false;
    }
    // `type(x)` for a builtin returns an interned type OBJECT (__PyTypeObj)
    // whose __name__ is the CPython type name, not a JS constructor. Route
    // `isinstance(x, type(x))` through the string path via that name.
    if (cls !== null && typeof cls === "object" && typeof cls.__name__ === "string" && cls.constructor && cls.constructor.name === "__PyTypeObj") {
        return __pyIsInstance(obj, cls.__name__);
    }
    if (typeof cls !== "function" && cls != null && typeof cls[Symbol.iterator] === "function") {
        for (const c of cls) { if (__pyIsInstance(obj, c)) return true; }
        return false;
    }
    if (obj == null) return false;
    try { if (obj instanceof cls) return true; } catch (_e) {}
    const ctor = obj.constructor;
    const mro = ctor && ctor.__mro__ ? ctor.__mro__ : null;
    return mro ? mro.indexOf(cls) >= 0 : false;
}
function __pyClassAttr(cls, name, value) {
    cls[name] = value;
    Object.defineProperty(cls.prototype, name, {
        get() { return cls[name]; },
        set(v) { Object.defineProperty(this, name, { value: v, writable: true, enumerable: true, configurable: true }); },
        configurable: true,
    });
}
function __pyClassCall(cls, name, args) {
    const s = cls[name];
    if (typeof s === "function") return cls[name](...args);
    const m = cls.prototype ? cls.prototype[name] : undefined;
    if (typeof m === "function") return m.call(args[0], ...args.slice(1));
    const e = new Error("type object '" + cls.name + "' has no attribute '" + name + "'");
    e.name = "AttributeError";
    throw e;
}
"#;

/// One frame of the class-emission stack.
struct ClassCtx {
    /// Class name — `super()` lowers to `__pySuper(<name>, this)`.
    name: String,
    /// Whether this class uses the cooperative PyObject object model
    /// (regular classes). When true, `__init__` is emitted as a prototype
    /// method (dispatched cooperatively via the MRO) rather than the JS
    /// `constructor`. Exception subclasses and `@dataclass` keep `false` —
    /// they retain native `constructor` + native-`super()` semantics.
    pyobject_model: bool,
    /// #300: whether the class has explicit bases. A NATIVE-path derived
    /// class (`pyobject_model == false` with a base) must call `super()`
    /// before touching `this` — when the Python `__init__` has no
    /// `super().__init__(...)` to hoist, a bare `super();` is synthesized.
    has_bases: bool,
}

/// JavaScript code generator.
/// Walks the AST and emits JavaScript source code.
pub struct JsCodegen {
    output: String,
    indent: usize,
    /// #201: import statements that appeared inside a function body. ES
    /// `import` is only legal at the module top level, so a function-local
    /// `import x` is captured here and flushed into the module preamble by
    /// `finish()` (the local name still binds — module scope is a superset).
    hoisted_imports: Vec<String>,
    runtime_imports: HashSet<String>,
    inline_runtime: bool,
    /// Stack of scopes tracking declared variable names.
    /// Used to decide `let x = ...` (first) vs `x = ...` (reassignment).
    declared_scopes: Vec<HashSet<String>>,
    /// #274: JS binding names already emitted by a module-scope `import` (the
    /// name after `as`, or the plain imported name). Python tolerates importing
    /// the same name twice (idempotent rebind); ES modules do not — a second
    /// `import { defaultdict }` is a "already declared" SyntaxError. Dedupe by
    /// binding so re-imports (common when a file re-imports a name its preamble
    /// / another line already brought in) are dropped.
    imported_bindings: HashSet<String>,
    /// #269: names genuinely HOISTED to this scope as a function/module `let`
    /// (via `collect_hoisted_names`), as opposed to merely block-scoped in a
    /// nested for-loop/if. Mirrors `declared_scopes` push/pop. Only a hoisted
    /// for-target may be emitted as a bare `for (i of …)` binding — a target
    /// that was `const`-declared inside a *sibling* loop is NOT in scope here.
    hoisted_scopes: Vec<HashSet<String>>,
    /// PBT-2: subset of `hoisted_scopes` — hoisted for-loop targets whose
    /// `let` was initialized to the __UNBOUND sentinel because no other
    /// binding guarantees them a value (zero-iteration loops must leave the
    /// name raising on read, per CPython). Mirrors `declared_scopes` push/pop.
    sentinel_scopes: Vec<HashSet<String>>,
    /// Per-scope name → coarse inferred type for the JS-quirk fixes.
    /// Mirrors `declared_scopes` exactly (push/pop in lockstep).
    local_types: Vec<HashMap<String, JsInferredType>>,
    /// When true, the immediately-pending Subscript/Attribute emission is
    /// the LHS of an assignment — emit bare `a[i]` / `x.y`, not the
    /// pyGetItem-wrapped read form. Reset to false before descending into
    /// the index/value sub-expressions (those are still reads).
    in_lhs_target: bool,
    /// Whether we're currently inside a @component function (enables PSX).
    in_component: bool,
    /// Round-3: inside a @classmethod body (`cls(...)` news the class).
    in_classmethod: bool,
    /// F6: param-name → hidden once-evaluated const name for the function
    /// currently being emitted. Python evaluates each default ONCE at def
    /// time; a bare JS default param re-evaluates per call (so `def f(xs=[])`
    /// would hand out a fresh list every call). emit_func_def hoists each
    /// default to `const <hidden> = <expr>;` before the declaration and
    /// records the mapping here; emit_params then references the hidden name.
    param_default_hoists: HashMap<String, String>,
    /// Monotonic counter making each hoisted-default const name unique.
    default_hoist_counter: usize,
    /// #350: module-level (top-level) function/class names already emitted as
    /// declarations, in source order. A second `def`/`class` of the same name
    /// is a Python last-binding-wins redefinition — JS would throw
    /// "Identifier already declared" for a duplicate `function`/`class`, so the
    /// redefinition is emitted as a reassignment (`f = function …`) instead.
    module_decl_names: std::collections::HashSet<String>,
    /// Whether createElement needs to be auto-imported from React.
    needs_create_element: bool,
    /// Whether Fragment needs to be auto-imported from React.
    needs_fragment: bool,
    /// Whether we're in collect_errors mode for dataclass validation.
    collecting_errors: bool,
    /// Source map builder (Some when source maps are enabled).
    sourcemap: Option<SourceMapBuilder>,
    /// Original source text (for byte offset → line:col conversion).
    source_text: Option<String>,
    /// A17: when true, omit `sourcesContent` from the emitted source map so
    /// original `.ps` source (comments, server-side logic, secrets) is NOT
    /// inlined into a production `.js.map`. Mappings still resolve; the
    /// original text just is not shipped.
    omit_sources_content: bool,
    /// Current output line (0-based).
    out_line: u32,
    /// Current output column (0-based).
    out_col: u32,
    /// Function names compiled to WASM — skip emitting their JS bodies.
    wasm_skip: HashSet<String>,
    /// Set of names defined as `class` at module/top level. Used by
    /// `emit_call` to disambiguate `Alert(...)` calls inside @component
    /// functions: a known class name → `new Alert(...)` (instantiation),
    /// otherwise → `createElement(Alert, ...)` (React component).
    known_classes: HashSet<String>,
    /// #300: names imported via a RELATIVE import (`from .shape import
    /// Shape`) — i.e. from another module of the same PythScribe project,
    /// which the same compiler lowers with the same object model. Used by
    /// `emit_class_def`: a base class drawn from this set takes the
    /// cooperative PyObject/`__init__` path (like a same-file base), NOT
    /// the native-`constructor` path reserved for genuinely external bases
    /// (React.Component, npm classes — always absolute imports).
    local_module_imports: HashSet<String>,
    /// Module-level `def`s annotated `-> float` (#136): calls to these
    /// are definitely-float for repr/str/print/f-string formatting.
    float_returning_functions: HashSet<String>,
    /// #106: locals later subscript-WRITTEN with a provably-non-string
    /// LITERAL key (d[1] = ..., d[True] = ...). Dict literals assigned
    /// to these names must construct in the Map-backed PyDict shape —
    /// a plain JS object physically cannot hold a non-string key, and
    /// the shape is fixed at construction.
    pydict_forced_locals: HashSet<String>,
    /// One-shot flag consumed by the next emit_dict_literal.
    force_pydict_literal: bool,
    /// Codegen-time diagnostics surfaced to the caller via
    /// `take_errors()`. Populated by Strategy::Unsupported entries in
    /// the method-lowering table — those produce a `throw new Error(...)`
    /// in the emitted JS plus an entry here so the CLI can fail the
    /// build cleanly rather than handing the user a runtime crash.
    codegen_errors: Vec<String>,
    /// Subscript-routing certificate (credible compilation, §7.2):
    /// every subscript decision is recorded here as it is made; the
    /// caller can drain it with `take_certificate()` and validate with
    /// `cert::check_certificate` against the emitted JS.
    certificate: crate::cert::Certificate,
    /// Names imported (without a user-supplied alias) from a React-like
    /// module. The import line camelCases the JS specifier — `from
    /// react_query import use_query` emits `import { useQuery } from
    /// "react-query"`. For the *local* binding to match, every reference
    /// to the Python-source name (in calls, attribute access, JSX
    /// element types, etc.) must also camelCase. Tracked here so the
    /// Name emitter can do the transform without re-querying the
    /// import resolver. User aliases (`from foo import use_x as my_x`)
    /// bypass this — `my_x` is what the user wrote and what we emit.
    react_imports: HashSet<String>,
    /// Track-B: LOCAL bindings (alias-aware) imported from React-ecosystem
    /// npm modules (`is_react_or_next_module`). PSX props on these tags get
    /// the same snake→camel conversion HTML tags get — the receiving library
    /// speaks camelCase (`onOpenChange`, `asChild`), unlike user @components
    /// whose snake_case prop vocabulary is preserved.
    react_lib_bindings: HashSet<String>,
    /// Track-B: local MODULE aliases bound to React-ecosystem npm modules
    /// (`import at_radix_ui.react_dialog as DialogPrimitive`). Dotted PSX
    /// tags rooted at one of these (`DialogPrimitive.Root`) are library
    /// components for prop-conversion purposes.
    react_lib_module_aliases: HashSet<String>,
    /// Track-B: bindings whose LOWERCASE members are React components
    /// (framer-motion's `motion.div` / `motion.span`). Member calls rooted
    /// here dispatch to createElement even though the attr is lowercase.
    react_member_component_bases: HashSet<String>,
    /// User-supplied `pyths.toml [npm.imports]` overrides. Keys are
    /// Python-source module names (with dots), values are JS module
    /// specifiers emitted verbatim. Consulted before the built-in
    /// `NPM_MODULE_MAPPINGS` table and before the kebab-case fallback.
    npm_imports: HashMap<String, String>,
    /// Whether to emit React Refresh signatures + registrations for
    /// `@component` functions. When true, each capitalized `@component`
    /// gets wrapped with `$RefreshSig$()` / `$RefreshReg$(...)` calls so
    /// the build plugin's HMR layer can preserve component state across
    /// edits. The `$RefreshSig$` / `$RefreshReg$` symbols are expected
    /// to be installed as module-level globals by the plugin — the
    /// codegen does not import them.
    react_refresh: bool,
    /// Stack of class names currently being emitted (innermost last).
    /// `super()` inside a method compiles to `__pySuper(<top>, this)` so
    /// the cooperative-MRO runtime helper knows the *defining* class to
    /// search forward from.
    class_stack: Vec<ClassCtx>,
    /// The npm package specifier to use for `from "<pkg>" import` when
    /// emitting runtime-helper imports. Defaults to `"pyths-runtime"`.
    /// Set to `"pyths-runtime/core"` for `--target worker` so numeric /
    /// Worker-safe modules auto-emit DOM-free imports (B-030 follow-up D).
    runtime_pkg: &'static str,
    /// #80: names defined as top-level `def` — a capitalized function name
    /// (`def Foo(): ...`) must NOT be `new`-called by the class-
    /// instantiation capitalization heuristic in emit_call.
    known_functions: HashSet<String>,
    /// #91: one frame per enclosing loop (innermost last). `Some(flag)`
    /// when that loop carries an `else` clause — `break` then sets the
    /// flag before breaking so the else clause is suppressed; `None` for
    /// loops without else (break must NOT touch an outer loop's flag).
    loop_flag_stack: Vec<Option<String>>,
    /// Monotonic counter making each loop's break flag unique — two
    /// sibling `for/else` loops in the same block must not redeclare one
    /// shared `let __for_broke`.
    loop_flag_counter: usize,
    /// Round-4 sweep: local names bound to the asyncio module namespace
    /// (`import asyncio` / `import asyncio as aio`) — used to detect
    /// `asyncio.run(...)` calls, which must be awaited (JS's shim returns
    /// a Promise; Python's blocks).
    asyncio_namespaces: HashSet<String>,
    /// #221: local names bound to a stdlib module namespace (`import re`,
    /// `import math as m`). A method-name that collides with a string/list
    /// method (`re.split`, `os.count`) must NOT be lowered as that method —
    /// it is a module function call, emitted verbatim (`re.split(...)`).
    module_namespaces: HashSet<String>,
    /// Local names bound to asyncio's `run` via `from asyncio import run`.
    asyncio_run_fns: HashSet<String>,
    /// Whether `await` is legal at the current emission point: module top
    /// level (ESM top-level await) and async function bodies. Sync
    /// function bodies set this false.
    await_ok: bool,
}

impl Default for JsCodegen {
    fn default() -> Self {
        Self::new()
    }
}

impl JsCodegen {
    pub fn new() -> Self {
        Self {
            output: String::with_capacity(4096),
            indent: 0,
            hoisted_imports: Vec::new(),
            runtime_imports: HashSet::new(),
            inline_runtime: false,
            declared_scopes: vec![HashSet::new()], // module scope
            hoisted_scopes: vec![HashSet::new()],  // module scope
            sentinel_scopes: vec![HashSet::new()], // module scope
            imported_bindings: HashSet::new(),
            local_types: vec![HashMap::new()],
            in_lhs_target: false,
            in_component: false,
            in_classmethod: false,
            param_default_hoists: HashMap::new(),
            default_hoist_counter: 0,
            module_decl_names: std::collections::HashSet::new(),
            needs_create_element: false,
            needs_fragment: false,
            collecting_errors: false,
            sourcemap: None,
            source_text: None,
            omit_sources_content: false,
            out_line: 0,
            out_col: 0,
            wasm_skip: HashSet::new(),
            known_classes: HashSet::new(),
            local_module_imports: HashSet::new(),
            float_returning_functions: HashSet::new(),
            pydict_forced_locals: HashSet::new(),
            force_pydict_literal: false,
            codegen_errors: Vec::new(),
            certificate: crate::cert::Certificate::default(),
            react_imports: HashSet::new(),
            react_lib_bindings: HashSet::new(),
            react_lib_module_aliases: HashSet::new(),
            react_member_component_bases: HashSet::new(),
            npm_imports: HashMap::new(),
            react_refresh: false,
            class_stack: Vec::new(),
            runtime_pkg: "pyths-runtime",
            known_functions: HashSet::new(),
            loop_flag_stack: Vec::new(),
            loop_flag_counter: 0,
            asyncio_namespaces: HashSet::new(),
            module_namespaces: HashSet::new(),
            asyncio_run_fns: HashSet::new(),
            await_ok: true,
        }
    }

    /// Create a codegen that inlines runtime helpers instead of importing them.
    /// Use this for `pyths run` so the output is self-contained.
    pub fn new_inline() -> Self {
        Self {
            output: String::with_capacity(4096),
            indent: 0,
            hoisted_imports: Vec::new(),
            runtime_imports: HashSet::new(),
            inline_runtime: true,
            declared_scopes: vec![HashSet::new()], // module scope
            hoisted_scopes: vec![HashSet::new()],  // module scope
            sentinel_scopes: vec![HashSet::new()], // module scope
            imported_bindings: HashSet::new(),
            local_types: vec![HashMap::new()],
            in_lhs_target: false,
            in_component: false,
            in_classmethod: false,
            param_default_hoists: HashMap::new(),
            default_hoist_counter: 0,
            module_decl_names: std::collections::HashSet::new(),
            needs_create_element: false,
            needs_fragment: false,
            collecting_errors: false,
            sourcemap: None,
            source_text: None,
            omit_sources_content: false,
            out_line: 0,
            out_col: 0,
            wasm_skip: HashSet::new(),
            known_classes: HashSet::new(),
            local_module_imports: HashSet::new(),
            float_returning_functions: HashSet::new(),
            pydict_forced_locals: HashSet::new(),
            force_pydict_literal: false,
            codegen_errors: Vec::new(),
            certificate: crate::cert::Certificate::default(),
            react_imports: HashSet::new(),
            react_lib_bindings: HashSet::new(),
            react_lib_module_aliases: HashSet::new(),
            react_member_component_bases: HashSet::new(),
            npm_imports: HashMap::new(),
            react_refresh: false,
            class_stack: Vec::new(),
            runtime_pkg: "pyths-runtime",
            known_functions: HashSet::new(),
            loop_flag_stack: Vec::new(),
            loop_flag_counter: 0,
            asyncio_namespaces: HashSet::new(),
            module_namespaces: HashSet::new(),
            asyncio_run_fns: HashSet::new(),
            await_ok: true,
        }
    }

    /// Enable React Refresh emission for `@component` functions. When
    /// enabled, each capitalized `@component` is wrapped in the
    /// `$RefreshSig$` / `$RefreshReg$` boilerplate that
    /// `react-refresh/runtime` expects. The build plugin (Vite,
    /// Next.js) is responsible for installing the `$RefreshSig$` /
    /// `$RefreshReg$` globals before the module evaluates and for
    /// emitting the HMR-accept handshake after it.
    pub fn enable_react_refresh(&mut self) {
        self.react_refresh = true;
    }

    /// Install project-level npm-import overrides from `pyths.toml
    /// [npm.imports]`. Keys are Python-source module names (with dots
    /// for sub-modules); values are the JS module specifier to emit
    /// verbatim. Consulted before the built-in mapping table and
    /// before the kebab-case fallback.
    pub fn set_npm_imports(&mut self, mappings: HashMap<String, String>) {
        self.npm_imports = mappings;
    }

    /// Override the package specifier used when emitting runtime-helper
    /// import statements. The default is `"pyths-runtime"`; pass
    /// `"pyths-runtime/core"` for `--target worker` so the output is
    /// DOM-free and Cloudflare-Worker-safe (B-030 follow-up D).
    pub fn set_runtime_pkg(&mut self, pkg: &'static str) {
        self.runtime_pkg = pkg;
    }

    /// Resolve a Python-source module name to its JS module specifier,
    /// honoring `pyths.toml [npm.imports]` overrides before the
    /// built-in resolution chain. Project overrides emit verbatim —
    /// the user is responsible for specifying the correct JS specifier
    /// (including any `@scope/` prefix or sub-path).
    fn resolve_module(&self, module: &str) -> String {
        if let Some(override_path) = self.npm_imports.get(module) {
            return override_path.clone();
        }
        resolve_module_path(module)
    }

    /// Set function names that should be skipped (compiled to WASM instead).
    pub fn set_wasm_skip(&mut self, names: HashSet<String>) {
        self.wasm_skip = names;
    }

    /// Emit import + re-export for WASM-compiled functions from the
    /// glue module. The bare `export { X } from "foo"` form is a
    /// transparent re-export only — it doesn't create a local binding,
    /// so any JS-side function in this module that references `X` (e.g.
    /// a `@component` that calls a WASM-routed numeric helper) would
    /// see a ReferenceError at runtime. We import the names locally
    /// AND re-export them so both call paths work.
    pub fn emit_wasm_reexports(&mut self, glue_filename: &str) {
        if self.wasm_skip.is_empty() {
            return;
        }
        let mut names: Vec<&String> = self.wasm_skip.iter().collect();
        names.sort();
        let joined = names
            .iter()
            .map(|s| s.as_str())
            .collect::<Vec<_>>()
            .join(", ");
        self.write(&format!(
            "\nimport {{ {} }} from \"{}\";\n",
            joined, glue_filename
        ));
        self.write(&format!("export {{ {} }};\n", joined));
    }

    pub fn finish(self) -> String {
        self.finish_certified().0
    }

    /// Like [`finish`], but also returns the layout information the subscript
    /// certificate needs to map a BODY-relative offset `o` (into `self.output`)
    /// to its offset in the FINAL js: `final = body_shift + o` for
    /// `o >= directive_len`. Returns `(js, body_shift, directive_len)`.
    ///
    /// Wiring note: `take_certificate` drains the certificate off `self` at
    /// lib.rs:112 BEFORE `finish` runs (both consume/borrow `self`), so we
    /// cannot store these values back onto `self.certificate` here — the cert
    /// is already gone. Instead `finish_certified` RETURNS them and
    /// `codegen_certified` stamps them onto the drained `Certificate` before
    /// handing it to `check_certificate`. This keeps `take_certificate`
    /// semantics intact and `finish` a thin wrapper (byte-identical js).
    pub fn finish_certified(self) -> (String, usize, usize) {
        let mut result = String::new();
        let mut body = &self.output[..];
        let mut directive_len = 0usize;

        // Extract "use client" / "use server" directive so it stays at the top
        for directive in &["\"use client\";\n", "\"use server\";\n"] {
            if body.starts_with(directive) {
                result.push_str(directive);
                body = &body[directive.len()..];
                directive_len = directive.len();
                break;
            }
        }

        if !self.runtime_imports.is_empty() {
            if self.inline_runtime {
                // Inline only the helpers that are actually used
                result.push_str(&Self::emit_inline_runtime(&self.runtime_imports));
                result.push('\n');
            } else {
                // Emit import statement
                let mut imports: Vec<&String> = self.runtime_imports.iter().collect();
                imports.sort();
                result.push_str("import { ");
                result.push_str(
                    &imports
                        .iter()
                        .map(|s| s.as_str())
                        .collect::<Vec<_>>()
                        .join(", "),
                );
                result.push_str(&format!(" }} from \"{}\";\n\n", self.runtime_pkg));
            }
        }

        // Auto-import createElement / Fragment from React when PSX is used
        if self.needs_create_element || self.needs_fragment {
            let mut react_imports = Vec::new();
            if self.needs_create_element {
                react_imports.push("createElement");
            }
            if self.needs_fragment {
                react_imports.push("Fragment");
            }
            result.push_str(&format!(
                "import {{ {} }} from \"react\";\n",
                react_imports.join(", ")
            ));
        }

        // #201: flush function-local imports hoisted to module scope. Skip any
        // whose exact line the top-level body already emits (avoids a duplicate
        // `import * as x` when the same module is imported both places).
        for line in &self.hoisted_imports {
            let dup =
                result.contains(&format!("{}\n", line)) || body.contains(&format!("{}\n", line));
            if !dup {
                result.push_str(line);
                result.push('\n');
            }
        }

        // `body_shift = P - D`, where `P = result.len()` at the moment the
        // post-directive body is appended and `D = directive_len`. Then a body
        // offset `o >= D` maps to `final = body_shift + o = P + (o - D)`.
        let body_shift = result.len() - directive_len;
        result.push_str(body);
        (result, body_shift, directive_len)
    }

    fn emit_inline_runtime(needed: &HashSet<String>) -> String {
        // A4: pyStr/pyPrint/pyRepr/pyFormatFloat form a small dependency
        // chain (pyPrint -> pyStr -> pyRepr -> pyFormatFloat) that isn't
        // visible to the generic `self.need_runtime(name)` call sites —
        // each call site only registers the ONE helper it directly calls.
        // Expand the requested set to its transitive closure here so the
        // inline (self-contained `pyths run`) output always defines every
        // helper it actually calls, matching how ES imports resolve
        // transitively for the non-inline path. (Function *declarations*
        // are hoisted in JS, so definition order below doesn't matter.)
        let mut needed = needed.clone();
        // #273: pyAnd/pyOr short-circuit on pyBool. #348: pyAny/pyAll too.
        if needed.contains("pyAnd")
            || needed.contains("pyOr")
            || needed.contains("pyAny")
            || needed.contains("pyAll")
        {
            needed.insert("pyBool".to_string());
        }
        if needed.contains("pyPrint") {
            needed.insert("pyStr".to_string());
        }
        if needed.contains("pyStr") {
            needed.insert("pyRepr".to_string());
        }
        if needed.contains("pyRepr") {
            needed.insert("pyFormatFloat".to_string());
        }
        // #283: complex literals lower to pyComplex; its __repr__ renders each
        // finite float component via pyFormatFloat (CPython complex-repr rules).
        if needed.contains("pyComplex") {
            needed.insert("pyFormatFloat".to_string());
        }
        // #129: format-spec helpers form their own chain — the dynamic
        // parser delegates to pyFormatSpec, and pyFormatSpec's f/F/%
        // types round through __fixedHalfEven (shared with pyFixed).
        if needed.contains("pyFormatDynamic") {
            needed.insert("pyFormatSpec".to_string());
        }
        // #110: tuple(iterable) factory builds on the pyTuple marker.
        if needed.contains("pyTupleOf") {
            needed.insert("pyTuple".to_string());
        }
        // list(iterable) factory routes through pySeq (dict → keys, str → code
        // points, range/gen/set materialized) and copies. pyListOf itself is
        // extracted from operators.js; pySeq is hand-written inline below, so
        // pull it in explicitly rather than relying on the extractor.
        if needed.contains("pyListOf") {
            needed.insert("pySeq".to_string());
        }
        // #341: single-arg round() of a non-finite float raises ValueError/
        // OverflowError (the inline pyRound now throws real class instances).
        if needed.contains("pyRound") {
            needed.insert("ValueError".to_string());
            needed.insert("OverflowError".to_string());
        }
        if needed.contains("pyFormatSpec") || needed.contains("pyFixed") {
            needed.insert("__fixedHalfEven".to_string());
        }
        // #347: pyFormatSpec's no-type-char float branch renders via pyFormatFloat.
        if needed.contains("pyFormatSpec") {
            needed.insert("pyFormatFloat".to_string());
        }
        // pyDivmod returns a tuple built from floor-div + mod.
        if needed.contains("pyDivmod") {
            needed.insert("pyTuple".to_string());
        }
        // #83 dict-shape dependency chain: pyBitOr's dict branch merges via
        // pyDictMerge; pyDictMerge/pyDict construct PyDicts; pyDictItems
        // marks pairs as tuples.
        if needed.contains("pyBitOr") {
            needed.insert("pyDictMerge".to_string());
        }
        if needed.contains("pyDictMerge") || needed.contains("pyDict") || needed.contains("pyCopy")
        {
            needed.insert("PyDict".to_string());
        }
        if needed.contains("pyDictItems")
            || needed.contains("pyDictPopitem")
            || needed.contains("pyZip")
        {
            needed.insert("pyTuple".to_string());
        }
        // pyMap iterates each argument via pyForIter (dicts → keys).
        if needed.contains("pyMap") {
            needed.insert("pyForIter".to_string());
        }
        // #214: the comparison operators need the lexicographic tuple/list
        // comparator so `(a, b) < (c, d)` matches Python (element-wise), not
        // JS array `<` (string coercion).
        if needed.contains("pyLt")
            || needed.contains("pyLe")
            || needed.contains("pyGt")
            || needed.contains("pyGe")
        {
            needed.insert("__seqLt".to_string());
        }
        // Drift fix: __seqLt now delegates element comparison to pyLt (matching
        // the package), so pyLt must be present whenever __seqLt is — even for a
        // program that only used `<=`/`>=`/`>` on sequences (which would pull
        // __seqLt but not pyLt).
        if needed.contains("__seqLt") {
            needed.insert("pyLt".to_string());
        }
        // #241: pyContains now compares elements with pyEq (bool≡int, tuples).
        if needed.contains("pyContains") {
            needed.insert("pyEq".to_string());
        }
        // #266: __pyMinmax (pyMin/pyMax) iterates its single arg via pyForIter
        // (so `max(dict, key=…)` iterates keys). #275: pySorted too — `sorted(d)`
        // sorts the dict KEYS, not its entries.
        if needed.contains("pyMin") || needed.contains("pyMax") || needed.contains("pySorted") {
            needed.insert("pyForIter".to_string());
        }
        let needed = &needed;

        let mut rt = String::new();
        rt.push_str("// --- PythScribe Runtime (inlined) ---\n");

        if needed.contains("pyRange") {
            rt.push_str(
                r#"function pyRange(startOrStop, stop, step) {
    const __b = (v) => (typeof v === "boolean" ? (v ? 1 : 0) : v);
    startOrStop = __b(startOrStop); stop = __b(stop); step = __b(step);
    let start;
    if (stop === undefined) { start = 0; stop = startOrStop; step = 1; }
    else { start = startOrStop; step = step || 1; }
    const result = [];
    if (step > 0) { for (let i = start; i < stop; i += step) result.push(i); }
    else if (step < 0) { for (let i = start; i > stop; i += step) result.push(i); }
    return result;
}
"#,
            );
        }
        if needed.contains("pyEnumerate") {
            // Sweep-A S2 fix: keyword calls (`enumerate(xs, start=1)`) pass
            // a `{start: 1}` options object (the codegen's universal
            // kwargs-as-object-literal convention), while positional calls
            // (`enumerate(xs, 1)`) pass a bare number — accept both shapes.
            rt.push_str(r#"function pyEnumerate(iterable, startArg = 0) {
    const start = (startArg && typeof startArg === "object") ? (startArg.start ?? 0) : startArg;
    const result = []; let i = start;
    for (const item of iterable) { const p = [i, item]; Object.defineProperty(p, "__pytuple__", { value: true, enumerable: false }); result.push(p); i++; }
    return result;
}
"#);
        }
        if needed.contains("pyZip") {
            // Lazy one-shot zip (Pythonic-checks sweep): terminates with
            // infinite iterators, yields pyTuple-marked rows, honors the
            // `strict=True` kwarg (trailing {strict} options object).
            rt.push_str(r#"function* pyZip(...iterables) {
    let strict = false;
    const last = iterables[iterables.length - 1];
    if (last !== null && typeof last === "object" && Object.getPrototypeOf(last) === Object.prototype && Object.prototype.hasOwnProperty.call(last, "strict")) {
        strict = !!last.strict;
        iterables = iterables.slice(0, -1);
    }
    if (iterables.length === 0) return;
    const iters = iterables.map((it) => {
        if (it instanceof Map) return it.keys();
        if (typeof it[Symbol.iterator] === "function") return it[Symbol.iterator]();
        return Object.keys(it)[Symbol.iterator]();
    });
    while (true) {
        const row = [];
        for (let i = 0; i < iters.length; i++) {
            const r = iters[i].next();
            if (r.done) {
                if (strict) {
                    const plural = (k) => k === 1 ? "argument 1" : `arguments 1-${k}`;
                    const bail = (msg) => { const e = new Error(msg); e.name = "ValueError"; throw e; };
                    if (i > 0) bail(`zip() argument ${i + 1} is shorter than ${plural(i)}`);
                    for (let j = 1; j < iters.length; j++) {
                        if (!iters[j].next().done) bail(`zip() argument ${j + 1} is longer than ${plural(j)}`);
                    }
                }
                return;
            }
            row.push(r.value);
        }
        yield pyTuple(...row);
    }
}
"#);
        }
        if needed.contains("pyMap") {
            // #110 unary map kept .map(fn) which fed (elem, index, array);
            // this form also supports CPython's multi-iterable
            // `map(f, xs, ys)` (f gets one arg per iterable, stops at the
            // shortest). Single-iterable path preserved exactly.
            rt.push_str(
                r#"function pyMap(fn, ...iterables) {
    if (iterables.length <= 1) return [...pyForIter(iterables[0])].map((x) => fn(x));
    const iters = iterables.map((it) => pyForIter(it)[Symbol.iterator]());
    const out = [];
    for (;;) {
        const row = [];
        for (const it of iters) { const r = it.next(); if (r.done) return out; row.push(r.value); }
        out.push(fn(...row));
    }
}
"#,
            );
        }
        if needed.contains("pySorted") {
            rt.push_str(
                r#"function pySorted(iterable, { key, reverse } = {}) {
    const arr = [...pyForIter(iterable)];
    const lt = (a, b) => {
        if (a != null && typeof a.__lt__ === "function") return !!a.__lt__(b);
        if (Array.isArray(a) && Array.isArray(b)) {
            const n = Math.min(a.length, b.length);
            for (let i = 0; i < n; i++) {
                if (lt(a[i], b[i])) return true;
                if (lt(b[i], a[i])) return false;
            }
            return a.length < b.length;
        }
        return a < b;
    };
    const cmp = (a, b) => (lt(a, b) ? -1 : lt(b, a) ? 1 : 0);
    const dir = reverse ? -1 : 1;
    if (key) arr.sort((a, b) => dir * cmp(key(a), key(b)));
    else arr.sort((a, b) => dir * cmp(a, b));
    return arr;
}
"#,
            );
        }
        if needed.contains("pyReversed") {
            rt.push_str("function pyReversed(iterable) { return [...iterable].reverse(); }\n");
        }
        if needed.contains("pyRound") {
            rt.push_str(
                r#"function __roundBigNeg(x, k) {
    const p = 10n ** BigInt(k);
    const neg = x < 0n;
    const a = neg ? -x : x;
    const q = a / p;
    const r = a % p;
    const twice = r * 2n;
    let up;
    if (twice < p) up = false;
    else if (twice > p) up = true;
    else up = (q % 2n) === 1n;
    const res = up ? (q + 1n) * p : q * p;
    return neg ? -res : res;
}
function pyRound(x, ndigits) {
    if (typeof x === "boolean") x = x ? 1 : 0;
    if (typeof ndigits === "boolean") ndigits = ndigits ? 1 : 0;
    if (typeof x === "bigint") {
        const nd = ndigits == null ? 0 : Math.trunc(Number(ndigits));
        return nd >= 0 ? x : __roundBigNeg(x, -nd);
    }
    if (typeof x === "number" && !isFinite(x)) {
        if (ndigits == null) {
            if (Number.isNaN(x)) throw new ValueError("cannot convert float NaN to integer");
            throw new OverflowError("cannot convert float infinity to integer");
        }
        return x;
    }
    if (x == null || typeof x !== "number") {
        throw new TypeError("type cannot be interpreted as a number");
    }
    const nd = ndigits == null ? 0 : Math.trunc(ndigits);
    const factor = Math.pow(10, nd);
    if (factor === 0) return x < 0 ? -0 : 0;
    if (!isFinite(factor)) return x;
    const scaled = x * factor;
    if (!isFinite(scaled)) return x;
    const floor = Math.floor(scaled);
    const diff = scaled - floor;
    let rounded;
    if (diff > 0.5) rounded = floor + 1;
    else if (diff < 0.5) rounded = floor;
    else rounded = floor % 2 === 0 ? floor : floor + 1;
    return rounded / factor;
}
"#,
            );
        }
        if needed.contains("pyLen") {
            rt.push_str(
                r#"function pyLen(obj) {
    if (obj == null) throw new TypeError("object of type 'NoneType' has no len()");
    if (typeof obj === "string") return /[\uD800-\uDBFF]/.test(obj) ? [...obj].length : obj.length;
    if (Array.isArray(obj)) return obj.length;
    if (obj instanceof Set || obj instanceof Map) return obj.size;
    if (typeof obj.__len__ === "function") return obj.__len__();
    if (typeof obj.next === "function" && typeof obj[Symbol.iterator] === "function") {
        throw new TypeError("object of type 'generator' has no len()");
    }
    if (typeof obj.length === "number") return obj.length;
    // Drift fix: match the package pyLen `.size` fallback (custom
    // collections exposing `.size` without being a Set/Map).
    if (typeof obj.size === "number") return obj.size;
    return Object.keys(obj).length;
}
"#,
            );
        }
        // PBT-1: pySlice is intentionally NOT hand-written here. The old
        // inline copy drifted from the package runtime (it never clamped
        // out-of-range bounds, so `[1,2,3][10:100]` returned None padding).
        // Like pySetSlice, it now flows through the #170 extraction fallback
        // below, which pulls the canonical runtime/src/runtime.js definition
        // with its dependencies — one source of truth for slice semantics.
        if needed.contains("pyBool") {
            rt.push_str(r#"function pyBool(x) {
    if (x == null) return false;
    if (typeof x === "boolean") return x;
    if (typeof x === "number") return x !== 0;
    if (typeof x === "string") return x.length > 0;
    if (Array.isArray(x)) return x.length > 0;
    if (x instanceof Set || x instanceof Map) return x.size > 0;
    if (typeof x.__bool__ === "function") return x.__bool__();
    if (typeof x.__len__ === "function") return x.__len__() > 0;
    if (typeof x === "object" && Object.getPrototypeOf(x) === Object.prototype) return Object.keys(x).length > 0;
    return true;
}
"#);
        }
        if needed.contains("pyAnd") {
            rt.push_str("function pyAnd(a, b) { return pyBool(a) ? b() : a; }\n");
        }
        if needed.contains("pyOr") {
            rt.push_str("function pyOr(a, b) { return pyBool(a) ? a : b(); }\n");
        }
        // #348: any()/all() consume the iterable lazily and short-circuit.
        // Mirrors runtime/src/types.js pyAny/pyAll.
        if needed.contains("pyAny") {
            rt.push_str("function pyAny(iterable) { for (const item of iterable) { if (pyBool(item)) return true; } return false; }\n");
        }
        if needed.contains("pyAll") {
            rt.push_str("function pyAll(iterable) { for (const item of iterable) { if (!pyBool(item)) return false; } return true; }\n");
        }
        // Builtin exception classes — emitted when user code raises or
        // subclasses one, so `class X(Exception)` / `raise ValueError(...)`
        // work under `pyths run` (the inline path), not just the npm/Vite
        // build.
        //
        // Drift fix: these are NO LONGER hand-written as flat `class X extends
        // Error { constructor(message) {...} }` stubs. That flat shape drifted
        // from the package in three behavioral ways — (1) it took a single
        // `message` arg and never set `.args`, so `repr(ValueError('a', 42))`
        // and `e.args` were wrong; (2) every class extended `Error` directly,
        // losing the CPython hierarchy (KeyError⊄LookupError etc.); (3)
        // KeyError/StopIteration lost their `str(e)`-repr / `.value` overrides.
        // It was ALSO inconsistent: only these 8 were hand-written while
        // LookupError/ArithmeticError/RuntimeError/NameError/… already flowed
        // through the #170 extraction fallback as the REAL package classes.
        //
        // Deleting the stubs routes ALL builtin exceptions through the same
        // extraction path (append_extracted_helpers), which pulls the canonical
        // `class Exception extends Error {…this.args = pyTuple(...args)…}` plus
        // its subclass hierarchy and transitive deps (pyTuple/__excStr/pyStr/
        // pyRepr) — inline == package by construction. (The hierarchy-aware
        // `emit_except_condition` name matching handles the *runtime-thrown*
        // name-tagged plain Errors, which remain plain Errors by design.)
        if needed.contains("PyDict") {
            // Mirrors runtime/src/runtime.js PyDict (#83): Map-backed dict
            // with CPython key canonicalization. See that copy for the full
            // derivation comments.
            rt.push_str(r#"const __TUPKEY = "\u0000pytuple\u0000";
let __objIdCounter = 0;
const __objIds = new WeakMap();
function __objId(o) { let id = __objIds.get(o); if (id === undefined) { id = ++__objIdCounter; __objIds.set(o, id); } return id; }
const __isPlainObj = (x) => { if (x === null || typeof x !== "object") return false; const p = Object.getPrototypeOf(x); return p === Object.prototype || p === null; };
const __unhashable = (n) => { const e = new Error(`unhashable type: '${n}'`); e.name = "TypeError"; return e; };
function __encodeTupleKey(t) {
    let s = "(";
    for (const el of t) {
        if (el === null || el === undefined) s += "N;";
        else if (typeof el === "boolean") s += "n:" + (el ? 1 : 0) + ";";
        else if (typeof el === "number" || typeof el === "bigint") s += "n:" + String(el) + ";";
        else if (typeof el === "string") s += "s:" + el.length + ":" + el + ";";
        else if (Array.isArray(el) && el.__pytuple__) s += __encodeTupleKey(el) + ";";
        else if (Array.isArray(el)) throw __unhashable("list");
        else if (el instanceof Set) throw __unhashable("set");
        else if (el instanceof Map || __isPlainObj(el)) throw __unhashable("dict");
        else s += "o:" + __objId(el) + ";";
    }
    return s + ")";
}
function __pyKey(k) {
    if (typeof k === "boolean") return k ? 1 : 0;
    if (typeof k === "bigint") { const n = Number(k); return (Number.isFinite(n) && BigInt(n) === k) ? n : k; } // crit-16: fold exactly-representable BigInts with Numbers
    if (Array.isArray(k)) { if (k.__pytuple__) return __TUPKEY + __encodeTupleKey(k); throw __unhashable("list"); }
    if (k instanceof Set) throw __unhashable("set");
    if (k instanceof Map || __isPlainObj(k)) throw __unhashable("dict");
    return k;
}
const __pyKeyObjs = new WeakMap();
class PyDict extends Map {
    constructor(src) {
        super();
        if (src != null) {
            if (src instanceof Map) { for (const [k, v] of src.entries()) this.set(k, v); }
            else if (typeof src[Symbol.iterator] === "function") { for (const [k, v] of src) this.set(k, v); }
            else { for (const k of Object.keys(src)) this.set(k, src[k]); }
        }
    }
    set(k, v) {
        const c = __pyKey(k);
        if ((typeof k === "boolean" || Array.isArray(k)) && !super.has(c)) {
            let m = __pyKeyObjs.get(this);
            if (!m) { m = new Map(); __pyKeyObjs.set(this, m); }
            m.set(c, k);
        }
        super.set(c, v);
        return this;
    }
    get(k) { return super.get(__pyKey(k)); }
    has(k) { return super.has(__pyKey(k)); }
    delete(k) { const c = __pyKey(k); const m = __pyKeyObjs.get(this); if (m) m.delete(c); return super.delete(c); }
    clear() { const m = __pyKeyObjs.get(this); if (m) m.clear(); super.clear(); }
    __key(c) { const m = __pyKeyObjs.get(this); return (m && m.has(c)) ? m.get(c) : c; }
    *keys() { for (const c of super.keys()) yield this.__key(c); }
    *entries() { for (const [c, v] of super.entries()) yield [this.__key(c), v]; }
    *[Symbol.iterator]() { yield* this.keys(); }
    forEach(fn, thisArg) { for (const [k, v] of this.entries()) fn.call(thisArg, v, k, this); }
}
"#);
        }
        if needed.contains("pyDict") {
            // Mirrors runtime/src/runtime.js pyDict (#83): dict() factory.
            rt.push_str(r#"function pyDict(src, kwargs) {
    const entries = [];
    if (src != null) {
        if (src instanceof Map) { for (const [k, v] of src.entries()) entries.push([k, v]); }
        else if (typeof src.keys === "function" && typeof src.__getitem__ === "function") { for (const k of src.keys()) entries.push([k, src.__getitem__(k)]); }
        else if (typeof src[Symbol.iterator] === "function" && typeof src !== "string") { for (const pair of src) entries.push([pair[0], pair[1]]); }
        else if (typeof src === "object") { for (const k of Object.keys(src)) entries.push([k, src[k]]); }
        else { const e = new Error(`'${typeof src}' object is not iterable`); e.name = "TypeError"; throw e; }
    }
    if (kwargs != null) for (const k of Object.keys(kwargs)) entries.push([k, kwargs[k]]);
    if (entries.every(([k]) => typeof k === "string")) {
        const out = {};
        for (const [k, v] of entries) {
            if (k === "__proto__") Object.defineProperty(out, k, { value: v, writable: true, enumerable: true, configurable: true });
            else out[k] = v;
        }
        return out;
    }
    return new PyDict(entries);
}
"#);
        }
        if needed.contains("pySetItem") {
            // Mirrors runtime/src/runtime.js pySetItem (#83).
            rt.push_str(r#"function pySetItem(obj, key, value) {
    if (obj == null) { const e = new Error("'NoneType' object does not support item assignment"); e.name = "TypeError"; throw e; }
    if (Array.isArray(obj)) {
        if (obj.__pytuple__) { const e = new Error("'tuple' object does not support item assignment"); e.name = "TypeError"; throw e; }
        let i = typeof key === "boolean" ? (key ? 1 : 0) : typeof key === "bigint" ? Number(key) : key;
        if (typeof i === "number" && Number.isInteger(i)) {
            if (i < 0) i += obj.length;
            if (i < 0 || i >= obj.length) { const e = new Error("list assignment index out of range"); e.name = "IndexError"; throw e; }
            obj[i] = value;
            return;
        }
        const e = new Error("list indices must be integers or slices"); e.name = "TypeError"; throw e;
    }
    if (obj instanceof Map) { obj.set(key, value); return; }
    if (typeof key === "boolean") key = key ? 1 : 0;
    if (typeof obj.__setitem__ === "function") { obj.__setitem__(key, value); return; }
    const proto = Object.getPrototypeOf(obj);
    if ((proto === Object.prototype || proto === null) && key === "__proto__") {
        Object.defineProperty(obj, "__proto__", { value, writable: true, enumerable: true, configurable: true });
        return;
    }
    obj[key] = value;
}
"#);
        }
        if needed.contains("pyDictKeys") {
            rt.push_str(r#"function pyDictKeys(d) {
    if (d == null) { const e = new Error("'NoneType' object is not iterable"); e.name = "TypeError"; throw e; }
    if (!Array.isArray(d) && typeof d.keys === "function") return [...d.keys()];
    return Object.keys(d);
}
"#);
        }
        if needed.contains("pyForIter") {
            rt.push_str(r#"function pyForIter(x) {
    if (x == null) { const e = new Error("'NoneType' object is not iterable"); e.name = "TypeError"; throw e; }
    if (typeof x === "string" || Array.isArray(x) || x instanceof Set) return x;
    if (x instanceof Map) return x.keys();
    if (typeof x[Symbol.iterator] === "function") return x;
    if (typeof x[Symbol.asyncIterator] === "function") return x;
    if (typeof x === "object") return Object.keys(x);
    return x;
}
"#);
        }
        if needed.contains("pySeq") {
            rt.push_str(r#"function pySeq(it) {
    if (Array.isArray(it)) return it;
    if (it == null) { const e = new Error("'NoneType' object is not iterable"); e.name = "TypeError"; throw e; }
    if (typeof it === "string") return /[\uD800-\uDBFF]/.test(it) ? [...it] : it.split("");
    if (it instanceof Map) return [...it.keys()];
    if (typeof it[Symbol.iterator] === "function") return [...it];
    if (typeof it === "object") return Object.keys(it);
    const e = new Error("'" + typeof it + "' object is not iterable"); e.name = "TypeError"; throw e;
}
"#);
        }
        if needed.contains("pyDictValues") {
            rt.push_str(r#"function pyDictValues(d) {
    if (d == null) { const e = new Error("'NoneType' object is not iterable"); e.name = "TypeError"; throw e; }
    if (!Array.isArray(d) && typeof d.values === "function") return [...d.values()];
    return Object.values(d);
}
"#);
        }
        if needed.contains("pyDictItems") {
            rt.push_str(r#"function pyDictItems(d) {
    if (d == null) { const e = new Error("'NoneType' object is not iterable"); e.name = "TypeError"; throw e; }
    if (!Array.isArray(d) && typeof d.entries === "function") return [...d.entries()].map((p) => pyTuple(p[0], p[1]));
    return Object.entries(d).map((p) => pyTuple(p[0], p[1]));
}
"#);
        }
        if needed.contains("pyDictGet") {
            // Mirrors runtime/src/runtime.js pyDictGet. (Previously absent
            // from the inline runtime entirely — `pyths run` crashed with
            // ReferenceError on any d.get(); noticed while wiring #83.)
            rt.push_str(
                r#"function pyDictGet(d, k, defaultValue) {
    if (d instanceof Map) return d.has(k) ? d.get(k) : defaultValue;
    if (d != null && typeof d === "object" && typeof d.get === "function") {
        const p = Object.getPrototypeOf(d);
        if (p !== Object.prototype && p !== null) {
            const r = d.get(k);
            return r == null && defaultValue !== undefined ? defaultValue : r;
        }
    }
    return (d != null && Object.prototype.hasOwnProperty.call(d, k)) ? d[k] : defaultValue;
}
"#,
            );
        }
        if needed.contains("pyDictSetdefault") {
            rt.push_str(r#"function pyDictSetdefault(d, k, defaultValue) {
    if (d instanceof Map) { if (d.has(k)) return d.get(k); d.set(k, defaultValue); return defaultValue; }
    if (Object.prototype.hasOwnProperty.call(d, k)) return d[k];
    d[k] = defaultValue;
    return defaultValue;
}
"#);
        }
        if needed.contains("pyDictPopitem") {
            rt.push_str(r#"function pyDictPopitem(d, lastArg) {
    // Drift fix: OrderedDict (and user classes) implement popitem(last=...)
    // themselves — dispatch so `popitem(last=False)` pops from the front,
    // matching the package pyDictPopitem.
    if (d != null && typeof d.popitem === "function") {
        return d.popitem(lastArg === undefined ? true : lastArg);
    }
    if (d instanceof Map) {
        if (d.size === 0) { const e = new Error("popitem(): dictionary is empty"); e.name = "KeyError"; throw e; }
        let last; for (const pair of d.entries()) last = pair;
        d.delete(last[0]);
        return pyTuple(last[0], last[1]);
    }
    const keys = Object.keys(d);
    if (keys.length === 0) { const e = new Error("popitem(): dictionary is empty"); e.name = "KeyError"; throw e; }
    const k = keys[keys.length - 1]; const v = d[k]; delete d[k];
    return pyTuple(k, v);
}
"#);
        }
        if needed.contains("pyPop") {
            // Mirrors runtime/src/runtime.js pyPop (list + dict, both shapes).
            rt.push_str(r#"function pyPop(obj, ...rest) {
    if (obj != null && !Array.isArray(obj) && !(obj instanceof Map) && typeof obj.pop === "function") {
        return obj.pop(...rest);
    }
    if (Array.isArray(obj)) {
        const n = obj.length;
        if (n === 0) { const e = new Error("pop from empty list"); e.name = "IndexError"; throw e; }
        let idx = rest.length === 0 ? -1 : rest[0];
        if (typeof idx === "boolean") idx = idx ? 1 : 0;
        if (typeof idx === "bigint") { if (idx >= -9007199254740991n && idx <= 9007199254740991n) idx = Number(idx); else { const e = new Error("pop index out of range"); e.name = "IndexError"; throw e; } }
        if (idx < 0) idx += n;
        if (idx < 0 || idx >= n) { const e = new Error("pop index out of range"); e.name = "IndexError"; throw e; }
        return obj.splice(idx, 1)[0];
    }
    if (obj instanceof Map) {
        const k = rest[0];
        if (obj.has(k)) { const v = obj.get(k); obj.delete(k); return v; }
        if (rest.length >= 2) return rest[1];
        const e = new Error(JSON.stringify(k)); e.name = "KeyError"; throw e;
    }
    const k = rest[0];
    if (Object.prototype.hasOwnProperty.call(obj, k)) { const v = obj[k]; delete obj[k]; return v; }
    if (rest.length >= 2) return rest[1];
    const e = new Error(JSON.stringify(k)); e.name = "KeyError"; throw e;
}
"#);
        }
        if needed.contains("pyUpdate") {
            rt.push_str(r#"function pyUpdate(obj, ...others) {
    if (obj instanceof Set) { for (const o of others) for (const v of o) obj.add(v); return; }
    // #242: a custom receiver with its own update (Counter adds COUNTS, not
    // overwrites) must win over the generic Map merge below.
    if (obj != null && Object.getPrototypeOf(obj) !== Object.prototype && typeof obj.update === "function") {
        for (const o of others) obj.update(o); return;
    }
    if (obj instanceof Map) {
        for (const o of others) for (const [k, v] of (o instanceof Map ? o.entries() : Object.entries(o))) obj.set(k, v);
        return;
    }
    for (const o of others) {
        if (o instanceof Map) { for (const [k, v] of o.entries()) obj[k] = v; }
        else Object.assign(obj, o);
    }
}
"#);
        }
        if needed.contains("pyClear") {
            rt.push_str(r#"function pyClear(obj) {
    if (Array.isArray(obj)) { obj.length = 0; return; }
    if (obj instanceof Set || obj instanceof Map) { obj.clear(); return; }
    // Drift fix: custom receivers with their own clear (deque, user classes).
    const __plain = obj != null && typeof obj === "object" && (Object.getPrototypeOf(obj) === Object.prototype || Object.getPrototypeOf(obj) === null);
    if (obj != null && !__plain && typeof obj.clear === "function") { obj.clear(); return; }
    if (obj && typeof obj === "object") { for (const k of Object.keys(obj)) delete obj[k]; return; }
    { const e = new Error(`object of type '${typeof obj}' has no clear()`); e.name = "TypeError"; throw e; }
}
"#);
        }
        if needed.contains("pyCopy") {
            rt.push_str(r#"function pyCopy(obj) {
    if (Array.isArray(obj)) return obj.slice();
    if (obj instanceof Set) return new (obj.constructor)(obj);
    // Custom receivers with their own copy (deque, Counter, OrderedDict,
    // defaultdict, user classes) — keeps the subclass type + __missing__/factory,
    // like CPython. Must precede the PyDict/Map fallbacks (#277).
    if (obj != null && typeof obj.copy === "function") return obj.copy();
    if (obj instanceof PyDict) return new PyDict(obj);
    if (obj instanceof Map) return new Map(obj.entries());
    // Drift fix: match the package TypeError guard for non-object receivers
    // (`(5).copy()` etc.) instead of returning a bogus `{}`.
    if (obj && typeof obj === "object") return { ...obj };
    { const e = new Error(`object of type '${typeof obj}' has no copy()`); e.name = "TypeError"; throw e; }
}
"#);
        }
        if needed.contains("pyDictMerge") {
            // Mirrors runtime/src/runtime.js pyDictMerge (#83).
            rt.push_str(r#"function pyDictMerge(...parts) {
    if (parts.some((p) => p instanceof Map)) {
        const out = new PyDict();
        for (const p of parts) {
            if (p == null) continue;
            if (p instanceof Map) { for (const [k, v] of p.entries()) out.set(k, v); }
            else { for (const k of Object.keys(p)) out.set(k, p[k]); }
        }
        return out;
    }
    const out = {};
    for (const p of parts) {
        if (p == null) continue;
        for (const k of Object.keys(p)) {
            if (k === "__proto__") Object.defineProperty(out, k, { value: p[k], writable: true, enumerable: true, configurable: true });
            else out[k] = p[k];
        }
    }
    return out;
}
"#);
        }
        if needed.contains("pyFormatFloat") {
            // Mirrors runtime/src/operators.js's pyFormatFloat exactly —
            // see that copy for the full derivation comment (CPython's
            // decpt<=-4||decpt>16 scientific-notation threshold, reusing
            // toExponential()'s shortest-round-trip digit string).
            rt.push_str(r#"function pyFormatFloat(n) {
    if (typeof n === "bigint") { const f = Number(n); if (!isFinite(f)) { const e = new Error("int too large to convert to float"); e.name = "OverflowError"; throw e; } n = f; }
    if (Number.isNaN(n)) return "nan";
    if (n === Infinity) return "inf";
    if (n === -Infinity) return "-inf";
    const negative = n < 0 || Object.is(n, -0);
    const abs = Math.abs(n);
    if (abs === 0) return negative ? "-0.0" : "0.0";
    const m = /^(\d)(?:\.(\d+))?e([+-]\d+)$/.exec(abs.toExponential());
    const digits = m[1] + (m[2] || "");
    const exponent = parseInt(m[3], 10);
    const decpt = exponent + 1;
    let out;
    if (decpt <= -4 || decpt > 16) {
        const mantissa = digits.length > 1 ? `${digits[0]}.${digits.slice(1)}` : digits;
        const sign = exponent < 0 ? "-" : "+";
        const expDigits = String(Math.abs(exponent)).padStart(2, "0");
        out = `${mantissa}e${sign}${expDigits}`;
    } else if (decpt <= 0) {
        out = `0.${"0".repeat(-decpt)}${digits}`;
    } else if (decpt >= digits.length) {
        out = `${digits}${"0".repeat(decpt - digits.length)}.0`;
    } else {
        out = `${digits.slice(0, decpt)}.${digits.slice(decpt)}`;
    }
    return negative ? `-${out}` : out;
}
"#);
        }
        if needed.contains("pyTuple") {
            rt.push_str(
                r#"function pyTuple(...items) {
    Object.defineProperty(items, "__pytuple__", { value: true, enumerable: false });
    return items;
}
"#,
            );
        }
        if needed.contains("pyRepr") {
            rt.push_str(r#"const __NP_RANGES = [0x0,0x1f,0x7f,0xa0,0xad,0xad,0x600,0x605,0x61c,0x61c,0x6dd,0x6dd,0x70f,0x70f,0x890,0x891,0x8e2,0x8e2,0x1680,0x1680,0x180e,0x180e,0x2000,0x200f,0x2028,0x202f,0x205f,0x2064,0x2066,0x206f,0x3000,0x3000,0xd800,0xf8ff,0xfeff,0xfeff,0xfff9,0xfffb,0x110bd,0x110bd,0x110cd,0x110cd,0x13430,0x1343f,0x1bca0,0x1bca3,0x1d173,0x1d17a,0xe0001,0xe0001,0xe0020,0xe007f,0xf0000,0xffffd,0x100000,0x10fffd];
function __cpNonPrintable(cp) {
    let lo = 0, hi = __NP_RANGES.length / 2 - 1;
    while (lo <= hi) { const mid = (lo + hi) >> 1; const a = __NP_RANGES[mid*2], b = __NP_RANGES[mid*2+1]; if (cp < a) hi = mid-1; else if (cp > b) lo = mid+1; else return true; }
    return false;
}
function pyRepr(obj) {
    if (obj === null || obj === undefined) return "None";
    if (typeof obj === "boolean") return obj ? "True" : "False";
    if (typeof obj === "object" && typeof obj.__repr__ === "function") return obj.__repr__();
    if (typeof obj === "bigint") return obj.toString();
    if (typeof obj === "number") { if (Number.isInteger(obj) && Math.abs(obj) <= Number.MAX_SAFE_INTEGER) return String(obj); return pyFormatFloat(obj); }
    if (typeof obj === "string") {
        let body = "";
        for (const ch of obj) {
            if (ch === "\\") body += "\\\\";
            else if (ch === "\t") body += "\\t";
            else if (ch === "\n") body += "\\n";
            else if (ch === "\r") body += "\\r";
            else {
                const cp = ch.codePointAt(0);
                if (__cpNonPrintable(cp)) body += cp < 0x100 ? "\\x" + cp.toString(16).padStart(2, "0") : cp < 0x10000 ? "\\u" + cp.toString(16).padStart(4, "0") : "\\U" + cp.toString(16).padStart(8, "0");
                else body += ch;
            }
        }
        if (!body.includes("'")) return `'${body}'`;
        if (!body.includes('"')) return `"${body}"`;
        return `'${body.replace(/'/g, "\\'")}'`;
    }
    if (typeof obj.__pytuple__ !== "undefined" || (Array.isArray(obj) && obj.__pytuple__)) {
        const inner = obj.map(pyRepr).join(", ");
        return obj.length === 1 ? `(${inner},)` : `(${inner})`;
    }
    if (Array.isArray(obj)) return `[${obj.map(pyRepr).join(", ")}]`;
    if (obj instanceof Set) { if (obj.size === 0) return "set()"; return `{${[...obj].map(pyRepr).join(", ")}}`; }
    if (obj instanceof Map) { const parts = []; for (const [k, v] of obj.entries()) parts.push(`${pyRepr(k)}: ${pyRepr(v)}`); return `{${parts.join(", ")}}`; }
    if (typeof obj.__repr__ === "function") return obj.__repr__();
    if (obj instanceof Error) {
        // Drift fix: prefer the class's `__name__` (runtime + user exception
        // classes stamp it) and repr each of `args` when present, matching the
        // package pyRepr — so `repr(ValueError('a', 42))` is `ValueError('a', 42)`
        // not `ValueError(('a', 42))`.
        const nm = (obj.constructor && obj.constructor.__name__) || obj.name;
        if (Array.isArray(obj.args)) return `${nm}(${obj.args.map((a) => pyRepr(a)).join(", ")})`;
        const msg = obj.message;
        return `${nm}(${msg != null && msg !== "" ? pyRepr(String(msg)) : ""})`;
    }
    if (typeof obj === "object") { const parts = []; for (const k of Object.keys(obj)) parts.push(`${pyRepr(k)}: ${pyRepr(obj[k])}`); return `{${parts.join(", ")}}`; }
    return String(obj);
}
"#);
        }
        if needed.contains("pyStr") {
            // #97: a user `__str__` is renamed to toString() by the codegen —
            // a non-native toString override is the user's __str__.
            rt.push_str(
                r#"function pyStr(obj) {
    if (typeof obj === "string") return obj;
    if (obj != null && typeof obj.__str__ === "function") return obj.__str__();
    if (obj instanceof Error) return obj.message != null ? String(obj.message) : "";
    if (obj !== null && typeof obj === "object" && !Array.isArray(obj)
        && typeof obj.toString === "function"
        && obj.toString !== Object.prototype.toString) {
        return obj.toString();
    }
    return pyRepr(obj);
}
"#,
            );
        }
        if needed.contains("pyEq") {
            rt.push_str(r#"function pyEq(a, b) {
    if (a === b) return true;
    if (a == null || b == null) return a == b;
    { const __n = (x) => typeof x === "bigint" || typeof x === "number" || typeof x === "boolean"; if (__n(a) && __n(b)) return a == b; }
    if (typeof a.__eq__ === "function") return a.__eq__(b);
    if (typeof b.__eq__ === "function") return b.__eq__(a);
    if (Array.isArray(a) && Array.isArray(b)) {
        if (!!a.__pytuple__ !== !!b.__pytuple__) return false;
        if (a.length !== b.length) return false;
        for (let i = 0; i < a.length; i++) if (!pyEq(a[i], b[i])) return false;
        return true;
    }
    if (a instanceof Set && b instanceof Set) {
        if (a.size !== b.size) return false;
        for (const x of a) if (!b.has(x)) return false;
        return true;
    }
    {
        const __plain = (x) => typeof x === "object" && Object.getPrototypeOf(x) === Object.prototype;
        const aMap = a instanceof Map, bMap = b instanceof Map;
        if ((aMap || __plain(a)) && (bMap || __plain(b))) {
            const aLen = aMap ? a.size : Object.keys(a).length;
            const bLen = bMap ? b.size : Object.keys(b).length;
            if (aLen !== bLen) return false;
            for (const [k, v] of (aMap ? a.entries() : Object.entries(a))) {
                let bHas, bv;
                if (bMap) { bHas = b.has(k); bv = b.get(k); }
                else if (typeof k !== "string") return false;
                else { bHas = Object.prototype.hasOwnProperty.call(b, k); bv = b[k]; }
                if (!bHas || !pyEq(v, bv)) return false;
            }
            return true;
        }
    }
    return a === b;
}
"#);
        }
        if needed.contains("__seqLt") {
            // Drift fix: element comparison routes through pyLt (consults BOTH
            // `a.__lt__` and the reflected `b.__gt__`, and recurses on nested
            // sequences) exactly like the package __seqLt — the old local
            // comparator only checked `x.__lt__`.
            rt.push_str(
                r#"function __seqLt(a, b) {
    const n = Math.min(a.length, b.length);
    for (let i = 0; i < n; i++) {
        if (pyLt(a[i], b[i])) return true;
        if (pyLt(b[i], a[i])) return false;
    }
    return a.length < b.length;
}
"#,
            );
        }
        if needed.contains("pyLt") {
            rt.push_str(
                r#"function pyLt(a, b) {
    if (a != null && typeof a.__lt__ === "function") return a.__lt__(b);
    if (b != null && typeof b.__gt__ === "function") return b.__gt__(a);
    if (Array.isArray(a) && Array.isArray(b)) return __seqLt(a, b);
    return a < b;
}
"#,
            );
        }
        if needed.contains("pyLe") {
            rt.push_str(
                r#"function pyLe(a, b) {
    if (a != null && typeof a.__le__ === "function") return a.__le__(b);
    if (b != null && typeof b.__ge__ === "function") return b.__ge__(a);
    if (Array.isArray(a) && Array.isArray(b)) return !__seqLt(b, a);
    return a <= b;
}
"#,
            );
        }
        if needed.contains("pyGt") {
            rt.push_str(
                r#"function pyGt(a, b) {
    if (a != null && typeof a.__gt__ === "function") return a.__gt__(b);
    if (b != null && typeof b.__lt__ === "function") return b.__lt__(a);
    if (Array.isArray(a) && Array.isArray(b)) return __seqLt(b, a);
    return a > b;
}
"#,
            );
        }
        if needed.contains("pyGe") {
            rt.push_str(
                r#"function pyGe(a, b) {
    if (a != null && typeof a.__ge__ === "function") return a.__ge__(b);
    if (b != null && typeof b.__le__ === "function") return b.__le__(a);
    if (Array.isArray(a) && Array.isArray(b)) return !__seqLt(a, b);
    return a >= b;
}
"#,
            );
        }
        if needed.contains("pyNeg") {
            rt.push_str(
                r#"function pyNeg(a) {
    if (a != null && typeof a.__neg__ === "function") return a.__neg__();
    if (typeof a === "bigint") {
        const __MAX_SAFE = 9007199254740991n;
        const negated = -a;
        return negated >= -__MAX_SAFE && negated <= __MAX_SAFE ? Number(negated) : negated;
    }
    return -a;
}
"#,
            );
        }
        if needed.contains("pyAbs") {
            rt.push_str(
                r#"function pyAbs(x) {
    if (x != null && typeof x.__abs__ === "function") return x.__abs__();
    if (typeof x === "bigint") return x < 0n ? -x : x;
    return Math.abs(x);
}
"#,
            );
        }
        // #283: minimal runtime `complex` type. Construction from real+imag,
        // .real/.imag, + - * (mixed int/float/complex via __toComplex coercion),
        // abs() -> sqrt(re^2+im^2) (a float), == , and a CPython-matching repr.
        // `cmath` and complex `/`, `**` are out of scope. Each dunder returns a
        // fresh PyComplex; pyAdd/pySub/pyMul dispatch here through __add__ etc.
        if needed.contains("pyComplex") {
            rt.push_str(r#"function __complexFmtPart(x, forceSign) {
    let s;
    if (!isFinite(x)) { s = Number.isNaN(x) ? "nan" : (x > 0 ? "inf" : "-inf"); }
    else if (Number.isInteger(x)) { s = (Object.is(x, -0) ? "-0" : String(x)); }
    else { s = pyFormatFloat(x); }
    if (forceSign && s[0] !== "-") s = "+" + s;
    return s;
}
function __complexRepr(re, im) {
    // Pure-imaginary (real is +0.0): print just the imaginary part, no parens.
    if (re === 0 && !Object.is(re, -0)) return __complexFmtPart(im, false) + "j";
    return "(" + __complexFmtPart(re, false) + __complexFmtPart(im, true) + "j)";
}
function __toComplex(o) {
    if (o instanceof PyComplex) return o;
    if (typeof o === "number") return new PyComplex(o, 0);
    if (typeof o === "bigint") return new PyComplex(Number(o), 0);
    if (typeof o === "boolean") return new PyComplex(o ? 1 : 0, 0);
    return null;
}
class PyComplex {
    constructor(re, im) { this.real = re; this.imag = im; }
    __add__(o) { const c = __toComplex(o); return c ? new PyComplex(this.real + c.real, this.imag + c.imag) : undefined; }
    __radd__(o) { const c = __toComplex(o); return c ? new PyComplex(c.real + this.real, c.imag + this.imag) : undefined; }
    __sub__(o) { const c = __toComplex(o); return c ? new PyComplex(this.real - c.real, this.imag - c.imag) : undefined; }
    __rsub__(o) { const c = __toComplex(o); return c ? new PyComplex(c.real - this.real, c.imag - this.imag) : undefined; }
    __mul__(o) { const c = __toComplex(o); return c ? new PyComplex(this.real * c.real - this.imag * c.imag, this.real * c.imag + this.imag * c.real) : undefined; }
    __rmul__(o) { const c = __toComplex(o); return c ? new PyComplex(c.real * this.real - c.imag * this.imag, c.real * this.imag + c.imag * this.real) : undefined; }
    __neg__() { return new PyComplex(-this.real, -this.imag); }
    __pos__() { return new PyComplex(this.real, this.imag); }
    __abs__() { return Math.hypot(this.real, this.imag); }
    __eq__(o) { const c = __toComplex(o); return c ? (this.real === c.real && this.imag === c.imag) : false; }
    __repr__() { return __complexRepr(this.real, this.imag); }
    __str__() { return __complexRepr(this.real, this.imag); }
}
function pyComplex(re, im) { return new PyComplex(re, im); }
"#);
        }
        if needed.contains("pyContains") {
            rt.push_str(r#"function pyContains(container, item) {
    if (container == null) { const e = new Error("argument of type 'NoneType' is not iterable"); e.name = "TypeError"; throw e; }
    if (typeof container.__contains__ === "function") return container.__contains__(item);
    if (typeof container === "string") { if (typeof item !== "string") { const e = new Error("'in <string>' requires string as left operand"); e.name = "TypeError"; throw e; } return container.includes(item); }
    if (Array.isArray(container)) return container.some((x) => pyEq(x, item));
    if (container instanceof Set) {
        if (container.has(item)) return true;
        if (typeof item === "boolean" || typeof item === "number" || typeof item === "bigint") { for (const x of container) if (pyEq(x, item)) return true; }
        return false;
    }
    if (container instanceof Map) return container.has(item);
    if (container != null && typeof container[Symbol.iterator] === "function") {
        for (const x of container) { if (pyEq(x, item)) return true; }
        return false;
    }
    return Object.prototype.hasOwnProperty.call(container, item);
}
"#);
        }
        if needed.contains("pyType") {
            rt.push_str(r#"class __PyTypeObj {
    constructor(name) { this.__name__ = name; }
    __repr__() { return "<class '" + this.__name__ + "'>"; }
    __str__() { return "<class '" + this.__name__ + "'>"; }
}
const __PyInt = new __PyTypeObj("int");
const __PyFloat = new __PyTypeObj("float");
const __PyBool = new __PyTypeObj("bool");
const __PyStr = new __PyTypeObj("str");
const __PyList = new __PyTypeObj("list");
const __PyTuple = new __PyTypeObj("tuple");
const __PySet = new __PyTypeObj("set");
const __PyDict = new __PyTypeObj("dict");
const __PyNoneType = new __PyTypeObj("NoneType");
const __PyTypeMeta = new __PyTypeObj("type");
const __PyFunction = new __PyTypeObj("function");
function pyType(v) {
    if (v === null || v === undefined) return __PyNoneType;
    switch (typeof v) {
        case "boolean": return __PyBool;
        case "number": return Number.isInteger(v) ? __PyInt : __PyFloat;
        case "bigint": return __PyInt;
        case "string": return __PyStr;
        case "function":
            return /^class[\s{]/.test(Function.prototype.toString.call(v)) ? __PyTypeMeta : __PyFunction;
    }
    if (v instanceof __PyTypeObj) return __PyTypeMeta;
    if (Array.isArray(v)) return v.__pytuple__ ? __PyTuple : __PyList;
    if (v instanceof Set) return __PySet;
    if (v instanceof Map) return __PyDict;
    if (v instanceof Error) {
        const ec = v.constructor;
        if (ec && ec !== Error && ec.__name__) return ec;
        let n = v.name;
        if (n === "ReferenceError") { n = /before initialization/.test(v.message || "") ? "UnboundLocalError" : "NameError"; }
        return new __PyTypeObj(n || "Exception");
    }
    const ctor = v.constructor;
    if (ctor && ctor !== Object) return ctor;
    return __PyDict;
}
"#);
        }
        // pyDivmod needs pyFloorDiv/pyMod; pySum needs pyAdd — both live in
        // the arith block.
        if [
            "pyAdd",
            "pySub",
            "pyMul",
            "pyDiv",
            "pyFloorDiv",
            "pyMod",
            "pyPow",
            "pyDivmod",
            "pySum",
        ]
        .iter()
        .any(|h| needed.contains(*h))
        {
            rt.push_str(PY_ARITH_JS);
        }
        if needed.contains("pyPrint") {
            rt.push_str(
                "function pyPrint(...args) {\n    console.log(args.map(pyStr).join(\" \"));\n}\n",
            );
        }
        if needed.contains("pyGetItem") {
            rt.push_str(r#"function pyGetItem(obj, key) {
    if (obj == null) { const e = new Error("'NoneType' object is not subscriptable"); e.name = "TypeError"; throw e; }
    if (typeof obj === "number" || typeof obj === "bigint" || typeof obj === "boolean") { const tn = typeof obj === "boolean" ? "bool" : ((typeof obj === "bigint" || Number.isInteger(obj)) ? "int" : "float"); const e = new Error("'" + tn + "' object is not subscriptable"); e.name = "TypeError"; throw e; }
    if (obj instanceof Set) { const e = new Error("'set' object is not subscriptable"); e.name = "TypeError"; throw e; }
    if (typeof key === "boolean") key = key ? 1 : 0;
    if (typeof key === "bigint" && (typeof obj === "string" || Array.isArray(obj))) {
        if (key >= -9007199254740991n && key <= 9007199254740991n) key = Number(key);
        else { const e = new Error((typeof obj === "string" ? "string" : "list") + " index out of range"); e.name = "IndexError"; throw e; }
    }
    if ((typeof obj === "string" || Array.isArray(obj)) && typeof key === "number" && !Number.isInteger(key)) { const e = new Error((typeof obj === "string" ? "string" : "list") + " indices must be integers"); e.name = "TypeError"; throw e; }
    if (typeof obj === "string") {
        if (typeof key === "string" || (typeof key === "number" && !Number.isInteger(key))) { const e = new Error("string indices must be integers"); e.name = "TypeError"; throw e; }
        if (/[\uD800-\uDBFF]/.test(obj)) {
            const cps = [...obj]; const n = cps.length; let i = key; if (i < 0) i += n;
            if (i < 0 || i >= n) { const e = new Error("string index out of range"); e.name = "IndexError"; throw e; }
            return cps[i];
        }
        const n = obj.length; let i = key; if (i < 0) i += n;
        if (i < 0 || i >= n) { const e = new Error("string index out of range"); e.name = "IndexError"; throw e; }
        return obj[i];
    }
    if (Array.isArray(obj)) {
        if (typeof key === "string" || (typeof key === "number" && !Number.isInteger(key))) { const e = new Error("list indices must be integers or slices, not " + (typeof key === "string" ? "str" : "float")); e.name = "TypeError"; throw e; }
        const n = obj.length; let i = key; if (i < 0) i += n;
        if (i < 0 || i >= n) { const e = new Error("list index out of range"); e.name = "IndexError"; throw e; }
        return obj[i];
    }
    if (obj instanceof Map) {
        if (!obj.has(key)) {
            if (typeof obj.__missing__ === "function") return obj.__missing__(key);
            const e = new Error(typeof key === "string" ? `'${key}'` : String(key)); e.name = "KeyError"; throw e;
        }
        return obj.get(key);
    }
    if (typeof obj.__getitem__ === "function") return obj.__getitem__(key);
    {
        const proto = Object.getPrototypeOf(obj);
        if (proto !== Object.prototype && proto !== null) return obj[key];
    }
    if (!Object.prototype.hasOwnProperty.call(obj, key)) {
        const e = new Error(typeof key === "string" ? `'${key}'` : String(key)); e.name = "KeyError"; throw e;
    }
    return obj[key];
}
"#);
        }
        if needed.contains("pyFloat") {
            rt.push_str(r#"function pyFloat(x) {
    if (typeof x === "boolean") return x ? 1 : 0;
    if (typeof x === "number") return x;
    if (typeof x === "bigint") { const n = Number(x); if (!isFinite(n)) { const e = new Error("int too large to convert to float"); e.name = "OverflowError"; throw e; } return n; }
    if (typeof x === "string") {
        const t = x.trim();
        const m = /^([+-]?)(inf|infinity|nan)$/i.exec(t);
        if (m) { if (m[2].toLowerCase() === "nan") return NaN; return m[1] === "-" ? -Infinity : Infinity; }
        let t2 = t;
        if (t.indexOf("_") !== -1) {
            const isDig = (c) => c >= 48 && c <= 57;
            for (let i = 0; i < t.length; i++) {
                if (t.charCodeAt(i) === 95 && !(isDig(t.charCodeAt(i - 1)) && isDig(t.charCodeAt(i + 1)))) { const e = new Error(`could not convert string to float: '${x}'`); e.name = "ValueError"; throw e; }
            }
            t2 = t.replace(/_/g, "");
        }
        if (t2 === "" || !/^[+-]?(\d+\.?\d*|\.\d+)([eE][+-]?\d+)?$/.test(t2)) { const e = new Error(`could not convert string to float: '${x}'`); e.name = "ValueError"; throw e; }
        return Number(t2);
    }
    return Number(x);
}
"#);
        }
        if needed.contains("pyIter") {
            // Mirrors runtime/src/runtime.js pyIter.
            rt.push_str(r#"function pyIter(obj) {
    if (obj == null) { const e = new Error("'NoneType' object is not iterable"); e.name = "TypeError"; throw e; }
    if (typeof obj[Symbol.iterator] === "function") return obj[Symbol.iterator]();
    if (typeof obj.__iter__ === "function") return obj.__iter__();
    const e = new Error("object is not iterable"); e.name = "TypeError"; throw e;
}
"#);
        }
        if needed.contains("__pyCallKw") || needed.contains("__pyKwArgs") {
            // Mirrors runtime/src/runtime.js __pyKwArgs/__pyCallKw
            // (round-2/-3 kwargs binding).
            rt.push_str(r#"function __pyKwArgs(fn, pos, kw) {
    const entries = kw instanceof Map ? Array.from(kw.entries()) : Object.entries(kw);
    const names = fn ? fn.__pyparams__ : undefined;
    if (!names) { const legacy = {}; for (const [k, v] of entries) legacy[k] = v; return [...pos, legacy]; }
    const fname = (fn && fn.name) || "function";
    const args = pos.slice();
    let rest = null;
    for (const [k, v] of entries) {
        const idx = names.indexOf(k);
        if (idx >= 0) {
            if (idx < pos.length) { const e = new Error(fname + "() got multiple values for argument '" + k + "'"); e.name = "TypeError"; throw e; }
            args[idx] = v;
        } else if (fn.__pykw__) { (rest = rest || {})[k] = v; }
        else { const e = new Error(fname + "() got an unexpected keyword argument '" + k + "'"); e.name = "TypeError"; throw e; }
    }
    if (fn.__pykw__) args[names.length] = rest || {};
    return args;
}
function __pyCallKw(fn, pos, kw) { return fn(...__pyKwArgs(fn, pos, kw)); }
"#);
        }
        if needed.contains("pyNext") {
            rt.push_str(r#"function pyNext(it, ...rest) {
    if (it == null || typeof it.next !== "function") { const e = new Error("object is not an iterator"); e.name = "TypeError"; throw e; }
    const r = it.next();
    if (r.done) { if (rest.length >= 1) return rest[0]; const e = new Error(); e.name = "StopIteration"; e.value = r.value === undefined ? null : r.value; const __a = r.value === undefined ? [] : [r.value]; Object.defineProperty(__a, "__pytuple__", { value: true, enumerable: false }); e.args = __a; throw e; }
    return r.value;
}
"#);
        }
        if needed.contains("pyInt") {
            // Mirrors runtime/src/operators.js pyInt (#82).
            rt.push_str(r#"function pyInt(x, base) {
    if (typeof base === "object" && base !== null) base = base.base;
    if (typeof x === "boolean") return x ? 1 : 0;
    if (typeof x === "bigint") return x;
    if (typeof x === "number") {
        if (Number.isNaN(x)) { const e = new Error("cannot convert float NaN to integer"); e.name = "ValueError"; throw e; }
        if (!Number.isFinite(x)) { const e = new Error("cannot convert float infinity to integer"); e.name = "OverflowError"; throw e; }
        return Math.trunc(x);
    }
    if (typeof x === "string") {
        const b = base == null ? 10 : Number(base);
        const t = x.trim();
        const m = /^([+-]?)([0-9a-zA-Z_]+)$/.exec(t);
        const bad = () => { const e = new Error(`invalid literal for int() with base ${b}: '${x}'`); e.name = "ValueError"; return e; };
        if (!m) throw bad();
        const body = m[2];
        if (/^_|_$|__/.test(body)) throw bad();
        const digits = body.replace(/_/g, "");
        const digitRe = b === 10 ? /^[0-9]+$/ : b === 16 ? /^[0-9a-fA-F]+$/ : b === 8 ? /^[0-7]+$/ : b === 2 ? /^[01]+$/ : null;
        if (digitRe) {
            if (!digitRe.test(digits)) throw bad();
            const prefix = b === 16 ? "0x" : b === 8 ? "0o" : b === 2 ? "0b" : "";
            const big = BigInt((m[1] === "-" ? "-" : "") + prefix + digits);
            return big >= -9007199254740991n && big <= 9007199254740991n ? Number(big) : big;
        }
        const v = parseInt((m[1] === "-" ? "-" : "") + digits, b);
        if (Number.isNaN(v) || digits.split("").some(d => Number.isNaN(parseInt(d, b)))) throw bad();
        return v;
    }
    if (x != null && typeof x.__int__ === "function") return x.__int__();
    return Math.trunc(Number(x));
}
"#);
        }
        if needed.contains("pyDelItem") {
            // Mirrors runtime/src/runtime.js pyDelItem (#101).
            rt.push_str(r#"function pyDelItem(obj, key) {
    if (obj == null) { const e = new Error("'NoneType' object does not support item deletion"); e.name = "TypeError"; throw e; }
    if (Array.isArray(obj)) {
        const n = obj.length;
        let i = typeof key === "bigint" ? Number(key) : key;
        if (i < 0) i += n;
        if (!Number.isInteger(i) || i < 0 || i >= n) { const e = new Error("list assignment index out of range"); e.name = "IndexError"; throw e; }
        obj.splice(i, 1);
        return;
    }
    if (obj instanceof Map) {
        if (!obj.delete(key)) { const e = new Error(typeof key === "string" ? `'${key}'` : String(key)); e.name = "KeyError"; throw e; }
        return;
    }
    if (typeof obj.__delitem__ === "function") { obj.__delitem__(key); return; }
    if (!Object.prototype.hasOwnProperty.call(obj, key)) { const e = new Error(typeof key === "string" ? `'${key}'` : String(key)); e.name = "KeyError"; throw e; }
    delete obj[key];
}
"#);
        }
        if needed.contains("pyChr") {
            rt.push_str(r#"function pyChr(n) {
    const i = typeof n === "bigint" ? Number(n) : Math.trunc(Number(n));
    if (!Number.isFinite(i) || i < 0 || i >= 0x110000) { const e = new Error("chr() arg not in range(0x110000)"); e.name = "ValueError"; throw e; }
    return String.fromCodePoint(i);
}
"#);
        }
        if needed.contains("pyOrd") {
            rt.push_str(r#"function pyOrd(s) {
    if (typeof s !== "string") { const e = new Error("ord() expected string of length 1"); e.name = "TypeError"; throw e; }
    const cps = [...s];
    if (cps.length !== 1) { const e = new Error(`ord() expected a character, but string of length ${cps.length} found`); e.name = "TypeError"; throw e; }
    return cps[0].codePointAt(0);
}
"#);
        }
        if needed.contains("pyDivmod") {
            // Mirrors runtime/src/operators.js pyDivmod (#90). pyFloorDiv/
            // pyMod come from PY_ARITH_JS (forced by the closure expansion
            // above); pyTuple likewise.
            rt.push_str(r#"function pyDivmod(a, b) {
    const __f = (x) => typeof x === "number" && !Number.isInteger(x);
    if ((__f(a) || __f(b)) && Number(b) === 0) { const e = new Error("float divmod()"); e.name = "ZeroDivisionError"; throw e; }
    return pyTuple(pyFloorDiv(a, b), pyMod(a, b));
}
"#);
        }
        if needed.contains("pySum") {
            // Mirrors runtime/src/operators.js pySum (#94).
            rt.push_str(
                r#"function pySum(iterable, startArg) {
    let start = 0;
    if (startArg !== undefined) {
        if (startArg !== null && typeof startArg === "object"
            && Object.getPrototypeOf(startArg) === Object.prototype
            && Object.prototype.hasOwnProperty.call(startArg, "start")) {
            start = startArg.start;
        } else {
            start = startArg;
        }
    }
    let acc = start;
    for (const item of iterable) acc = pyAdd(acc, item);
    return acc;
}
"#,
            );
        }
        if needed.contains("pyMin") || needed.contains("pyMax") {
            // Mirrors runtime/src/runtime.js pyMin/pyMax (#88).
            rt.push_str(r#"function __pyMinmax(name, wantGreater, args) {
    let key = null, dflt, hasDefault = false;
    if (args.length > 1) {
        const last = args[args.length - 1];
        if (last !== null && typeof last === "object"
            && Object.getPrototypeOf(last) === Object.prototype
            && (Object.prototype.hasOwnProperty.call(last, "key")
                || Object.prototype.hasOwnProperty.call(last, "default"))) {
            if (last.key != null) key = last.key;
            if (Object.prototype.hasOwnProperty.call(last, "default")) { dflt = last.default; hasDefault = true; }
            args = args.slice(0, -1);
        }
    }
    const items = args.length === 1 ? [...pyForIter(args[0])] : args;
    if (items.length === 0) {
        if (hasDefault) return dflt;
        const e = new Error(`${name}() iterable argument is empty`); e.name = "ValueError"; throw e;
    }
    let best = items[0];
    let bestKey = key ? key(best) : best;
    for (let i = 1; i < items.length; i++) {
        const k = key ? key(items[i]) : items[i];
        const better = wantGreater
            ? (typeof k?.__gt__ === "function" ? k.__gt__(bestKey) : k > bestKey)
            : (typeof k?.__lt__ === "function" ? k.__lt__(bestKey) : k < bestKey);
        if (better) { best = items[i]; bestKey = k; }
    }
    return best;
}
function pyMin(...args) { return __pyMinmax("min", false, args); }
function pyMax(...args) { return __pyMinmax("max", true, args); }
"#);
        }
        if needed.contains("pyBitOr")
            || needed.contains("pyBitAnd")
            || needed.contains("pyBitXor")
            || needed.contains("pyBitNot")
        {
            // Mirrors runtime/src/operators.js pyBitOr/pyBitAnd/pyBitXor/
            // pyBitNot (#93; wave-14 unary `~`). Self-contained set logic (no
            // pySetUnion import in the inline runtime). Shared numeric helpers
            // first; the binary trio and unary pyBitNot are gated separately so
            // a `~`-only program doesn't pull in pyBitOr's pyDictMerge
            // reference (which is only auto-needed for pyBitOr, see the #83
            // dependency chain above).
            rt.push_str(r#"const __fits32 = (x) => typeof x === "number" && Number.isInteger(x) && Math.abs(x) <= 0x7fffffff;
const __bigNorm = (b) => (b >= -9007199254740991n && b <= 9007199254740991n ? Number(b) : b);
const __asBig = (x) => (typeof x === "bigint" ? x : BigInt(x));
const __bitIntOk = (x) => typeof x === "bigint" || typeof x === "boolean" || (typeof x === "number" && Number.isInteger(x));
// NOTE: named __bitTypeName (NOT __opTypeName) on purpose — PY_ARITH_JS emits
// its own top-level `__opTypeName`, and both blocks can be emitted for the same
// program (arithmetic + bit-ops), so a shared name would double-declare. Drift
// fix: the set/dict arms below were missing (reported 'object' where the package
// __opTypeName reports 'set'/'dict').
function __bitTypeName(v) {
    if (v === null || v === undefined) return "NoneType";
    if (typeof v === "boolean") return "bool";
    if (typeof v === "number") return Number.isInteger(v) ? "int" : "float";
    if (typeof v === "bigint") return "int";
    if (typeof v === "string") return "str";
    if (Array.isArray(v)) return v.__pytuple__ ? "tuple" : "list";
    if (v instanceof Set) return "set";
    if (v instanceof Map) return "dict";
    const c = v && v.constructor;
    return (c && (c.__name__ || c.name)) || "object";
}
"#);
            if needed.contains("pyBitOr")
                || needed.contains("pyBitAnd")
                || needed.contains("pyBitXor")
            {
                rt.push_str(r#"function __reqBitInt(op, a, b, fctx) {
    fctx = fctx || 0;
    if (fctx || !__bitIntOk(a) || !__bitIntOk(b)) { const an = (fctx & 1) ? "float" : __bitTypeName(a); const bn = (fctx & 2) ? "float" : __bitTypeName(b); const e = new Error(`unsupported operand type(s) for ${op}: '${an}' and '${bn}'`); e.name = "TypeError"; throw e; }
}
function pyBitOr(a, b, fctx) {
    if (a instanceof Set && b instanceof Set) { const out = new (a.constructor)(a); for (const v of b) out.add(v); return out; }
    if (a != null && typeof a.__or__ === "function") return a.__or__(b);
    const __plainD = (x) => x !== null && typeof x === "object" && Object.getPrototypeOf(x) === Object.prototype;
    if ((a instanceof Map || __plainD(a)) && (b instanceof Map || __plainD(b))) return pyDictMerge(a, b);
    __reqBitInt("|", a, b, fctx);
    if (__fits32(a) && __fits32(b)) return a | b;
    return __bigNorm(__asBig(a) | __asBig(b));
}
function pyBitAnd(a, b, fctx) {
    if (a instanceof Set && b instanceof Set) {
        const [small, big] = b.size > a.size ? [a, b] : [b, a];
        const out = new (a.constructor)();
        for (const v of small) if (big.has(v)) out.add(v);
        return out;
    }
    if (a != null && typeof a.__and__ === "function") return a.__and__(b);
    __reqBitInt("&", a, b, fctx);
    if (__fits32(a) && __fits32(b)) return a & b;
    return __bigNorm(__asBig(a) & __asBig(b));
}
function pyBitXor(a, b, fctx) {
    if (a instanceof Set && b instanceof Set) {
        const out = new (a.constructor)();
        for (const v of a) if (!b.has(v)) out.add(v);
        for (const v of b) if (!a.has(v)) out.add(v);
        return out;
    }
    if (a != null && typeof a.__xor__ === "function") return a.__xor__(b);
    __reqBitInt("^", a, b, fctx);
    if (__fits32(a) && __fits32(b)) return a ^ b;
    return __bigNorm(__asBig(a) ^ __asBig(b));
}
"#);
            }
            if needed.contains("pyBitNot") {
                // Mirrors runtime/src/operators.js pyBitNot (wave-14 F9):
                // Python unary `~x = -x - 1`, arbitrary precision — raw JS `~`
                // does ToInt32 (`~(2**40)` → -1, not -1099511627777). CPython's
                // unary TypeError message shape; fctx truthy = operand
                // statically float (#343 discipline, unary form).
                rt.push_str(r#"function pyBitNot(a, fctx) {
    if (a != null && typeof a.__invert__ === "function") return a.__invert__();
    if (fctx || !__bitIntOk(a)) { const an = fctx ? "float" : __bitTypeName(a); const e = new Error(`bad operand type for unary ~: '${an}'`); e.name = "TypeError"; throw e; }
    if (__fits32(a)) return ~a;
    return __bigNorm(-__asBig(a) - 1n);
}
"#);
            }
        }
        if needed.contains("__fixedHalfEven") {
            // Mirrors runtime/src/runtime.js __fixedHalfEven (#86) —
            // shared by pyFixed and pyFormatSpec (#129).
            rt.push_str(
                r#"function __fixedHalfEven(x, prec) {
    const p = prec > 0 ? prec : 0;
    if (x === 0) return p > 0 ? "0." + "0".repeat(p) : "0";
    const dv = new DataView(new ArrayBuffer(8));
    dv.setFloat64(0, x);
    const hi = dv.getUint32(0), lo = dv.getUint32(4);
    const expBits = (hi >>> 20) & 0x7ff;
    let mant = (BigInt(hi & 0xfffff) << 32n) | BigInt(lo);
    let e2;
    if (expBits === 0) { e2 = -1074n; } else { mant |= 1n << 52n; e2 = BigInt(expBits) - 1075n; }
    const scaled = mant * 10n ** BigInt(p);
    let q;
    if (e2 >= 0n) { q = scaled << e2; }
    else {
        const shift = -e2;
        q = scaled >> shift;
        const r = scaled & ((1n << shift) - 1n);
        const half = 1n << (shift - 1n);
        if (r > half || (r === half && (q & 1n) === 1n)) q += 1n;
    }
    let s = q.toString();
    if (p === 0) return s;
    if (s.length <= p) s = "0".repeat(p - s.length + 1) + s;
    return s.slice(0, s.length - p) + "." + s.slice(s.length - p);
}
"#,
            );
        }

        if needed.contains("pyTupleOf") {
            // Mirrors runtime/src/operators.js pyTupleOf (#110, #348 overflow).
            rt.push_str(r#"function pyTupleOf(iterable) {
    if (iterable === undefined) return pyTuple();
    let items;
    if (Array.isArray(iterable)) {
        items = iterable.slice();
    } else if (iterable !== null && typeof iterable === "object" && typeof iterable[Symbol.iterator] !== "function" && Object.getPrototypeOf(iterable) === Object.prototype) {
        items = Object.keys(iterable);
    } else {
        items = [...iterable];
    }
    Object.defineProperty(items, "__pytuple__", { value: true, enumerable: false });
    return items;
}
"#);
        }

        if needed.contains("pyFixed") {
            // Mirrors runtime/src/runtime.js pyFixed (#86).
            rt.push_str(r#"function pyFixed(x, prec) {
    if (typeof x === "string") { const e = new Error("Unknown format code 'f' for object of type 'str'"); e.name = "ValueError"; throw e; }
    const n = Number(x);
    if (Number.isNaN(n)) return "nan";
    if (n === Infinity) return "inf";
    if (n === -Infinity) return "-inf";
    const neg = n < 0 || Object.is(n, -0);
    const body = __fixedHalfEven(Math.abs(n), Math.trunc(prec));
    return neg ? "-" + body : body;
}
"#);
        }

        if needed.contains("pyFormatSpec") {
            // Mirrors runtime/src/runtime.js pyFormatSpec (#129).
            rt.push_str(r##"function pyFormatSpec(value, opts, isFloat) {
    opts = opts || {};
    const ty = opts.type;
    if (typeof value === "string" && ty != null && ty !== "s") {
        const e = new Error(`Unknown format code '${ty}' for object of type 'str'`);
        e.name = "ValueError";
        throw e;
    }
    let s;
    let isNumeric = false;
    let neg = false;

    // Group a digit string from the right (CPython: `,`/`_` every 3 for
    // decimal, `_` every 4 for b/o/x/X). Digit-agnostic (hex letters).
    const group = (str, size, sep) => {
        let out = "";
        for (let i = str.length; i > 0; i -= size) {
            const chunk = str.slice(Math.max(0, i - size), i);
            out = out ? chunk + sep + out : chunk;
        }
        return out;
    };

    if (ty === "s" || ty === undefined && typeof value === "string") {
        s = String(value);
        if (opts.precision != null) s = s.slice(0, opts.precision);
    } else if (ty === "b" || ty === "o" || ty === "x" || ty === "X" || ty === "d" || ty === "n" || ty === "c"
        || (ty === undefined && typeof value === "bigint")) {
        isNumeric = true;
        if (ty === "c") {
            s = String.fromCodePoint(Number(value));
        } else {
            // Keep BigInt ints exact (arbitrary precision) — never round
            // through Number.
            let n = typeof value === "bigint" ? value : Math.trunc(Number(value));
            neg = n < 0;
            if (neg) n = -n;
            const radix = ty === "b" ? 2 : ty === "o" ? 8 : (ty === "x" || ty === "X") ? 16 : 10;
            s = n.toString(radix);
            if (ty === "X") s = s.toUpperCase();
            // Grouping applies to the digits only; the #-prefix goes
            // OUTSIDE the grouped digits (0b1010_1010).
            if (opts.grouping) s = group(s, radix === 10 ? 3 : 4, opts.grouping);
            if (opts.alt) {
                if (radix === 2) s = "0b" + s;
                else if (radix === 8) s = "0o" + s;
                else if (radix === 16) s = (ty === "X" ? "0X" : "0x") + s;
            }
        }
    } else if (ty === "e" || ty === "E" || ty === "f" || ty === "F" || ty === "g" || ty === "G" || ty === "%" || ty === undefined) {
        isNumeric = true;
        let n = Number(value);
        if (ty === "%") n = n * 100;
        neg = n < 0 || Object.is(n, -0);
        n = Math.abs(n);
        const prec = opts.precision != null ? opts.precision : 6;
        if (ty === "e" || ty === "E") {
            s = n.toExponential(prec);
            // CPython zero-pads the exponent to at least 2 digits
            // (e+03, e-04). JS toExponential produces e+3 / e-4. Patch
            // by normalizing the trailing exponent.
            s = s.replace(/e([+-])(\d)$/, "e$10$2");
            if (ty === "E") s = s.toUpperCase();
        } else if (ty === "g" || ty === "G") {
            // CPython 'g': with precision p (default 6; 0 → 1), let exp be
            // the decimal exponent of the value rounded to p significant
            // digits. If -4 <= exp < p → fixed notation, else scientific;
            // trailing zeros stripped (unless '#'), exponent >= 2 digits.
            let p = prec;
            if (p === 0) p = 1;
            if (n === 0) {
                s = "0";
            } else if (!Number.isFinite(n)) {
                s = n === Infinity ? "inf" : "nan";
            } else {
                const m = /^(\d)(?:\.(\d+))?e([+-]\d+)$/.exec(n.toExponential(p - 1));
                const digits = m[1] + (m[2] || "");
                const exp10 = parseInt(m[3], 10);
                if (exp10 >= -4 && exp10 < p) {
                    if (exp10 >= 0) {
                        s = digits.length <= exp10 + 1
                            ? digits + "0".repeat(exp10 + 1 - digits.length)
                            : digits.slice(0, exp10 + 1) + "." + digits.slice(exp10 + 1);
                    } else {
                        s = "0." + "0".repeat(-exp10 - 1) + digits;
                    }
                    if (!opts.alt && s.includes(".")) s = s.replace(/\.?0+$/, "");
                } else {
                    let mant = opts.alt ? digits : digits.replace(/0+$/, "") || "0";
                    const mantStr = mant.length > 1 ? mant[0] + "." + mant.slice(1) : mant;
                    s = mantStr + "e" + (exp10 < 0 ? "-" : "+") + String(Math.abs(exp10)).padStart(2, "0");
                }
            }
            if (ty === "G") s = s.toUpperCase();
        } else if (ty === "%") {
            // Round-half-even on the exact double, like CPython (#86).
            s = __fixedHalfEven(n, opts.precision != null ? opts.precision : 6) + "%";
        } else if (ty === "f" || ty === "F" || opts.precision != null) {
            s = __fixedHalfEven(n, prec);
        } else if (isFloat) {
            s = pyFormatFloat(n);
        } else {
            s = String(n);
        }
        if ((ty === "f" || ty === "F" || ty === undefined) && opts.grouping) {
            // Insert separators in the integer part only.
            const dot = s.indexOf(".");
            const intPart = dot === -1 ? s : s.slice(0, dot);
            const fracPart = dot === -1 ? "" : s.slice(dot);
            s = group(intPart, 3, opts.grouping) + fracPart;
        }
    } else {
        s = String(value);
    }

    // Sign handling for numeric values
    let signStr = "";
    if (isNumeric) {
        if (neg) signStr = "-";
        else if (opts.sign === "+") signStr = "+";
        else if (opts.sign === " ") signStr = " ";
    }

    // Width / fill / align
    const width = opts.width || 0;
    if (width > 0) {
        const fill = opts.fill || (opts.zero && isNumeric ? "0" : " ");
        const align = opts.align || (opts.zero && isNumeric ? "=" : (isNumeric ? ">" : "<"));
        const total = signStr.length + s.length;
        if (total < width) {
            const need = width - total;
            if (align === "<") return signStr + s + fill.repeat(need);
            if (align === ">") return fill.repeat(need) + signStr + s;
            if (align === "^") {
                const left = Math.floor(need / 2);
                return fill.repeat(left) + signStr + s + fill.repeat(need - left);
            }
            if (align === "=") return signStr + fill.repeat(need) + s;
        }
    }
    return signStr + s;
}
"##);
        }

        if needed.contains("pyFormatDynamic") {
            // Mirrors runtime/src/runtime.js parseFormatSpec +
            // pyFormatDynamic (#108/#129).
            rt.push_str(r##"function parseFormatSpec(s) {
    const opts = {};
    const chars = [...s];
    let i = 0;
    if (chars.length >= 2 && "<>=^".includes(chars[1])) {
        opts.fill = chars[0]; opts.align = chars[1]; i = 2;
    } else if (chars.length >= 1 && "<>=^".includes(chars[0])) {
        opts.align = chars[0]; i = 1;
    }
    if (i < chars.length && "+- ".includes(chars[i])) { opts.sign = chars[i]; i++; }
    if (i < chars.length && chars[i] === "#") { opts.alt = true; i++; }
    if (i < chars.length && chars[i] === "0") { opts.zero = true; i++; }
    let w = "";
    while (i < chars.length && /[0-9]/.test(chars[i])) { w += chars[i]; i++; }
    if (w) opts.width = parseInt(w, 10);
    if (i < chars.length && (chars[i] === "," || chars[i] === "_")) { opts.grouping = chars[i]; i++; }
    if (i < chars.length && chars[i] === ".") {
        i++;
        let p = "";
        while (i < chars.length && /[0-9]/.test(chars[i])) { p += chars[i]; i++; }
        if (p) opts.precision = parseInt(p, 10);
    }
    if (i < chars.length) { opts.type = chars[i]; i++; }
    return opts;
}
function pyFormatDynamic(value, specStr) {
    return pyFormatSpec(value, parseFormatSpec(String(specStr)));
}
"##);
        }

        if needed.contains("__pyClass")
            || needed.contains("__pySuper")
            || needed.contains("PyObject")
            || needed.contains("__pyIsInstance")
            || needed.contains("__pyClassAttr")
            || needed.contains("__pyClassCall")
        {
            rt.push_str(PY_OBJECT_MODEL_JS);
        }

        // #170: fallback for helpers with no hand-written inline entry above
        // (pyStr* method helpers, __pyAsyncIter, and anything added to the
        // package runtime later). Extract their definitions — with their
        // transitive top-level dependencies — from the CANONICAL package
        // runtime sources embedded at build time, so `pyths run` can never
        // again lag behind `pyths compile`'s imported runtime.
        Self::append_extracted_helpers(needed, &mut rt);

        rt.push_str("// --- End Runtime ---\n");
        rt
    }

    /// Package runtime sources embedded at build time (single source of truth
    /// for the #170 fallback). Order matters: first definition of a name wins,
    /// and runtime.js is preferred over operators.js.
    const PKG_RUNTIME_SOURCES: [&'static str; 2] = [
        include_str!("../../../runtime/src/runtime.js"),
        include_str!("../../../runtime/src/operators.js"),
    ];

    /// Split the embedded package-runtime sources into named top-level slices.
    /// A slice starts at a column-0 declaration (`[export] [async] function/
    /// class/const/let NAME`) and runs to the next one — line-based on
    /// purpose, so string/template/regex contents can't confuse it the way a
    /// brace scanner would. Returns slices in source order plus a name index.
    fn package_runtime_slices() -> (Vec<(String, String)>, HashMap<String, usize>) {
        fn decl_name(line: &str) -> Option<String> {
            let mut rest = line;
            if let Some(r) = rest.strip_prefix("export ") {
                rest = r;
            }
            if let Some(r) = rest.strip_prefix("async ") {
                rest = r;
            }
            for kw in ["function*", "function", "class", "const", "let"] {
                if let Some(r) = rest.strip_prefix(kw) {
                    let r = r.trim_start();
                    let name: String = r
                        .chars()
                        .take_while(|c| c.is_ascii_alphanumeric() || *c == '_' || *c == '$')
                        .collect();
                    return if name.is_empty() { None } else { Some(name) };
                }
            }
            None
        }

        let mut slices: Vec<(String, String)> = Vec::new();
        let mut by_name: HashMap<String, usize> = HashMap::new();
        for src in Self::PKG_RUNTIME_SOURCES {
            let lines: Vec<&str> = src.lines().collect();
            let mut starts: Vec<(usize, String)> = Vec::new();
            for (i, l) in lines.iter().enumerate() {
                if l.starts_with("import ") || l.starts_with("export {") {
                    continue;
                }
                if let Some(name) = decl_name(l) {
                    starts.push((i, name));
                }
            }
            for (j, (ln, name)) in starts.iter().enumerate() {
                let end = starts.get(j + 1).map_or(lines.len(), |(n, _)| *n);
                let mut text = lines[*ln..end].join("\n");
                text.truncate(text.trim_end().len());
                text.push('\n');
                let text = match text.strip_prefix("export ") {
                    Some(t) => t.to_string(),
                    None => text,
                };
                if !by_name.contains_key(name) {
                    by_name.insert(name.clone(), slices.len());
                    slices.push((name.clone(), text));
                }
            }
        }
        (slices, by_name)
    }

    /// True iff `rt` already carries a top-level definition of `name`
    /// (hand-written inline entries emit plain declarations).
    fn inline_defines(rt: &str, name: &str) -> bool {
        [
            format!("function {}(", name),
            format!("function {} (", name),
            format!("function* {}(", name),
            format!("class {} ", name),
            format!("class {}{{", name),
            format!("class {} {{", name),
            format!("const {} ", name),
            format!("const {}=", name),
            format!("const {} =", name),
        ]
        .iter()
        .any(|p| rt.contains(p.as_str()))
    }

    /// Append every `needed` helper that the hand-written table above did not
    /// define, extracted from the package runtime with its transitive
    /// top-level dependencies, in original source order (so any module-init
    /// ordering in the package runtime is preserved).
    fn append_extracted_helpers(needed: &HashSet<String>, rt: &mut String) {
        let (slices, by_name) = Self::package_runtime_slices();

        let mut work: Vec<String> = needed
            .iter()
            .filter(|n| !Self::inline_defines(rt, n) && by_name.contains_key(*n))
            .cloned()
            .collect();
        if work.is_empty() {
            return;
        }

        let mut chosen: HashSet<usize> = HashSet::new();
        while let Some(name) = work.pop() {
            let Some(&idx) = by_name.get(&name) else {
                continue;
            };
            if !chosen.insert(idx) {
                continue;
            }
            // Scan the slice for identifiers that are themselves package
            // top-level declarations and not already inline-defined.
            let text = &slices[idx].1;
            let mut ident = String::new();
            for c in text.chars().chain(std::iter::once(' ')) {
                if c.is_ascii_alphanumeric() || c == '_' || c == '$' {
                    ident.push(c);
                    continue;
                }
                if !ident.is_empty() {
                    if ident != name
                        && by_name.contains_key(&ident)
                        && !Self::inline_defines(rt, &ident)
                        && !chosen.contains(&by_name[&ident])
                    {
                        work.push(ident.clone());
                    }
                    ident.clear();
                }
            }
        }

        let mut order: Vec<usize> = chosen.into_iter().collect();
        order.sort_unstable();
        rt.push_str("// --- Extracted from package runtime (#170 fallback) ---\n");
        for idx in order {
            rt.push_str(&slices[idx].1);
            rt.push('\n');
        }
    }

    /// Enable source map generation.
    pub fn enable_sourcemap(&mut self, source: &str, source_file: &str, output_file: &str) {
        self.source_text = Some(source.to_string());
        self.sourcemap = Some(SourceMapBuilder::new(source_file, output_file));
    }

    /// A17: omit `sourcesContent` from the emitted map (production hygiene).
    pub fn set_omit_sources_content(&mut self, omit: bool) {
        self.omit_sources_content = omit;
    }

    /// Finish and return both JS and source map. The map is finalized AFTER the
    /// final layout is known: `finish_certified` reports where the body lands
    /// (`body_shift`/`directive_len` in BYTES), from which we derive the LINE
    /// shift the map needs so DevTools resolves frames to the right `.ps` line
    /// (DX-3). We also inline the source (DX-2) and pass the final JS so `names`
    /// is populated for preserved identifiers only (DX-1).
    pub fn finish_with_sourcemap(mut self) -> (String, String) {
        let mut sm = self.sourcemap.take();
        let src_text = self.source_text.clone();
        // A17: capture before `finish_certified` consumes `self`.
        let omit_sources_content = self.omit_sources_content;
        let (js, body_shift, directive_len) = self.finish_certified();
        let map = if let Some(mut sm) = sm.take() {
            // BYTE offset where the post-directive body starts in the final js.
            let body_start_byte = body_shift + directive_len;
            // The prelude occupies whole lines; the map shifts down by their
            // count. Directive lines (kept at the very top) are the shift floor.
            let body_start_line = js[..body_start_byte.min(js.len())].matches('\n').count() as u32;
            let directive_lines = js[..directive_len.min(js.len())].matches('\n').count() as u32;
            sm.set_line_shift(body_start_line - directive_lines, directive_lines);
            // A17: skip inlining original source when opted out.
            if let Some(t) = src_text {
                if !omit_sources_content {
                    sm.set_source_content(t);
                }
            }
            sm.set_generated_js(js.clone());
            sm.to_json()
        } else {
            String::new()
        };
        (js, map)
    }

    /// Record a source map mapping from the current output position to an original byte offset.
    fn mark_mapping(&mut self, orig_byte_offset: usize) {
        if let (Some(sm), Some(src)) = (&mut self.sourcemap, &self.source_text) {
            let (orig_line, orig_col) = sourcemap::byte_offset_to_line_col(src, orig_byte_offset);
            sm.add_mapping(self.out_line, self.out_col, orig_line, orig_col);
        }
    }

    fn write(&mut self, s: &str) {
        // Track output line/column for source maps
        for ch in s.chars() {
            if ch == '\n' {
                self.out_line += 1;
                self.out_col = 0;
            } else {
                self.out_col += 1;
            }
        }
        self.output.push_str(s);
    }

    fn writeln(&mut self, s: &str) {
        self.write_indent();
        self.output.push_str(s);
        // Track the content portion
        for ch in s.chars() {
            if ch == '\n' {
                self.out_line += 1;
                self.out_col = 0;
            } else {
                self.out_col += 1;
            }
        }
        self.output.push('\n');
        self.out_line += 1;
        self.out_col = 0;
    }

    fn write_indent(&mut self) {
        for _ in 0..self.indent {
            self.output.push_str("    ");
            self.out_col += 4;
        }
    }

    fn need_runtime(&mut self, name: &str) {
        self.runtime_imports.insert(name.to_string());
    }

    /// Drain the diagnostics emitted during codegen. Call after
    /// `emit_module` completes; a non-empty result means the compile
    /// should fail. The CLI is responsible for surfacing the messages.
    pub fn take_errors(&mut self) -> Vec<String> {
        std::mem::take(&mut self.codegen_errors)
    }

    /// Drain the subscript-routing certificate recorded during
    /// `emit_module` (credible compilation, §7.2). Validate with
    /// [`crate::cert::check_certificate`] against the emitted JS.
    pub fn take_certificate(&mut self) -> crate::cert::Certificate {
        std::mem::take(&mut self.certificate)
    }

    /// #122: if `value` is a lambda with >= 1 self-defaulted param
    /// (`i=i`), emit `((i, ...) => (rest...) => body)(i, ...)` — the
    /// self-defaulted params become creation-time captures (outer arrow
    /// params shadow the enclosing bindings, so the body needs no
    /// renaming); the remaining params keep their positions/defaults.
    /// Returns false (nothing emitted) when the pattern doesn't apply.
    fn try_emit_capture_lambda(&mut self, value: &Expr) -> bool {
        let ExprKind::Lambda { params, body } = &value.kind else {
            return false;
        };
        let is_self_default = |p: &Param| -> bool {
            matches!(&p.default, Some(d) if matches!(&d.kind, ExprKind::Name(n) if *n == p.name))
        };
        if !params.iter().any(is_self_default) {
            return false;
        }
        let (captured, rest): (Vec<&Param>, Vec<&Param>) =
            params.iter().partition(|p| is_self_default(p));
        let names: Vec<String> = captured
            .iter()
            .map(|p| Self::sanitize_ident(&p.name).into_owned())
            .collect();
        self.write(&format!("(({}) => ", names.join(", ")));
        let inner = Expr {
            kind: ExprKind::Lambda {
                params: rest.into_iter().cloned().collect(),
                body: body.clone(),
            },
            span: value.span,
        };
        self.emit_expr(&inner);
        self.write(&format!(")({})", names.join(", ")));
        true
    }

    /// #121: does any `return` in this body (nested blocks included)
    /// return a call whose callee is an unbound known-HTML-tag name?
    fn body_returns_unbound_html_call(&self, body: &[Stmt]) -> bool {
        let unbound_tag = |e: &Expr| -> bool {
            if let ExprKind::Call { func, .. } = &e.kind {
                if let ExprKind::Name(n) = &func.kind {
                    return react::is_html_element(n)
                        && !self.is_declared(n)
                        && !self.known_functions.contains(n)
                        && !self.known_classes.contains(n);
                }
            }
            false
        };
        for stmt in body {
            match &stmt.kind {
                StmtKind::Return(Some(e)) if unbound_tag(e) => return true,
                StmtKind::If {
                    body,
                    elif_clauses,
                    else_body,
                    ..
                } => {
                    if self.body_returns_unbound_html_call(body) {
                        return true;
                    }
                    for (_, b) in elif_clauses {
                        if self.body_returns_unbound_html_call(b) {
                            return true;
                        }
                    }
                    if let Some(b) = else_body {
                        if self.body_returns_unbound_html_call(b) {
                            return true;
                        }
                    }
                }
                StmtKind::While { body, .. } | StmtKind::For { body, .. } => {
                    if self.body_returns_unbound_html_call(body) {
                        return true;
                    }
                }
                _ => {}
            }
        }
        false
    }

    /// #251: recognise `type(x) == T` / `T == type(x)` where T is a builtin
    /// type name — returns the `type()` argument and the type's `__name__`.
    fn type_identity_cmp<'e>(left: &'e Expr, right: &'e Expr) -> Option<(&'e Expr, &'static str)> {
        fn as_type_call(e: &Expr) -> Option<&Expr> {
            if let ExprKind::Call {
                func, args, kwargs, ..
            } = &e.kind
            {
                if args.len() == 1 && kwargs.is_empty() {
                    if let ExprKind::Name(n) = &func.kind {
                        if n == "type" {
                            return Some(&args[0]);
                        }
                    }
                }
            }
            None
        }
        fn as_builtin_type(e: &Expr) -> Option<&'static str> {
            if let ExprKind::Name(n) = &e.kind {
                return match n.as_str() {
                    "int" => Some("int"),
                    "str" => Some("str"),
                    "list" => Some("list"),
                    "dict" => Some("dict"),
                    "set" => Some("set"),
                    "tuple" => Some("tuple"),
                    "bool" => Some("bool"),
                    "float" => Some("float"),
                    _ => None,
                };
            }
            None
        }
        if let (Some(a), Some(t)) = (as_type_call(left), as_builtin_type(right)) {
            return Some((a, t));
        }
        if let (Some(t), Some(a)) = (as_builtin_type(left), as_type_call(right)) {
            return Some((a, t));
        }
        None
    }

    /// #244: `dict()` with no args — a forced local assigned this must be
    /// Map-backed (extends #230 beyond the `{}` literal to the constructor).
    fn is_empty_dict_ctor(value: &Expr) -> bool {
        matches!(&value.kind, ExprKind::Call { func, args, kwargs, .. }
            if args.is_empty() && kwargs.is_empty()
            && matches!(&func.kind, ExprKind::Name(n) if n == "dict"))
    }

    /// #230: a subscript-WRITE key that is provably a string keeps the plain-JS
    /// -object fast path (the record idiom `d["name"] = v` / `d[f"k{i}"] = v`).
    /// Anything else — an int/tuple literal (already), OR a dynamic key like a
    /// Name/expression — may be non-string, and a plain object silently
    /// stringifies it (`d = {}; d[num] = 1` produced `{"1": ...}` and broke
    /// `num in d`), so force the Map-backed PyDict. Conservative: forcing a
    /// PyDict for a key that turns out to be a string is still correct (PyDict
    /// handles string keys), only marginally slower.
    fn write_key_forces_pydict(key: &Expr) -> bool {
        !matches!(
            &key.kind,
            ExprKind::StringLiteral(_) | ExprKind::FString { .. }
        )
    }

    /// #106 pre-scan: collect names subscript-written (plain or
    /// augmented, incl. tuple-unpack targets) with a non-string literal
    /// key anywhere in the module (nested bodies included).
    fn scan_pydict_forced(stmts: &[Stmt], out: &mut HashSet<String>) {
        let check_target = |t: &Expr, out: &mut HashSet<String>| {
            if let ExprKind::Subscript { value, index, .. } = &t.kind {
                if let ExprKind::Name(n) = &value.kind {
                    if Self::write_key_forces_pydict(index) {
                        out.insert(n.clone());
                    }
                }
            }
        };
        for stmt in stmts {
            match &stmt.kind {
                StmtKind::Assign { targets, .. } => {
                    for t in targets {
                        check_target(t, out);
                        if let ExprKind::Tuple(elts) | ExprKind::List(elts) = &t.kind {
                            for e in elts {
                                check_target(e, out);
                            }
                        }
                    }
                }
                StmtKind::AugAssign { target, .. } => check_target(target, out),
                StmtKind::FuncDef { body, .. } | StmtKind::ClassDef { body, .. } => {
                    Self::scan_pydict_forced(body, out);
                }
                StmtKind::If {
                    body,
                    elif_clauses,
                    else_body,
                    ..
                } => {
                    Self::scan_pydict_forced(body, out);
                    for (_, b) in elif_clauses {
                        Self::scan_pydict_forced(b, out);
                    }
                    if let Some(b) = else_body {
                        Self::scan_pydict_forced(b, out);
                    }
                }
                StmtKind::While {
                    body, else_body, ..
                }
                | StmtKind::For {
                    body, else_body, ..
                } => {
                    Self::scan_pydict_forced(body, out);
                    if let Some(b) = else_body {
                        Self::scan_pydict_forced(b, out);
                    }
                }
                StmtKind::Try {
                    body,
                    handlers,
                    else_body,
                    finally_body,
                } => {
                    Self::scan_pydict_forced(body, out);
                    for h in handlers {
                        Self::scan_pydict_forced(&h.body, out);
                    }
                    if let Some(b) = else_body {
                        Self::scan_pydict_forced(b, out);
                    }
                    if let Some(b) = finally_body {
                        Self::scan_pydict_forced(b, out);
                    }
                }
                StmtKind::With { body, .. } => Self::scan_pydict_forced(body, out),
                _ => {}
            }
        }
    }

    /// Map the inference result onto the certificate's stable receiver
    /// type surface ([`crate::cert::RecvTy`]).
    fn recv_ty_of(ty: JsInferredType) -> crate::cert::RecvTy {
        match ty {
            JsInferredType::Primitive => crate::cert::RecvTy::Primitive,
            JsInferredType::Float => crate::cert::RecvTy::Float,
            JsInferredType::List => crate::cert::RecvTy::List,
            JsInferredType::Dict => crate::cert::RecvTy::Dict,
            JsInferredType::Set => crate::cert::RecvTy::Set,
            JsInferredType::Tuple => crate::cert::RecvTy::Tuple,
            JsInferredType::Unknown => crate::cert::RecvTy::Unknown,
        }
    }

    fn push_scope(&mut self) {
        self.declared_scopes.push(HashSet::new());
        self.hoisted_scopes.push(HashSet::new());
        self.sentinel_scopes.push(HashSet::new());
        self.local_types.push(HashMap::new());
    }

    fn pop_scope(&mut self) {
        self.declared_scopes.pop();
        self.hoisted_scopes.pop();
        self.sentinel_scopes.pop();
        self.local_types.pop();
    }

    /// #269: mark `name` as genuinely hoisted (a function/module-scope `let`)
    /// in the current scope. Called alongside `declare` at every
    /// `collect_hoisted_names` emission site.
    fn mark_hoisted(&mut self, name: &str) {
        if let Some(scope) = self.hoisted_scopes.last_mut() {
            scope.insert(name.to_string());
        }
    }

    /// #269: is `name` a hoisted function/module-scope `let` in the current
    /// scope (so a bare `for (name of …)` can safely write it)?
    fn is_hoisted(&self, name: &str) -> bool {
        self.hoisted_scopes.last().is_some_and(|s| s.contains(name))
    }

    /// PBT-2: mark `name` as a hoisted for-loop target initialized to the
    /// __UNBOUND sentinel in the current scope. Reads of it are routed
    /// through __pyChkLocal/__pyChkGlobal so a zero-iteration loop leaves it
    /// raising (UnboundLocalError / NameError) like CPython, instead of
    /// reading as undefined→None.
    fn mark_sentinel(&mut self, name: &str) {
        if let Some(scope) = self.sentinel_scopes.last_mut() {
            scope.insert(name.to_string());
        }
    }

    /// PBT-2: is `name` a sentinel-initialized hoisted for-target in the
    /// CURRENT scope? (Outer-scope sentinels are deliberately not guarded —
    /// closure reads of an outer loop var keep their existing behavior.)
    fn is_sentinel(&self, name: &str) -> bool {
        self.sentinel_scopes
            .last()
            .is_some_and(|s| s.contains(name))
    }

    /// PBT-2: true while emitting module scope (sentinel reads there raise
    /// NameError, not UnboundLocalError).
    fn at_module_scope(&self) -> bool {
        self.sentinel_scopes.len() == 1
    }

    /// Record a coarse inferred type for `name` in the innermost scope.
    /// Used at assignment sites so later test-expression / binop emit
    /// can route through Python-faithful runtime helpers.
    fn record_type(&mut self, name: &str, ty: JsInferredType) {
        if matches!(ty, JsInferredType::Unknown) {
            return;
        }
        if let Some(scope) = self.local_types.last_mut() {
            scope.insert(name.to_string(), ty);
        }
    }

    /// Walk scopes innermost-out looking for `name`'s inferred type.
    /// Returns `Unknown` if not found.
    fn lookup_type(&self, name: &str) -> JsInferredType {
        for scope in self.local_types.iter().rev() {
            if let Some(&ty) = scope.get(name) {
                return ty;
            }
        }
        JsInferredType::Unknown
    }

    /// Emit a boolean test expression (used by `if`, `while`, ternary).
    /// JS truthiness diverges from Python on empty collections — `if []`
    /// is truthy in JS, falsy in Python. We wrap only when the test
    /// expression is *known* to be a collection (literal or scope-tracked
    /// list/dict/set/tuple); Primitive and Unknown pass through bare to
    /// preserve existing test output and avoid wrapping numeric/bool/str
    /// flows. JS truthiness already matches Python for numbers, strings,
    /// bools, and None.
    fn emit_test_expr(&mut self, test: &Expr) {
        // #211: wrap in pyBool for every non-scalar operand, INCLUDING Unknown
        // — an unannotated `lst` could be a list, and an empty list is FALSY in
        // Python but TRUTHY in JS (`while ([])` loops forever / one step too
        // far). Only Primitive/Float (int/bool/str/None/float) share JS
        // truthiness and keep the bare fast path. Previously only `is_collection`
        // wrapped, so `while lst:` on an unannotated param over-ran on empty
        // (HumanEval /70: `max()`/`min()` on the empty tail).
        if !self.infer_type(test).is_scalar() {
            self.need_runtime("pyBool");
            self.write("pyBool(");
            self.emit_expr(test);
            self.write(")");
        } else {
            self.emit_expr(test);
        }
    }

    /// Map a parameter annotation expression (`x: list`, `x: dict`, etc.)
    /// to the coarse JS inferred type. Used to seed `local_types` for
    /// function parameters so subscript / `==` / `if` sites know the type.
    ///
    /// Recognises bare-name annotations (`list`, `dict`, `set`, `tuple`,
    /// `int`, `str`, etc.) and Subscript forms (`List[int]`, `Dict[str,
    /// int]`, `Optional[T]`). Anything more complex returns Unknown.
    fn js_type_from_annotation(&self, ann: &Expr) -> JsInferredType {
        match &ann.kind {
            ExprKind::Name(n) => match n.as_str() {
                "list" | "List" => JsInferredType::List,
                "dict" | "Dict" => JsInferredType::Dict,
                "set" | "Set" | "frozenset" | "FrozenSet" => JsInferredType::Set,
                "tuple" | "Tuple" => JsInferredType::Tuple,
                "float" => JsInferredType::Float,
                "int" | "bool" | "str" | "bytes" | "None" | "NoneType" => JsInferredType::Primitive,
                _ => JsInferredType::Unknown,
            },
            // `List[int]`, `Dict[str, int]`, etc. — the outer name carries
            // the kind; the type-args are irrelevant for the coarse JS
            // distinction (we only care about list-vs-dict-vs-primitive).
            ExprKind::Subscript { value, .. } => self.js_type_from_annotation(value),
            _ => JsInferredType::Unknown,
        }
    }

    /// Coarse type inference for an expression — used by the JS-quirk
    /// fix sites. Recognises literal collections, primitive literals,
    /// A4: deliberately narrow, whitelist-only check used ONLY by the
    /// print()/str()/repr()/f-string compile-time float fast path (see
    /// the A4 notes at emit_call and the FStringPart::Expr case). This is
    /// NOT the same as `matches!(self.infer_type(expr), JsInferredType::
    /// Float)` — `infer_type`'s `BinOp::Div => Float` rule is
    /// unconditional ("true division always yields a float") and does
    /// NOT check operand types, which is wrong for classes overriding
    /// `__truediv__` (Decimal, Fraction): `Decimal(1) / Decimal(3)` also
    /// infers as `Float` there, even though the runtime value is a
    /// Decimal instance. That imprecision was harmless for infer_type's
    /// other (softer) uses, but reusing it here to bypass pyRepr entirely
    /// via pyFormatFloat corrupted Decimal/Fraction division output
    /// (caught by the differential corpus: dec_div_third et al. went
    /// from `Decimal('0.333...')` to a bare truncated float string).
    ///
    /// So this helper only trusts the cases that can NEVER be a custom
    /// class in disguise: a literal `float` token, or unary +/- directly
    /// wrapping one (`-1.0`). It deliberately does NOT follow Name
    /// look-ups or arithmetic propagation — those would need to trust
    /// `infer_type` (or its Div imprecision) transitively. This narrows
    /// the *compile-time* fix to exactly the literal-argument case from
    /// the bug report (`print(1.0)`, `str(1.0)`, `repr(1.0)`, `f"{1.0}"`
    /// and their negated forms) rather than the broader "any statically-
    /// float-typed expression" — a documented, deliberate scope
    /// narrowing, not an oversight.
    fn is_definitely_float(&self, expr: &Expr) -> bool {
        use pyths_syntax::operators::BinOp as B;
        match &expr.kind {
            ExprKind::FloatLiteral(_) => true,
            // #227: a variable whose tracked type is Float (`x = 2.0` then
            // `print(x)`). record_type/lookup_type already propagate Float
            // conservatively (last-write-wins per assignment); without this a
            // whole float stored in a var printed as an int (`2` not `2.0`),
            // because at runtime `2.0` and `2` are the same JS number. Does not
            // reach through params/containers — a residual of the same
            // int/float representation limit documented on isinstance (#215).
            ExprKind::Name(n) => matches!(self.lookup_type(n), JsInferredType::Float),
            // F4: a `float(...)` call unconditionally yields a real Python
            // float, so `print(float(2))`/`str(float(2))` pre-format to "2.0".
            // Safe here (unlike infer_type's blanket `Div => Float`): float()
            // never returns a Decimal/Fraction whose `__repr__` we'd corrupt.
            ExprKind::Call { func, .. } if matches!(&func.kind, ExprKind::Name(n) if n == "float") => {
                true
            }
            // #136: calls to module-level `-> float`-annotated functions.
            ExprKind::Call { func, .. } if matches!(&func.kind, ExprKind::Name(n) if self.float_returning_functions.contains(n)) => {
                true
            }
            // #318: `round(x, ndigits)` keeps x's type. With a definitely-float
            // first arg it is a float (`round(1234.5678, -2)` → `1200.0`), so it
            // must pre-format. Single-arg `round(x)` is an int — excluded by the
            // `>= 2` guard. Requiring the arg be definitely-float keeps Decimal/
            // Fraction (whose __repr__ must not be float-formatted) out, since
            // those are never definitely-float.
            ExprKind::Call { func, args, .. }
                if matches!(&func.kind, ExprKind::Name(n) if n == "round")
                    && args.len() >= 2
                    && self.is_definitely_float(&args[0]) =>
            {
                true
            }
            // #283: `abs(z)` for a definitely-complex `z` is a Python float
            // (magnitude), so `print(abs(3+4j))` pre-formats to "5.0" (a whole-
            // valued magnitude would otherwise print as "5").
            ExprKind::Call { func, args, .. }
                if matches!(&func.kind, ExprKind::Name(n) if n == "abs")
                    && args.len() == 1
                    && self.is_definitely_complex(&args[0]) =>
            {
                true
            }
            // #283: `z.real` / `z.imag` on a definitely-complex `z` are floats
            // (`(2j).real` -> 0.0, `(3+4j).imag` -> 4.0).
            ExprKind::Attribute { value, attr, .. }
                if matches!(attr.as_str(), "real" | "imag")
                    && self.is_definitely_complex(value) =>
            {
                true
            }
            ExprKind::UnaryOp {
                op: pyths_syntax::operators::UnaryOp::Neg | pyths_syntax::operators::UnaryOp::Pos,
                operand,
            } => self.is_definitely_float(operand),
            // #136: arithmetic with a definitely-float operand yields a
            // float. CPython: float op Fraction → float (correct to
            // float-format); float op Decimal raises TypeError, so no
            // valid program reaches this arm with a Decimal.
            ExprKind::BinOp { left, op, right }
                if matches!(
                    op,
                    B::Add | B::Sub | B::Mul | B::Div | B::Mod | B::Pow | B::FloorDiv
                ) =>
            {
                if self.is_definitely_float(left) || self.is_definitely_float(right) {
                    return true;
                }
                // True division of definitely-NUMERIC operands is a float
                // in Python 3 (`8 / 2` → 4.0). Both sides must be
                // definitely numeric so Decimal/Fraction division (whose
                // __repr__ must not be float-formatted) stays out — the
                // reason this predicate is a whitelist.
                matches!(op, B::Div)
                    && self.is_definitely_number(left)
                    && self.is_definitely_number(right)
            }
            _ => false,
        }
    }

    /// #283: is `expr` statically a Python `complex`? An imaginary literal, a
    /// unary +/- of one, or `+`/`-`/`*` where either operand is complex.
    /// Used to pre-format the float-typed results `abs(z)`, `z.real`, `z.imag`.
    /// Deliberately narrow (literal-driven, like `is_definitely_float`): does
    /// not chase Names/containers. `/` and `**` are out of scope (#283).
    fn is_definitely_complex(&self, expr: &Expr) -> bool {
        use pyths_syntax::operators::BinOp as B;
        match &expr.kind {
            ExprKind::ImagLiteral(_) => true,
            ExprKind::UnaryOp {
                op: pyths_syntax::operators::UnaryOp::Neg | pyths_syntax::operators::UnaryOp::Pos,
                operand,
            } => self.is_definitely_complex(operand),
            ExprKind::BinOp {
                left,
                op: B::Add | B::Sub | B::Mul,
                right,
            } => self.is_definitely_complex(left) || self.is_definitely_complex(right),
            _ => false,
        }
    }

    /// #136 helper: int/float literals and arithmetic compositions of
    /// them — the operands for which `a / b` is definitely a Python
    /// float (never Decimal/Fraction).
    fn is_definitely_number(&self, expr: &Expr) -> bool {
        use pyths_syntax::operators::BinOp as B;
        match &expr.kind {
            ExprKind::IntLiteral(_) => true,
            ExprKind::UnaryOp {
                op: pyths_syntax::operators::UnaryOp::Neg | pyths_syntax::operators::UnaryOp::Pos,
                operand,
            } => self.is_definitely_number(operand),
            ExprKind::BinOp {
                left,
                op: B::Add | B::Sub | B::Mul | B::Div | B::Mod | B::Pow | B::FloorDiv,
                right,
            } => self.is_definitely_number(left) && self.is_definitely_number(right),
            _ => self.is_definitely_float(expr),
        }
    }

    /// boolean-result ops, builtin call returns, and Name lookups
    /// through `local_types`. Anything else returns `Unknown`.
    /// #273: does `expr`'s JS truthiness match Python's? True for scalars
    /// (int/bool/str/None/float) — a raw JS `&&`/`||` short-circuits correctly.
    /// A container or Unknown may be an empty collection (JS-truthy, Python-falsy)
    /// so it must go through pyAnd/pyOr.
    fn truthiness_agrees(&self, expr: &Expr) -> bool {
        matches!(
            self.infer_type(expr),
            JsInferredType::Primitive | JsInferredType::Float
        )
    }

    fn infer_type(&self, expr: &Expr) -> JsInferredType {
        match &expr.kind {
            ExprKind::FloatLiteral(_) => JsInferredType::Float,
            ExprKind::IntLiteral(_)
            | ExprKind::BoolLiteral(_)
            | ExprKind::NoneLiteral
            | ExprKind::StringLiteral(_)
            | ExprKind::FString { .. } => JsInferredType::Primitive,
            ExprKind::List(_) | ExprKind::ListComp { .. } => JsInferredType::List,
            ExprKind::Tuple(_) => JsInferredType::Tuple,
            ExprKind::Set(_) | ExprKind::SetComp { .. } => JsInferredType::Set,
            ExprKind::Dict { .. } | ExprKind::DictComp { .. } => JsInferredType::Dict,
            // Boolean-result operators: comparison, logical-not, `in`, `is`.
            ExprKind::Compare { .. } => JsInferredType::Primitive,
            ExprKind::UnaryOp {
                op: pyths_syntax::operators::UnaryOp::Not,
                ..
            } => JsInferredType::Primitive,
            // Arithmetic between primitives stays primitive; between
            // collections we treat the *result* as that collection kind
            // (since this method is also used to seed scope from RHS).
            ExprKind::BinOp { op, left, right } => match op {
                pyths_syntax::operators::BinOp::Eq
                | pyths_syntax::operators::BinOp::NotEq
                | pyths_syntax::operators::BinOp::Lt
                | pyths_syntax::operators::BinOp::LtEq
                | pyths_syntax::operators::BinOp::Gt
                | pyths_syntax::operators::BinOp::GtEq
                | pyths_syntax::operators::BinOp::In
                | pyths_syntax::operators::BinOp::NotIn
                | pyths_syntax::operators::BinOp::Is
                | pyths_syntax::operators::BinOp::IsNot => JsInferredType::Primitive,
                pyths_syntax::operators::BinOp::Add => {
                    let lt = self.infer_type(left);
                    let rt = self.infer_type(right);
                    if lt == rt && lt.is_collection() {
                        lt
                    } else if lt.is_scalar() && rt.is_scalar() {
                        // float propagates (int + float → float).
                        if matches!(lt, JsInferredType::Float)
                            || matches!(rt, JsInferredType::Float)
                        {
                            JsInferredType::Float
                        } else {
                            JsInferredType::Primitive
                        }
                    } else {
                        JsInferredType::Unknown
                    }
                }
                // True division always yields a float.
                pyths_syntax::operators::BinOp::Div => JsInferredType::Float,
                _ => {
                    let lt = self.infer_type(left);
                    let rt = self.infer_type(right);
                    if lt.is_scalar() && rt.is_scalar() {
                        if matches!(lt, JsInferredType::Float)
                            || matches!(rt, JsInferredType::Float)
                        {
                            JsInferredType::Float
                        } else {
                            JsInferredType::Primitive
                        }
                    } else {
                        JsInferredType::Unknown
                    }
                }
            },
            ExprKind::IfExpr {
                body, else_body, ..
            } => {
                // `x if c else y` — result type matches branches if they agree.
                let bt = self.infer_type(body);
                let et = self.infer_type(else_body);
                if bt == et {
                    bt
                } else {
                    JsInferredType::Unknown
                }
            }
            ExprKind::Name(name) => self.lookup_type(name),
            ExprKind::Call { func, args, .. } => {
                if let ExprKind::Name(n) = &func.kind {
                    match n.as_str() {
                        // range/enumerate lower to pyRange/pyEnumerate,
                        // which return real arrays — safe to treat as List
                        // (keeps the comprehension fast path unwrapped).
                        "list" | "sorted" | "reversed" | "range" | "enumerate" => {
                            JsInferredType::List
                        }
                        "dict" => JsInferredType::Dict,
                        "set" | "frozenset" => JsInferredType::Set,
                        "tuple" => JsInferredType::Tuple,
                        // F4: float() always yields a Python float, so the
                        // print/str/repr float pre-formatter renders `float(2)`
                        // as "2.0" (not "2").
                        "float" => JsInferredType::Float,
                        // #318: `round(x)` is an int, but `round(x, ndigits)`
                        // keeps x's type — `round(1234.5678, -2)` is the float
                        // `1200.0`, not `1200`. Infer Float only when a second
                        // arg is present AND the value is a known float.
                        "round" => {
                            if args.len() >= 2
                                && matches!(self.infer_type(&args[0]), JsInferredType::Float)
                            {
                                JsInferredType::Float
                            } else {
                                JsInferredType::Primitive
                            }
                        }
                        "len" | "int" | "bool" | "abs" | "ord" | "sum" | "min" | "max" => {
                            JsInferredType::Primitive
                        }
                        "str" | "repr" | "chr" | "input" => JsInferredType::Primitive,
                        _ => JsInferredType::Unknown,
                    }
                } else {
                    JsInferredType::Unknown
                }
            }
            _ => JsInferredType::Unknown,
        }
    }

    /// Check if a name has been declared in the current scope.
    fn is_declared(&self, name: &str) -> bool {
        self.declared_scopes
            .last()
            .is_some_and(|s| s.contains(name))
    }

    /// #306: check if a name is declared in ANY live scope (module scope
    /// included). `is_declared` only consults the innermost frame, which is
    /// right for shadowing decisions but wrong for "is this name bound to
    /// anything at all?" — a module-level `ins = ...` or an outer-function
    /// local must count as a binding when deciding whether a lowercase PSX
    /// call can be claimed as an HTML tag.
    fn is_declared_in_any_scope(&self, name: &str) -> bool {
        self.declared_scopes.iter().any(|s| s.contains(name))
    }

    /// #306: inside a @component/@psx body, should this lowercase call name
    /// lower to `createElement("<name>", ...)` even though it is not in the
    /// known HTML/SVG allowlist? React accepts ANY lowercase tag string
    /// (unknown tags render as custom elements with a dev-only warning), so
    /// a tag-shaped name with NO visible binding — not a local/param, not a
    /// module-level def/class, not an import, not a Python builtin — is far
    /// more likely a real element (`ins`, `del`, `font`, `center`, ...)
    /// than a function call. The old behavior emitted a bare `ins(...)`
    /// identifier call: a guaranteed ReferenceError at runtime, silent at
    /// compile time. Same rescue principle as #110/#121: only code that
    /// previously threw can be claimed.
    ///
    /// Known limitation (documented): a module-level ASSIGNMENT bound
    /// *after* the component in source order (`@component def App(): ...`
    /// followed by `ins = lambda: ...`) is not visible to the single-pass
    /// emitter and would be claimed as a tag. Top-level `def`/`class`/
    /// imports are pre-scanned and always win regardless of order.
    fn is_unbound_psx_tag(&self, name: &str) -> bool {
        // Tag-shaped: [a-z][a-z0-9]* — every HTML/SVG element name fits
        // this shape; snake_case helpers (`use_query`) and anything with
        // uppercase (camelCase locals) never match.
        let tag_shaped = name.chars().next().is_some_and(|c| c.is_ascii_lowercase())
            && name
                .chars()
                .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit());
        tag_shaped
            && !self.is_declared_in_any_scope(name)
            && !self.known_functions.contains(name)
            && !self.known_classes.contains(name)
            && !self.imported_bindings.contains(name)
            && !self.react_imports.contains(name)
            && !self.module_namespaces.contains(name)
            && !is_python_builtin_name(name)
            && !is_js_global_callable(name)
    }

    /// #306: PSX-element dispatch used at every `in_component` call site —
    /// the known HTML/SVG allowlist (fast path, order-independent) plus the
    /// unbound-lowercase-tag fallback above.
    fn is_psx_tag_call(&self, name: &str) -> bool {
        react::is_psx_element(name) || self.is_unbound_psx_tag(name)
    }

    /// F1: sanitize a Python identifier that is emitted into a JS *binding*
    /// or *reference* position (variable / function / param / class name and
    /// every reference to it). JS reserved / contextual words that are legal
    /// Python identifiers (`let`, `new`, `delete`, `default`, ...) become
    /// invalid JS binding names, so we deterministically append `$`. Because
    /// the mapping is a pure function of the name, every reference to the same
    /// Python binding renames identically — no scope tracking needed. Member
    /// access (`obj.new`) and dict keys are emitted through other paths and
    /// stay verbatim (already-legal JS). `super` is intentionally excluded:
    /// PythScribe lowers `super()` specially and JS `super` is valid in the
    /// positions it appears.
    fn sanitize_ident(name: &str) -> std::borrow::Cow<'_, str> {
        if is_js_reserved_word(name) {
            std::borrow::Cow::Owned(format!("{}$", name))
        } else {
            std::borrow::Cow::Borrowed(name)
        }
    }

    /// Mark a name as declared in the current scope.
    fn declare(&mut self, name: &str) {
        if let Some(scope) = self.declared_scopes.last_mut() {
            scope.insert(name.to_string());
        }
    }

    /// Mark all names in an assignment/for target as declared.
    fn declare_target(&mut self, target: &Expr) {
        match &target.kind {
            ExprKind::Name(name) => self.declare(name),
            ExprKind::Tuple(elts) | ExprKind::List(elts) => {
                for elt in elts {
                    self.declare_target(elt);
                }
            }
            ExprKind::Starred(inner) => self.declare_target(inner),
            _ => {}
        }
    }

    // ── Module ────────────────────────────────────────────

    pub fn emit_module(&mut self, module: &Module) {
        // Heuristic: ~80 bytes of JS per statement
        self.output.reserve(module.body.len() * 80);

        // Pre-scan: collect every top-level `class` name so that calls
        // inside @component functions can disambiguate dataclass
        // instantiation (`Alert(...)` → `new Alert(...)`) from React
        // component creation (`Header(...)` → `createElement(Header, ...)`).
        for stmt in &module.body {
            if let StmtKind::ClassDef { name, .. } = &stmt.kind {
                self.known_classes.insert(name.clone());
            }
            // #253: datetime's classes are lowercase, so record them here so
            // `date(...)` / `datetime(...)` get `new` (heuristic won't fire).
            if let StmtKind::ImportFrom {
                module,
                names,
                level,
            } = &stmt.kind
            {
                if module == "datetime" {
                    for a in names {
                        if matches!(a.name.as_str(), "datetime" | "date" | "time" | "timedelta") {
                            let local = a.alias.as_deref().unwrap_or(&a.name);
                            self.known_classes.insert(local.to_string());
                        }
                    }
                }
                // GENERAL: a CapWords name imported from ANY stdlib module is a
                // class (Counter/OrderedDict/ChainMap/Decimal/Fraction/…), by the
                // Python naming convention. Register it so a call inside a
                // @component lowers to `new X(...)` (constructor) instead of
                // `createElement(X, ...)` — the capitalized-name → React-component
                // default that otherwise mis-lowers EVERY capitalized stdlib
                // constructor used in a component (the interaction-suite
                // `Counter.most_common` bug). Lowercase stdlib names are functions
                // (product/combinations/defaultdict/deque) and correctly stay
                // plain calls; datetime's lowercase classes are handled above.
                if *level == 0 && STDLIB_MODULES.contains(&module.as_str()) {
                    for a in names {
                        let local = a.alias.as_deref().unwrap_or(&a.name);
                        if local.chars().next().is_some_and(|c| c.is_uppercase()) {
                            self.known_classes.insert(local.to_string());
                        }
                    }
                }
                // #300: a RELATIVE import (`from .shape import Shape`) binds
                // names from another module of the same PythScribe project —
                // compiled by this same compiler with the same object model.
                // Record them so `class Rectangle(Shape)` with an imported
                // base takes the cooperative PyObject/`__init__` path instead
                // of the native-`constructor` path (which emitted a derived
                // constructor with no `super()` → "Must call super
                // constructor" at instantiation). Absolute imports stay
                // external: npm/React classes are never relative.
                if *level > 0 {
                    for a in names {
                        if a.name != "*" {
                            let local = a.alias.as_deref().unwrap_or(&a.name);
                            self.local_module_imports.insert(local.to_string());
                        }
                    }
                }
            }
            // #80: record top-level `def` names so the class-instantiation
            // capitalization heuristic doesn't `new`-call plain functions
            // (`def Foo(): return 42` / `Foo()` returned `{}`).
            Self::scan_pydict_forced(std::slice::from_ref(stmt), &mut self.pydict_forced_locals);
            if let StmtKind::FuncDef {
                name, return_type, ..
            } = &stmt.kind
            {
                self.known_functions.insert(name.clone());
                // #136: `-> float`-annotated functions definitely return
                // floats (the annotation is the user's contract), so
                // repr/str/print/f-string of their call results can
                // float-format at compile time.
                if let Some(rt_expr) = return_type {
                    if matches!(&rt_expr.kind, ExprKind::Name(n) if n == "float") {
                        self.float_returning_functions.insert(name.clone());
                    }
                }
            }
        }

        // Hoist `"use client"` / `"use server"` to the very top of the
        // module. React/Next require the directive to be the first
        // statement (before imports) — so it must lead even when a module
        // docstring precedes it in source. Scan the leading run of
        // string-literal statements (docstring + directives); emit the
        // directive(s) here and skip them in the body loop below. `finish()`
        // then keeps the directive above the prepended runtime imports.
        let mut directive_idxs: std::collections::HashSet<usize> = std::collections::HashSet::new();
        for (i, stmt) in module.body.iter().enumerate() {
            if let StmtKind::Expr(expr) = &stmt.kind {
                if let ExprKind::StringLiteral(s) = &expr.kind {
                    if s == "use client" || s == "use server" {
                        self.writeln(&format!("\"{}\";", s));
                        directive_idxs.insert(i);
                    }
                    // A docstring (non-directive string) does NOT stop the
                    // scan — the directive may legally follow it.
                    continue;
                }
            }
            // First non-string statement ends the leading directive region.
            break;
        }

        // B-023 extension (Twitter-clone find): the nested-block `let` hoist
        // previously ran only for FUNCTION bodies, so a module-level
        // `try: v = ... except KeyError: v = default` block-scoped `let v`
        // inside the try and the except branch hit a ReferenceError. Apply
        // the same Python-scoping hoist at module level: names first
        // assigned inside a nested block get a top-level `let` declaration.
        // PBT-2: a hoisted for-loop target with no other guaranteed binding
        // is initialized to the __UNBOUND sentinel; reads route through
        // __pyChkGlobal so a zero-iteration loop leaves it raising NameError
        // (CPython) instead of reading as undefined→None.
        let hoisted_names = Self::collect_hoisted_names(&module.body);
        let hoisted_set: HashSet<String> = hoisted_names.iter().map(|(n, _)| n.clone()).collect();
        let mut sentinels = Self::sentinel_for_names(&module.body, &hoisted_set);
        // #288: a promoted name's depth-0 first assignment executes before
        // any of its loops — the binding is guaranteed, so no sentinel (and
        // no guard churn on its reads).
        for (n, promoted) in &hoisted_names {
            if *promoted {
                sentinels.remove(n);
            }
        }
        for (hoisted, promoted) in hoisted_names {
            if !self.is_declared(&hoisted) {
                self.write_indent();
                // #288: a promoted name's inline depth-0 first assignment
                // would have carried `export` (B-015); the hoisted `let`
                // takes over the declaration, so keep the export here.
                if promoted {
                    self.write("export ");
                }
                if sentinels.contains(&hoisted) {
                    self.need_runtime("__UNBOUND");
                    self.write(&format!(
                        "let {} = __UNBOUND;\n",
                        Self::sanitize_ident(&hoisted)
                    ));
                    self.mark_sentinel(&hoisted);
                } else {
                    self.write(&format!("let {};\n", Self::sanitize_ident(&hoisted)));
                }
                self.declare(&hoisted);
            }
            // #269: a hoisted name is a genuine module-scope `let` (or an
            // already-declared enclosing binding) — safe for a bare for-target.
            self.mark_hoisted(&hoisted);
        }

        for (i, stmt) in module.body.iter().enumerate() {
            if directive_idxs.contains(&i) {
                continue; // already hoisted above
            }
            self.emit_stmt(stmt);
        }
    }

    // ── Statements ────────────────────────────────────────

    fn emit_stmt(&mut self, stmt: &Stmt) {
        // #201: `import` inside a function body must be hoisted to module scope
        // (ES imports are top-level only). Capture the import emission, dedent
        // it, and defer the `import` lines to the preamble via `hoisted_imports`;
        // any non-import lines (e.g. an unresolvable-import `throw`) stay inline.
        let is_import = matches!(
            &stmt.kind,
            StmtKind::Import { .. } | StmtKind::ImportFrom { .. } | StmtKind::ImportSideEffect(_)
        );
        if is_import && self.indent > 0 {
            let saved_out = std::mem::take(&mut self.output);
            let saved_indent = self.indent;
            let (saved_line, saved_col) = (self.out_line, self.out_col);
            self.indent = 0;
            self.emit_stmt_inner(stmt);
            self.indent = saved_indent;
            let captured = std::mem::replace(&mut self.output, saved_out);
            self.out_line = saved_line;
            self.out_col = saved_col;
            for line in captured.lines() {
                if line.trim().is_empty() {
                    continue;
                }
                if line.trim_start().starts_with("import ") {
                    if !self.hoisted_imports.iter().any(|h| h == line) {
                        self.hoisted_imports.push(line.to_string());
                    }
                } else {
                    self.writeln(line);
                }
            }
            return;
        }
        self.emit_stmt_inner(stmt);
    }

    fn emit_stmt_inner(&mut self, stmt: &Stmt) {
        // Record source mapping for this statement
        self.mark_mapping(stmt.span.start);
        match &stmt.kind {
            StmtKind::Expr(expr) => {
                self.write_indent();
                self.emit_expr(expr);
                self.write(";\n");
            }
            StmtKind::Assign { targets, value } => {
                self.emit_assign(targets, value);
            }
            StmtKind::AugAssign { target, op, value } => {
                // #83: augmented subscript assignment on a Dict/Unknown
                // receiver must go through the shape-dispatching helpers
                // (raw `d[k] += v` breaks on a Map-backed dict). Hoist the
                // receiver + key into consts so side effects run once,
                // then read-modify-write via pyGetItem/pySetItem with the
                // matching Python operator helper.
                if let ExprKind::Subscript {
                    value: recv,
                    index,
                    optional,
                } = &target.kind
                {
                    if !*optional && !matches!(&index.kind, ExprKind::Slice { .. }) {
                        let recv_ty = self.infer_type(recv);
                        // #278: the bare `a[i] OP= v` form is a real JS element
                        // access only for a List/Tuple receiver indexed by a
                        // provably-NON-NEGATIVE integer literal. JS `a[-1]` is the
                        // property "-1", not Python's last element, so `nums[-1]
                        // -= s` silently wrote a stray "-1" property and left the
                        // last element unchanged (while pyGetItem read it back
                        // correctly → divergence). Dict/Unknown, negative or
                        // computed indices route through pyGetItem/pySetItem.
                        // #249/bigint: the bare raw-JS `a[i] OP= v` path
                        // truncates BigInt-producing operators to int32 and
                        // throws on mixed BigInt/number (`xs[0] += 2**60`,
                        // `xs[0] <<= 50`). Only take the fast path for
                        // operators with no Python helper — everything with a
                        // helper routes through the hoisting path below so the
                        // arbitrary-precision semantics hold.
                        let op_has_helper = matches!(
                            op,
                            AugAssignOp::Add
                                | AugAssignOp::Sub
                                | AugAssignOp::Mul
                                | AugAssignOp::Div
                                | AugAssignOp::FloorDiv
                                | AugAssignOp::Mod
                                | AugAssignOp::Pow
                                | AugAssignOp::BitAnd
                                | AugAssignOp::BitOr
                                | AugAssignOp::BitXor
                                | AugAssignOp::ShiftLeft
                                | AugAssignOp::ShiftRight
                        );
                        let bare_ok = !op_has_helper
                            && matches!(recv_ty, JsInferredType::List | JsInferredType::Tuple)
                            && matches!(&index.kind, ExprKind::IntLiteral(n) if *n >= 0);
                        if !bare_ok {
                            let n = self.default_hoist_counter;
                            self.default_hoist_counter += 1;
                            let o = format!("__aug_o{}", n);
                            let k = format!("__aug_k{}", n);
                            self.need_runtime("pyGetItem");
                            self.need_runtime("pySetItem");
                            self.write_indent();
                            self.write(&format!("{{ const {} = ", o));
                            self.emit_expr(recv);
                            self.write(&format!("; const {} = ", k));
                            self.emit_expr(index);
                            self.write("; ");
                            // Python-operator helper where one exists
                            // (arbitrary-precision ints, list concat, dict
                            // merge, ...); raw JS operator for the rest.
                            let helper = match op {
                                AugAssignOp::Add => Some("pyAdd"),
                                AugAssignOp::Sub => Some("pySub"),
                                AugAssignOp::Mul => Some("pyMul"),
                                AugAssignOp::Div => Some("pyDiv"),
                                AugAssignOp::FloorDiv => Some("pyFloorDiv"),
                                AugAssignOp::Mod => Some("pyMod"),
                                AugAssignOp::Pow => Some("pyPow"),
                                AugAssignOp::BitAnd => Some("pyBitAnd"),
                                AugAssignOp::BitOr => Some("pyBitOr"),
                                AugAssignOp::BitXor => Some("pyBitXor"),
                                AugAssignOp::ShiftLeft => Some("pyShiftLeft"),
                                AugAssignOp::ShiftRight => Some("pyShiftRight"),
                            };
                            match helper {
                                Some(h) => {
                                    self.need_runtime(h);
                                    self.write(&format!(
                                        "pySetItem({o}, {k}, {h}(pyGetItem({o}, {k}), ",
                                        o = o,
                                        k = k,
                                        h = h
                                    ));
                                    self.emit_expr(value);
                                    self.write(")); }\n");
                                }
                                None => {
                                    let js_op = aug_assign_op_str(op).trim_end_matches('=');
                                    self.write(&format!(
                                        "pySetItem({o}, {k}, (pyGetItem({o}, {k}) {op} ",
                                        o = o,
                                        k = k,
                                        op = js_op
                                    ));
                                    self.emit_expr(value);
                                    self.write(")); }\n");
                                }
                            }
                            return;
                        }
                    }
                }
                // Round-2 pythonic sweep: plain-NAME augmented assignment
                // routes through the same Python-operator helpers as the
                // binary form — raw JS `d |= {...}` coerces a dict to a
                // number (printed 0), raw `+=` skips BigInt promotion and
                // list concat. Reading a name twice is side-effect-free,
                // so `x = h(x, v)` is exact.
                if matches!(&target.kind, ExprKind::Name(_)) {
                    let helper = match op {
                        AugAssignOp::Add => Some("pyAdd"),
                        AugAssignOp::Sub => Some("pySub"),
                        AugAssignOp::Mul => Some("pyMul"),
                        AugAssignOp::Div => Some("pyDiv"),
                        AugAssignOp::FloorDiv => Some("pyFloorDiv"),
                        AugAssignOp::Mod => Some("pyMod"),
                        AugAssignOp::Pow => Some("pyPow"),
                        AugAssignOp::BitAnd => Some("pyBitAnd"),
                        AugAssignOp::BitOr => Some("pyBitOr"),
                        AugAssignOp::BitXor => Some("pyBitXor"),
                        AugAssignOp::ShiftLeft => Some("pyShiftLeft"),
                        AugAssignOp::ShiftRight => Some("pyShiftRight"),
                    };
                    if let Some(h) = helper {
                        self.need_runtime(h);
                        self.write_indent();
                        self.in_lhs_target = true;
                        self.emit_expr(target);
                        self.in_lhs_target = false;
                        self.write(&format!(" = {}(", h));
                        self.emit_expr(target);
                        self.write(", ");
                        self.emit_expr(value);
                        self.write(");\n");
                        return;
                    }
                }
                self.write_indent();
                // Augmented assignment is a read-then-write. JS `+=` /
                // `-=` etc. do both atomically against the same lvalue,
                // so we must emit the target in bare-LHS form (no
                // pyGetItem wrap) for the syntax to compile.
                self.in_lhs_target = true;
                self.emit_expr(target);
                self.in_lhs_target = false;
                self.write(&format!(" {} ", aug_assign_op_str(op)));
                self.emit_expr(value);
                self.write(";\n");
            }
            StmtKind::FuncDef {
                name,
                params,
                body,
                decorator_list,
                return_type: _,
                is_async,
            } => {
                if self.wasm_skip.contains(name) {
                    return; // compiled to WASM, re-exported from glue
                }
                self.declare(name);
                self.emit_func_def(name, params, body, decorator_list, *is_async);
            }
            StmtKind::ClassDef {
                name,
                bases,
                body,
                decorator_list,
            } => {
                self.declare(name);
                self.emit_class_def(name, bases, body, decorator_list);
            }
            StmtKind::Return(value) => {
                self.write_indent();
                self.write("return");
                if let Some(expr) = value {
                    self.write(" ");
                    // In component context, a returned tuple becomes a fragment
                    if self.in_component {
                        if let ExprKind::Tuple(elements) = &expr.kind {
                            self.emit_psx_fragment(elements);
                        } else {
                            self.emit_expr(expr);
                        }
                    } else {
                        self.emit_expr(expr);
                    }
                }
                self.write(";\n");
            }
            StmtKind::If {
                test,
                body,
                elif_clauses,
                else_body,
            } => {
                self.emit_if(test, body, elif_clauses, else_body);
            }
            StmtKind::While {
                test,
                body,
                else_body,
            } => {
                self.emit_while(test, body, else_body);
            }
            StmtKind::For {
                target,
                iter,
                body,
                else_body,
                is_async,
            } => {
                self.emit_for(target, iter, body, else_body, *is_async);
            }
            StmtKind::Break => {
                // #91: when the innermost enclosing loop has an `else`
                // clause, breaking must set that loop's flag so the else
                // clause is suppressed (previously the flag was declared
                // and checked but never set). Only the innermost frame —
                // an inner loop's break must not touch an outer flag.
                if let Some(Some(flag)) = self.loop_flag_stack.last() {
                    let flag = flag.clone();
                    self.writeln(&format!("{} = true;", flag));
                }
                self.writeln("break;");
            }
            StmtKind::Continue => {
                self.writeln("continue;");
            }
            StmtKind::Pass => {
                // No-op in JS — emit comment for clarity
            }
            StmtKind::Import { names } => {
                for alias in names {
                    let local = alias.alias.as_deref().unwrap_or(&alias.name);
                    self.declare(local);
                    // Round-4 sweep: remember asyncio namespace bindings so
                    // `asyncio.run(...)` call-sites can be awaited.
                    if alias.name == "asyncio" || alias.name == "pyths.asyncio" {
                        self.asyncio_namespaces.insert(local.to_string());
                    }
                    // #221: track stdlib module namespaces so `re.split(...)`
                    // isn't mistaken for the string `.split` method.
                    let base = alias.name.strip_prefix("pyths.").unwrap_or(&alias.name);
                    if STDLIB_MODULES.contains(&base) {
                        self.module_namespaces.insert(local.to_string());
                    }
                    // Track-B: `import at_radix_ui.react_dialog as Dialog` —
                    // dotted PSX tags rooted at this alias (`Dialog.Root`)
                    // are library components; their props get snake→camel'd.
                    if is_react_or_next_module(&alias.name) {
                        self.react_lib_module_aliases.insert(local.to_string());
                    }
                    // #274: `import heapq` twice → two `import * as heapq` →
                    // "already declared". Emit the binding once.
                    if !self.imported_bindings.insert(local.to_string()) {
                        continue;
                    }
                    let module_path = self.resolve_module(&alias.name);
                    // SECURITY (A2): module_path may be a verbatim
                    // `[npm.imports]` override value — config-derived, untrusted.
                    // Route through the escaper; never `format!("\"{}\"", …)`.
                    self.writeln(&format!(
                        "import * as {} from {};",
                        local,
                        js_string_literal(&module_path)
                    ));
                }
            }
            StmtKind::ImportSideEffect(path) => {
                // PythScribe extension: `import "./styles.css"` — side-effect
                // asset import, no bound name. A specifier containing a raw
                // quote / backslash / newline / U+2028-9 is not a valid module
                // path — it can only be an attempt to break out of the emitted
                // string literal and inject a top-level statement. Reject with a
                // clean compile error rather than emitting anything.
                if path.contains('"')
                    || path.contains('\\')
                    || path.contains('\n')
                    || path.contains('\r')
                    || path.contains('\u{2028}')
                    || path.contains('\u{2029}')
                {
                    let diag = format!(
                        "invalid side-effect import specifier {:?}: module specifiers may \
                         not contain quote, backslash, or newline characters",
                        path
                    );
                    eprintln!("error: {}", diag);
                    self.codegen_errors.push(diag.clone());
                    self.writeln(&format!(
                        "throw new Error({});",
                        js_string_literal(&format!("PythScribe: {}", diag))
                    ));
                    return;
                }
                // SECURITY: source-derived; MUST go through the escaper (never
                // `format!("\"{}\"", path)`). See escape_js_string doc.
                self.writeln(&format!("import {};", js_string_literal(path)));
            }
            StmtKind::ImportFrom {
                module,
                names,
                level,
            } => {
                // #253: the datetime module's classes are spelled lowercase
                // (datetime/date/time/timedelta), so the capitalization
                // heuristic won't `new`-call them — register as known classes.
                if module == "datetime" {
                    for a in names {
                        if matches!(a.name.as_str(), "datetime" | "date" | "time" | "timedelta") {
                            let local = a.alias.as_deref().unwrap_or(&a.name);
                            self.known_classes.insert(local.to_string());
                        }
                    }
                }
                // #276: `from module import *`. The erased modules
                // (typing/dataclasses/pydantic) and relative star imports are a
                // no-op; any other module gets a namespace import — valid ESM
                // that loads the module (preserving import side-effects). Bare
                // starred names are NOT bound as locals (ESM has no "import all
                // names" form without enumerating them); that stays a documented
                // v3.x enhancement. All 5 LiveCodeBench `import *` samples used
                // it as unused boilerplate (`from math import *`).
                if names.len() == 1 && names[0].name == "*" {
                    if *level == 0
                        && !matches!(module.as_str(), "typing" | "dataclasses" | "pydantic")
                    {
                        let module_path = self.resolve_module(module);
                        let ns = format!("__pyStar{}", self.default_hoist_counter);
                        self.default_hoist_counter += 1;
                        // SECURITY (A2): escape the (possibly config-overridden)
                        // specifier.
                        self.writeln(&format!(
                            "import * as {} from {};",
                            ns,
                            js_string_literal(&module_path)
                        ));
                    }
                    return;
                }

                // Relative imports (`from .foo import x`) bypass all the
                // npm-name remapping / pyths.react splitting / stdlib
                // routing. They emit a literal relative ESM specifier
                // computed from the dot-depth + dotted module name; no
                // kebab-casing of the trailing segment (B-006 dodge).
                if *level > 0 {
                    let prefix = "../".repeat((*level - 1) as usize);
                    let module_path = if module.is_empty() {
                        // `from . import foo` — target the index of the
                        // current package directory.
                        format!("./{}", prefix.trim_end_matches('/'))
                            .trim_end_matches('.')
                            .to_string()
                    } else {
                        format!("./{}{}", prefix, module.replace('.', "/"))
                    };
                    let import_names: Vec<String> = names
                        .iter()
                        .map(|a| {
                            if let Some(alias) = &a.alias {
                                format!("{} as {}", a.name, alias)
                            } else {
                                a.name.clone()
                            }
                        })
                        .collect();
                    for a in names {
                        let local = a.alias.as_deref().unwrap_or(&a.name);
                        self.declare(local);
                    }
                    // SECURITY (A2): relative specifier is source-derived
                    // (dotted module name) — escape it defensively so no import
                    // specifier is ever built by raw interpolation.
                    self.writeln(&format!(
                        "import {{ {} }} from {};",
                        import_names.join(", "),
                        js_string_literal(&module_path)
                    ));
                    return;
                }

                // Skip compile-time-only imports
                if module == "dataclasses" || module == "pydantic" || module == "typing" {
                    return;
                }

                // #105: `from pyths import <name>` — bare `pyths` is not an
                // npm package, so the only meaningful reading is as an alias
                // of `import <name>` for known stdlib modules (mirroring the
                // dotted `pyths.<name>` form). Anything else previously
                // compiled cleanly to `import { x } from "pyths"` — an
                // unresolvable specifier that failed at runtime with
                // ERR_MODULE_NOT_FOUND (silent miscompile).
                if module == "pyths" {
                    for a in names {
                        let local = a.alias.as_deref().unwrap_or(&a.name);
                        self.declare(local);
                        if STDLIB_MODULES.contains(&a.name.as_str()) {
                            self.writeln(&format!(
                                "import * as {} from \"pyths-runtime/stdlib/{}\";",
                                local, a.name
                            ));
                        } else {
                            let diag = format!(
                                "`from pyths import {}`: `{}` is not a PythScribe stdlib module. \
                                 Supported forms: `import {}` or `from pyths import <stdlib>` \
                                 (stdlib: {}), `import pyths.<web>` (dom, fetch, storage, router), \
                                 or `from pyths.<module> import ...`.",
                                a.name,
                                a.name,
                                a.name,
                                STDLIB_MODULES.join(", ")
                            );
                            eprintln!("error: {}", diag);
                            self.codegen_errors.push(diag.clone());
                            self.writeln(&format!(
                                "throw new Error({:?});",
                                format!("PythScribe: {}", diag)
                            ));
                        }
                    }
                    return;
                }
                let is_react_module = is_react_or_next_module(module);

                // Special case: `from pyths.react import ...` is a HYBRID — some
                // names live in the `react` npm package (hooks, React APIs) and
                // others live in `pyths-runtime/react` (codegen-meta helpers like
                // `component`, `psx`, `style`, `classes`). Conflating them into a
                // single `from "pyths-runtime/react"` import fails at bundle time
                // because the runtime can't expose React without forcing it as a
                // hard dep (which breaks non-React consumers of pyths-runtime).
                // Split into two import statements when both groups appear.
                if module == "pyths.react" {
                    let mut react_names: Vec<String> = Vec::new();
                    let mut runtime_names: Vec<String> = Vec::new();
                    for a in names {
                        let js_name = react::snake_to_camel(&a.name);
                        let with_alias = if let Some(alias) = &a.alias {
                            format!("{} as {}", js_name, alias)
                        } else {
                            js_name
                        };
                        if react::is_react_core_export(&a.name) {
                            react_names.push(with_alias);
                        } else {
                            runtime_names.push(with_alias);
                        }
                    }
                    // Track declarations + PSX dispatch hints (same as the
                    // general path below).
                    for a in names {
                        let local = a.alias.as_deref().unwrap_or(&a.name);
                        self.declare(local);
                        if a.alias.is_none() {
                            self.react_imports.insert(a.name.clone());
                        }
                    }
                    if !react_names.is_empty() {
                        self.writeln(&format!(
                            "import {{ {} }} from \"react\";",
                            react_names.join(", ")
                        ));
                    }
                    if !runtime_names.is_empty() {
                        self.writeln(&format!(
                            "import {{ {} }} from \"pyths-runtime/react\";",
                            runtime_names.join(", ")
                        ));
                    }
                    return;
                }

                // #274: dedupe by JS binding — Python allows re-importing a name
                // (idempotent), but a second ES `import { X }` is a SyntaxError.
                // Drop names whose binding was already imported at module scope.
                let import_names: Vec<String> = names
                    .iter()
                    .filter_map(|a| {
                        let js_name = if is_react_module {
                            react::snake_to_camel(&a.name)
                        } else {
                            a.name.clone()
                        };
                        let binding = a.alias.clone().unwrap_or_else(|| js_name.clone());
                        if !self.imported_bindings.insert(binding.clone()) {
                            return None;
                        }
                        Some(if let Some(alias) = &a.alias {
                            format!("{} as {}", js_name, alias)
                        } else {
                            js_name
                        })
                    })
                    .collect();
                // Every name was already imported — emit nothing (the bindings
                // are all in scope). Side-effect tracking below still runs.
                if import_names.is_empty() {
                    return;
                }
                // Mark imported names as declared. For React-like
                // modules, also track the Python-source name in
                // `react_imports`. This serves two purposes:
                //
                //   1. Bare references with underscores get camelCased
                //      so the JS binding matches (`use_state` →
                //      `useState`). The transform at emit_expr no-ops
                //      for names without underscores, so tracking
                //      them is harmless for that case.
                //
                //   2. Inside @component, PSX-mode call dispatch
                //      consults this set to disambiguate React-hook
                //      calls from HTML-tag calls when the name collides
                //      (e.g., React 19's `use()` vs SVG `<use>`).
                //
                // User aliases bypass tracking — the alias is what the
                // user wants emitted.
                for a in names {
                    let local = a.alias.as_deref().unwrap_or(&a.name);
                    self.declare(local);
                    if is_react_module && a.alias.is_none() {
                        self.react_imports.insert(a.name.clone());
                    }
                    // Track-B: remember the LOCAL binding (alias included) so
                    // PSX props on these library components get snake→camel'd,
                    // and so `motion.div`-style lowercase members dispatch as
                    // components.
                    if is_react_module {
                        self.react_lib_bindings.insert(local.to_string());
                        if a.name == "motion" {
                            self.react_member_component_bases.insert(local.to_string());
                        }
                    }
                    // Round-4 sweep: `from asyncio import run` — track the
                    // local binding so `run(...)` call-sites get awaited.
                    if (module == "asyncio" || module == "pyths.asyncio") && a.name == "run" {
                        self.asyncio_run_fns.insert(local.to_string());
                    }
                }
                let module_path = self.resolve_module(module);
                // SECURITY (A2): module_path may be a verbatim `[npm.imports]`
                // override value — config-derived, untrusted. Escape it.
                self.writeln(&format!(
                    "import {{ {} }} from {};",
                    import_names.join(", "),
                    js_string_literal(&module_path)
                ));
            }
            StmtKind::Try {
                body,
                handlers,
                else_body,
                finally_body,
            } => {
                self.emit_try(body, handlers, else_body, finally_body);
            }
            StmtKind::Raise(value, cause) => {
                // Auto-import the exception class when raising a known Python
                // builtin (parallels how `len()` use auto-imports `pyLen`).
                // Without this, `raise ValueError("x")` compiles to
                // `throw new ValueError("x")` but ValueError is never imported
                // from pyths-runtime → ReferenceError at runtime.
                if let Some(expr) = value {
                    if let ExprKind::Call { func, .. } = &expr.kind {
                        if let ExprKind::Name(name) = &func.kind {
                            self.import_builtin_exception(name);
                        }
                    }
                }
                if let Some(c) = cause {
                    // `raise X from Y` — Python instantiates a bare class
                    // cause too; both operands may need their runtime import.
                    if let ExprKind::Call { func, .. } = &c.kind {
                        if let ExprKind::Name(name) = &func.kind {
                            self.import_builtin_exception(name);
                        }
                    } else if let ExprKind::Name(name) = &c.kind {
                        self.import_builtin_exception(name);
                    }
                }

                match value {
                    None => {
                        // Bare `raise` — re-raise the active exception. The
                        // enclosing handler always binds it as `__exc`, so a
                        // plain rethrow is exact. (Previously emitted `throw;`
                        // — a JS SyntaxError; found by the round-4 sweep.)
                        self.write_indent();
                        self.write("throw __exc;\n");
                    }
                    Some(expr) => {
                        self.write_indent();
                        self.write("throw ");
                        if cause.is_some() {
                            // `raise X from Y` (PEP 3134) — chain the cause
                            // onto the thrown object as `__cause__`, matching
                            // CPython's attribute of the same name.
                            self.write("Object.assign(");
                        }
                        // A raise operand is NEVER a JSX element — inside a
                        // @component, PSX mode would otherwise turn a capitalized
                        // exception constructor into createElement(Exception, ...)
                        // (found by the Coursera clone: `raise Exception("x")` in a
                        // component threw a React element). Suspend PSX so the
                        // call lowers as a constructor / plain expression.
                        let prev_in_component = self.in_component;
                        self.in_component = false;
                        self.emit_raise_operand(expr);
                        if let Some(c) = cause {
                            self.write(", { __cause__: ");
                            self.emit_raise_operand(c);
                            self.write(" })");
                        }
                        self.in_component = prev_in_component;
                        self.write(";\n");
                    }
                }
            }
            StmtKind::Assert { test, msg } => {
                // Match Python's `AssertionError` semantics: throw an
                // Error whose `.name` reads `"AssertionError"` so
                // `try/except AssertionError` (lowered to
                // `catch (e) { if (e.name === "AssertionError") ... }`)
                // matches it, while regular `instanceof Error` still
                // works for generic catch blocks.
                self.write_indent();
                self.write("if (!(");
                self.emit_expr(test);
                self.write(")) { throw Object.assign(new Error(");
                if let Some(m) = msg {
                    self.emit_expr(m);
                } else {
                    self.write("\"Assertion failed\"");
                }
                self.write("), { name: \"AssertionError\" }); }\n");
            }
            StmtKind::Global(_) | StmtKind::Nonlocal(_) => {
                // These are scope declarations — handled by name resolution in later phases
            }
            StmtKind::Del(targets) => {
                // #101: shape-dispatch per target kind.
                //   del d[k] / del xs[i] → pyDelItem(obj, key)  (dict-key
                //     delete with KeyError; list splice — not a JS hole —
                //     with IndexError; Map delete)
                //   del obj.attr         → delete obj.attr;
                //   del x (bare name)    → x = undefined;  (a bare
                //     `delete x` is a strict-mode SyntaxError; Python's
                //     unbind-the-name has no direct JS equivalent)
                for target in targets {
                    match &target.kind {
                        // #321: `del xs[a:b]` slice-delete — clamp OOB bounds
                        // per CPython slice.indices (the pySlice/pySetSlice
                        // sibling on the DELETE path) instead of routing a
                        // Slice through pyDelItem (which sees `null` and raises
                        // a spurious IndexError). Simple + extended slices.
                        ExprKind::Subscript {
                            value,
                            index,
                            optional: false,
                        } if matches!(&index.kind, ExprKind::Slice { .. }) => {
                            if let ExprKind::Slice { lower, upper, step } = &index.kind {
                                self.need_runtime("pyDelSlice");
                                self.write_indent();
                                self.write("pyDelSlice(");
                                self.emit_expr(value);
                                for bound in [lower, upper, step] {
                                    self.write(", ");
                                    match bound {
                                        Some(e) => self.emit_expr(e),
                                        None => self.write("null"),
                                    }
                                }
                                self.write(");\n");
                            }
                        }
                        ExprKind::Subscript { value, index, .. } => {
                            self.need_runtime("pyDelItem");
                            self.write_indent();
                            self.write("pyDelItem(");
                            self.emit_expr(value);
                            self.write(", ");
                            self.emit_expr(index);
                            self.write(");\n");
                        }
                        ExprKind::Attribute { value, attr, .. } => {
                            self.write_indent();
                            self.write("delete ");
                            self.emit_expr(value);
                            self.write(&format!(".{};\n", attr));
                        }
                        ExprKind::Name(name) => {
                            self.write_indent();
                            self.write(&format!("{} = undefined;\n", Self::sanitize_ident(name)));
                        }
                        _ => {
                            self.write_indent();
                            self.write("delete ");
                            self.emit_expr(target);
                            self.write(";\n");
                        }
                    }
                }
            }
            StmtKind::AnnAssign { target, value, .. } => {
                // Annotated assignment: emit as regular assignment, strip type annotation
                if let Some(val) = value {
                    self.emit_assign(std::slice::from_ref(target), val);
                }
                // If no value, it's just a type declaration — skip in JS
            }
            StmtKind::With {
                items,
                body,
                is_async,
            } => {
                self.emit_with(items, body, *is_async);
            }
            StmtKind::Match { subject, cases } => {
                self.emit_match(subject, cases);
            }
        }
    }

    /// Emit the elements of a destructuring PATTERN (`[a, [b, c], ...rest]`),
    /// recursing into nested tuple/list targets (#85 — these previously
    /// mis-emitted as `pyTuple(b, c)` value expressions). Names are
    /// sanitized; non-Name targets (subscript/attribute members) emit as
    /// plain LHS expressions, which JS destructuring assignment accepts.
    fn emit_destructure_pattern(&mut self, elts: &[Expr]) {
        self.write("[");
        for (i, elt) in elts.iter().enumerate() {
            if i > 0 {
                self.write(", ");
            }
            match &elt.kind {
                ExprKind::Name(n) => {
                    self.write(&Self::sanitize_ident(n));
                }
                ExprKind::Tuple(inner) | ExprKind::List(inner) => {
                    self.emit_destructure_pattern(inner);
                }
                ExprKind::Starred(inner) => {
                    self.write("...");
                    self.emit_expr(inner);
                }
                _ => {
                    let was_lhs = self.in_lhs_target;
                    self.in_lhs_target = true;
                    self.emit_expr(elt);
                    self.in_lhs_target = was_lhs;
                }
            }
        }
        self.write("]");
    }

    /// Collect every plain Name bound by an unpack target (recursive).
    fn collect_pattern_names(elts: &[Expr], out: &mut Vec<String>) {
        for elt in elts {
            match &elt.kind {
                ExprKind::Name(n) => out.push(n.clone()),
                ExprKind::Tuple(inner) | ExprKind::List(inner) => {
                    Self::collect_pattern_names(inner, out);
                }
                ExprKind::Starred(inner) => {
                    if let ExprKind::Name(n) = &inner.kind {
                        out.push(n.clone());
                    }
                }
                _ => {}
            }
        }
    }

    fn emit_assign(&mut self, targets: &[Expr], value: &Expr) {
        // #99: chained assignment `a = b = expr` — evaluate the RHS ONCE
        // (Python semantics), then assign each target left-to-right. For a
        // non-trivial RHS, stash it in a hidden const so side effects run
        // exactly once; trivial RHS (literal / bare name) re-emits inline.
        if targets.len() > 1 {
            let trivial = matches!(
                &value.kind,
                ExprKind::Name(_)
                    | ExprKind::IntLiteral(_)
                    | ExprKind::FloatLiteral(_)
                    | ExprKind::StringLiteral(_)
                    | ExprKind::BoolLiteral(_)
                    | ExprKind::NoneLiteral
            );
            if trivial {
                for t in targets {
                    self.emit_assign(std::slice::from_ref(t), value);
                }
            } else {
                let tmp = format!("__chain_{}", self.default_hoist_counter);
                self.default_hoist_counter += 1;
                self.write_indent();
                self.write(&format!("const {} = ", tmp));
                self.emit_expr(value);
                self.write(";\n");
                let tmp_expr = Expr {
                    kind: ExprKind::Name(tmp),
                    span: value.span,
                };
                for t in targets {
                    self.emit_assign(std::slice::from_ref(t), &tmp_expr);
                }
            }
            return;
        }

        // Pre-compute inferred type of the RHS once — used for both the
        // top-level Name target and tuple-unpack element widening.
        let value_ty = self.infer_type(value);

        // Handle tuple/list unpacking: a, b = expr. Emitted as a
        // destructuring ASSIGNMENT (`([a, b] = expr);`) with any
        // first-time names pre-declared via `let` — so reassignment of
        // already-declared names works (#84: the swap idiom `a, b = b, a`
        // previously re-declared with `const` → SyntaxError) and nested
        // patterns recurse (#85).
        // #106.1: dict literal assigned to a poisoned name → force the
        // Map-backed shape at construction.
        if let ExprKind::Name(n) = &targets[0].kind {
            if self.pydict_forced_locals.contains(n)
                && (matches!(&value.kind, ExprKind::Dict { .. }) || Self::is_empty_dict_ctor(value))
            {
                self.force_pydict_literal = true;
            }
        }

        if let ExprKind::Tuple(elts) | ExprKind::List(elts) = &targets[0].kind {
            // #106.2: JS destructuring patterns cannot route subscript
            // elements through pySetItem — `[d[2], xs[0]] = rhs` sets a
            // useless own property on a Map-backed dict. Evaluate the
            // RHS once into a temp, then assign element-wise through the
            // single-target path (which shape-dispatches). Star patterns
            // keep the destructuring form (slice semantics).
            let has_star = elts.iter().any(|e| matches!(e.kind, ExprKind::Starred(_)));
            let has_subscript = elts
                .iter()
                .any(|e| matches!(e.kind, ExprKind::Subscript { .. }));
            if has_subscript && !has_star {
                let hoist = self.default_hoist_counter;
                self.default_hoist_counter += 1;
                let tmp = format!("__unpack{}", hoist);
                self.write_indent();
                self.write(&format!("const {} = ", tmp));
                self.emit_expr(value);
                self.write(
                    ";
",
                );
                for (i, elt) in elts.iter().enumerate() {
                    let tmp_elem = Expr {
                        kind: ExprKind::Subscript {
                            value: Box::new(Expr {
                                kind: ExprKind::Name(tmp.clone()),
                                span: value.span,
                            }),
                            index: Box::new(Expr {
                                kind: ExprKind::IntLiteral(i as i128),
                                span: value.span,
                            }),
                            optional: false,
                        },
                        span: value.span,
                    };
                    self.emit_assign(std::slice::from_ref(elt), &tmp_elem);
                }
                return;
            }
            // Round-2 pythonic sweep: a starred target anywhere but LAST
            // cannot destructure in JS (`[a, ...mid, z] =` — rest must be
            // final). Evaluate the RHS once into a Python-iterated array,
            // then assign element-wise: pre-star by index, the star by a
            // negative-stop Python slice, trailing elements by negative
            // index (both shape-dispatch through the normal single-target
            // path).
            let star_idx = elts
                .iter()
                .position(|e| matches!(e.kind, ExprKind::Starred(_)));
            if let Some(si) = star_idx {
                if si + 1 != elts.len() {
                    let hoist = self.default_hoist_counter;
                    self.default_hoist_counter += 1;
                    let tmp = format!("__unpack{}", hoist);
                    self.write_indent();
                    self.write(&format!("const {} = ", tmp));
                    self.emit_iterable_as_array(value);
                    self.write(";\n");
                    let len = elts.len();
                    let post = (len - si - 1) as i64;
                    let span = value.span;
                    let mk = |kind: ExprKind| Expr { kind, span };
                    for (i, elt) in elts.iter().enumerate() {
                        if let ExprKind::Starred(inner) = &elt.kind {
                            let rhs = mk(ExprKind::Subscript {
                                value: Box::new(mk(ExprKind::Name(tmp.clone()))),
                                index: Box::new(mk(ExprKind::Slice {
                                    lower: Some(Box::new(mk(ExprKind::IntLiteral(si as i128)))),
                                    upper: Some(Box::new(mk(ExprKind::IntLiteral(
                                        -(post as i128),
                                    )))),
                                    step: None,
                                })),
                                optional: false,
                            });
                            self.emit_assign(std::slice::from_ref(inner.as_ref()), &rhs);
                        } else {
                            let idx = if i < si {
                                i as i64
                            } else {
                                i as i64 - len as i64
                            };
                            let rhs = mk(ExprKind::Subscript {
                                value: Box::new(mk(ExprKind::Name(tmp.clone()))),
                                index: Box::new(mk(ExprKind::IntLiteral(idx as i128))),
                                optional: false,
                            });
                            self.emit_assign(std::slice::from_ref(elt), &rhs);
                        }
                    }
                    return;
                }
            }
            let mut names = Vec::new();
            Self::collect_pattern_names(elts, &mut names);
            for n in &names {
                if !self.is_declared(n) {
                    self.write_indent();
                    self.write(&format!("let {};\n", Self::sanitize_ident(n)));
                    self.declare(n);
                }
            }
            self.write_indent();
            self.write("(");
            self.emit_destructure_pattern(elts);
            self.write(" = ");
            self.emit_expr(value);
            self.write(");\n");
            // #227: propagate per-element inferred types for a matching-arity
            // literal RHS, so `a, b = -1.0, 1.0` records `a`/`b` as Float and
            // `print(a)` keeps the `.0` (a whole float and int are the same JS
            // number at runtime).
            if let ExprKind::Tuple(vals) | ExprKind::List(vals) = &value.kind {
                if vals.len() == elts.len() {
                    for (t, v) in elts.iter().zip(vals) {
                        if let ExprKind::Name(n) = &t.kind {
                            let vt = if self.is_definitely_float(v) {
                                JsInferredType::Float
                            } else {
                                self.infer_type(v)
                            };
                            self.record_type(n, vt);
                        }
                    }
                }
            }
            return;
        }

        self.write_indent();

        let target = &targets[0];

        // `__default__ = Name` (module level) → `export default Name;`.
        // PythScribe has no `export default` syntax, but ES-module default
        // exports are mandatory for some consumers — notably Next.js App
        // Router `page`/`layout` modules, which resolve the route component
        // from the module's default export. Pair it with the named
        // `@component` (`def Page(): ...; __default__ = Page`).
        if self.indent == 0 {
            if let ExprKind::Name(name) = &target.kind {
                if name == "__default__" {
                    self.write("export default ");
                    self.emit_expr(value);
                    self.write(";\n");
                    return;
                }
            }
        }

        // #81: `obj.__proto__ = v` / `self.__proto__ = v` invokes the REAL
        // Object.prototype.__proto__ accessor setter, which silently
        // ignores non-object values — the F3 dict-literal proto fix has an
        // attribute-assignment sibling here. defineProperty creates a
        // normal own data property instead.
        if let ExprKind::Attribute {
            value: obj, attr, ..
        } = &target.kind
        {
            if attr == "__proto__" {
                self.write("Object.defineProperty(");
                self.emit_expr(obj);
                self.write(", \"__proto__\", { value: ");
                self.emit_expr(value);
                self.write(", writable: true, enumerable: true, configurable: true });\n");
                return;
            }
        }

        if let ExprKind::Name(name) = &target.kind {
            // First assignment → `let`, subsequent → plain reassignment.
            // Module-scope (indent 0) names export so other `.ps`/`.js` modules
            // can import top-level constants — completes the export model
            // alongside module-level classes/functions (B-015). Nested
            // assignments (locals, indent > 0) stay bare.
            if !self.is_declared(name) {
                if self.indent == 0 {
                    self.write("export ");
                }
                self.write("let ");
                self.declare(name);
            }
            // NOTE: the RHS type is recorded AFTER the value is emitted (below),
            // not here — recording it now would let a self-referential RHS read
            // the new type. e.g. `s = [x for x in s]` (str→list): iterating the
            // OLD `s` (a str) must still pySeq-wrap, so `s` must stay Unknown
            // while the comprehension is emitted (#272).
        }
        // #83: subscript WRITES on Dict/Unknown receivers route through the
        // shape-dispatching pySetItem — raw `d[k] = v` on a Map-backed dict
        // would set a useless own property instead of a Map entry, and on a
        // plain dict a non-string key would still stringify. List/Tuple
        // receivers keep the bare native write (hot path, unchanged).
        // #219: slice assignment `l[a:b] = xs` / `l[::k] = xs`. A raw
        // `pySlice(...) = xs` is an invalid JS assignment target; route to the
        // in-place pySetSlice (splice for step 1, element-wise for extended).
        if let ExprKind::Subscript {
            value: recv,
            index,
            optional,
        } = &target.kind
        {
            if !*optional {
                if let ExprKind::Slice { lower, upper, step } = &index.kind {
                    self.need_runtime("pySetSlice");
                    self.write("pySetSlice(");
                    self.emit_expr(recv);
                    for bound in [lower, upper, step] {
                        self.write(", ");
                        match bound {
                            Some(e) => self.emit_expr(e),
                            None => self.write("null"),
                        }
                    }
                    self.write(", ");
                    self.emit_expr(value);
                    self.write(");\n");
                    return;
                }
            }
        }
        if let ExprKind::Subscript {
            value: recv,
            index,
            optional,
        } = &target.kind
        {
            if !*optional && !matches!(&index.kind, ExprKind::Slice { .. }) {
                // crit-7: every subscript write routes through pySetItem, which
                // bounds-checks lists (a=[1]; a[1]=2 raises IndexError instead
                // of extending), rejects tuple assignment (immutable), and
                // handles dicts + negative/computed indices. A non-negative
                // literal index is NOT provably in-bounds, so the old bare
                // native-store fast path (#278) was unsound. Hot loops use
                // computed indices, which already routed here.
                {
                    self.need_runtime("pySetItem");
                    self.write("pySetItem(");
                    self.emit_expr(recv);
                    self.write(", ");
                    self.emit_expr(index);
                    self.write(", ");
                    self.emit_expr(value);
                    self.write(");\n");
                    return;
                }
            }
        }
        self.in_lhs_target = true;
        self.emit_expr(target);
        self.in_lhs_target = false;
        self.write(" = ");
        self.emit_expr(value);
        self.write(";\n");
        // #272: record the RHS's coarse type now — AFTER the value is emitted —
        // so a self-referential RHS (`s = [x for x in s]`) sees `s`'s prior type
        // while emitting, not the type being assigned. Later `if x:` / `x == y`
        // emission still gets the right wrap decision.
        if let ExprKind::Name(name) = &target.kind {
            self.record_type(name, value_ty);
        }
    }

    /// B-023: collect local names whose *first* assignment in a function body
    /// is inside a nested control-flow block (depth > 0). Python function-scopes
    /// these, but emitting `let` at the (nested) first-assignment site would
    /// block-scope them in JS, breaking later use. Names first-assigned at the
    /// top level are excluded (the existing inline-`let` behavior is fine there).
    /// #199: names declared `global`/`nonlocal` anywhere in a function body
    /// (recursing through control-flow blocks but NOT into nested defs/classes,
    /// which open their own scope). Assignments to these rebind the outer
    /// binding, so they must be marked already-declared before the local-hoist
    /// pass — otherwise codegen emits a shadowing `let` (TDZ / lost mutation).
    fn collect_global_names(body: &[Stmt]) -> Vec<String> {
        fn walk(stmts: &[Stmt], out: &mut Vec<String>) {
            for s in stmts {
                match &s.kind {
                    StmtKind::Global(names) | StmtKind::Nonlocal(names) => {
                        for n in names {
                            if !out.contains(n) {
                                out.push(n.clone());
                            }
                        }
                    }
                    StmtKind::If {
                        body,
                        elif_clauses,
                        else_body,
                        ..
                    } => {
                        walk(body, out);
                        for (_, b) in elif_clauses {
                            walk(b, out);
                        }
                        if let Some(b) = else_body {
                            walk(b, out);
                        }
                    }
                    StmtKind::While {
                        body, else_body, ..
                    }
                    | StmtKind::For {
                        body, else_body, ..
                    } => {
                        walk(body, out);
                        if let Some(b) = else_body {
                            walk(b, out);
                        }
                    }
                    StmtKind::With { body, .. } => walk(body, out),
                    StmtKind::Try {
                        body,
                        handlers,
                        else_body,
                        finally_body,
                    } => {
                        walk(body, out);
                        for h in handlers {
                            walk(&h.body, out);
                        }
                        if let Some(b) = else_body {
                            walk(b, out);
                        }
                        if let Some(b) = finally_body {
                            walk(b, out);
                        }
                    }
                    StmtKind::Match { cases, .. } => {
                        for c in cases {
                            walk(&c.body, out);
                        }
                    }
                    // FuncDef / ClassDef open a fresh scope — their `global`s
                    // rebind *their* enclosing scope, not this body's. Do not
                    // descend.
                    _ => {}
                }
            }
        }
        let mut out = Vec::new();
        walk(body, &mut out);
        out
    }

    /// #262: does the loop body reassign any of this for-target's names? If so
    /// the loop binding must be `let` (reassignable), not `const`.
    fn for_target_reassigned(target: &Expr, body: &[Stmt]) -> bool {
        fn names(e: &Expr, out: &mut Vec<String>) {
            match &e.kind {
                ExprKind::Name(n) => out.push(n.clone()),
                ExprKind::Tuple(elts) | ExprKind::List(elts) => {
                    for x in elts {
                        names(x, out);
                    }
                }
                ExprKind::Starred(inner) => names(inner, out),
                _ => {}
            }
        }
        let mut tn = Vec::new();
        names(target, &mut tn);
        if tn.is_empty() {
            return false;
        }
        let reassigned = Self::reassigned_names(body);
        tn.iter().any(|n| reassigned.contains(n))
    }

    /// #220: names bound by a plain/aug/annotated assignment anywhere in this
    /// body (recursing through control-flow blocks but NOT into nested
    /// defs/classes). Used to decide whether a for-loop variable is *reused* as
    /// an ordinary variable and therefore must be function-scope hoisted.
    /// For-loop targets themselves are deliberately excluded.
    fn reassigned_names(body: &[Stmt]) -> std::collections::HashSet<String> {
        fn names_of(expr: &Expr, out: &mut std::collections::HashSet<String>) {
            match &expr.kind {
                ExprKind::Name(n) => {
                    out.insert(n.clone());
                }
                ExprKind::Tuple(elts) | ExprKind::List(elts) => {
                    for e in elts {
                        names_of(e, out);
                    }
                }
                ExprKind::Starred(inner) => names_of(inner, out),
                _ => {}
            }
        }
        fn walk(stmts: &[Stmt], out: &mut std::collections::HashSet<String>) {
            for s in stmts {
                match &s.kind {
                    StmtKind::Assign { targets, .. } => {
                        for t in targets {
                            names_of(t, out);
                        }
                    }
                    StmtKind::AugAssign { target, .. }
                    | StmtKind::AnnAssign {
                        target,
                        value: Some(_),
                        ..
                    } => names_of(target, out),
                    StmtKind::If {
                        body,
                        elif_clauses,
                        else_body,
                        ..
                    } => {
                        walk(body, out);
                        for (_, b) in elif_clauses {
                            walk(b, out);
                        }
                        if let Some(b) = else_body {
                            walk(b, out);
                        }
                    }
                    StmtKind::While {
                        body, else_body, ..
                    }
                    | StmtKind::For {
                        body, else_body, ..
                    } => {
                        walk(body, out);
                        if let Some(b) = else_body {
                            walk(b, out);
                        }
                    }
                    StmtKind::With { body, .. } => walk(body, out),
                    StmtKind::Try {
                        body,
                        handlers,
                        else_body,
                        finally_body,
                    } => {
                        walk(body, out);
                        for h in handlers {
                            walk(&h.body, out);
                        }
                        if let Some(b) = else_body {
                            walk(b, out);
                        }
                        if let Some(b) = finally_body {
                            walk(b, out);
                        }
                    }
                    StmtKind::Match { cases, .. } => {
                        for c in cases {
                            walk(&c.body, out);
                        }
                    }
                    _ => {}
                }
            }
        }
        let mut out = std::collections::HashSet::new();
        walk(body, &mut out);
        out
    }

    /// #325: names that are ONLY ever augmented-assigned (`x += …`) in this
    /// function body and never given a plain binding — plain assign, for/with
    /// target, ann-assign, walrus, except-as, match capture, or a nested
    /// def/class name. Python makes any assignment (aug included) a function
    /// local, so such a name is an UNBOUND local: reading it (which `x += …`
    /// does) raises UnboundLocalError. Without this the codegen treats the
    /// aug-assign as a closure write to an enclosing binding of the same name
    /// and silently produces a value. Nested def/class bodies are their own
    /// scope and are not descended into. Over-counting "bound" is safe (it
    /// just declines to sentinel); under-counting would risk a false
    /// UnboundLocalError, so every binding form is folded into `bound`.
    fn aug_only_locals(body: &[Stmt]) -> Vec<String> {
        use std::collections::HashSet;
        fn names_of(e: &Expr, out: &mut HashSet<String>) {
            match &e.kind {
                ExprKind::Name(n) => {
                    out.insert(n.clone());
                }
                ExprKind::Tuple(xs) | ExprKind::List(xs) => {
                    for x in xs {
                        names_of(x, out);
                    }
                }
                ExprKind::Starred(i) => names_of(i, out),
                _ => {}
            }
        }
        // Walrus targets bind in the enclosing (function) scope (PEP 572).
        fn walrus_of(e: &Expr, out: &mut HashSet<String>) {
            match &e.kind {
                ExprKind::NamedExpr { target, value } => {
                    if let ExprKind::Name(n) = &target.kind {
                        out.insert(n.clone());
                    }
                    walrus_of(value, out);
                }
                ExprKind::BinOp { left, right, .. } => {
                    walrus_of(left, out);
                    walrus_of(right, out);
                }
                ExprKind::UnaryOp { operand, .. } => walrus_of(operand, out),
                ExprKind::Compare { left, comparisons } => {
                    walrus_of(left, out);
                    for (_, x) in comparisons {
                        walrus_of(x, out);
                    }
                }
                ExprKind::Call {
                    func, args, kwargs, ..
                } => {
                    walrus_of(func, out);
                    for a in args {
                        walrus_of(a, out);
                    }
                    for k in kwargs {
                        walrus_of(&k.value, out);
                    }
                }
                ExprKind::IfExpr {
                    test,
                    body,
                    else_body,
                } => {
                    walrus_of(test, out);
                    walrus_of(body, out);
                    walrus_of(else_body, out);
                }
                ExprKind::List(xs) | ExprKind::Tuple(xs) | ExprKind::Set(xs) => {
                    for x in xs {
                        walrus_of(x, out);
                    }
                }
                ExprKind::Subscript { value, index, .. } => {
                    walrus_of(value, out);
                    walrus_of(index, out);
                }
                ExprKind::Attribute { value, .. } => walrus_of(value, out),
                _ => {}
            }
        }
        fn pat_captures(p: &Pattern, out: &mut HashSet<String>) {
            match p {
                Pattern::Capture(n) => {
                    out.insert(n.clone());
                }
                Pattern::As { pattern, name } => {
                    out.insert(name.clone());
                    pat_captures(pattern, out);
                }
                Pattern::Star(Some(n)) => {
                    out.insert(n.clone());
                }
                Pattern::Sequence(ps) | Pattern::Or(ps) => {
                    for q in ps {
                        pat_captures(q, out);
                    }
                }
                Pattern::Class { args, .. } => {
                    for q in args {
                        pat_captures(q, out);
                    }
                }
                Pattern::Mapping(pairs) => {
                    for (_, q) in pairs {
                        pat_captures(q, out);
                    }
                }
                _ => {}
            }
        }
        fn walk(stmts: &[Stmt], aug: &mut Vec<String>, bound: &mut HashSet<String>) {
            for s in stmts {
                match &s.kind {
                    StmtKind::AugAssign { target, value, .. } => {
                        if let ExprKind::Name(n) = &target.kind {
                            if !aug.contains(n) {
                                aug.push(n.clone());
                            }
                        } else {
                            names_of(target, bound);
                        }
                        walrus_of(value, bound);
                    }
                    StmtKind::Assign { targets, value } => {
                        for t in targets {
                            names_of(t, bound);
                            walrus_of(t, bound);
                        }
                        walrus_of(value, bound);
                    }
                    StmtKind::AnnAssign { target, value, .. } => {
                        if value.is_some() {
                            names_of(target, bound);
                        }
                        if let Some(v) = value {
                            walrus_of(v, bound);
                        }
                    }
                    StmtKind::Expr(e) | StmtKind::Return(Some(e)) => walrus_of(e, bound),
                    StmtKind::For {
                        target,
                        iter,
                        body,
                        else_body,
                        ..
                    } => {
                        names_of(target, bound);
                        walrus_of(iter, bound);
                        walk(body, aug, bound);
                        if let Some(e) = else_body {
                            walk(e, aug, bound);
                        }
                    }
                    StmtKind::If {
                        test,
                        body,
                        elif_clauses,
                        else_body,
                    } => {
                        walrus_of(test, bound);
                        walk(body, aug, bound);
                        for (c, b) in elif_clauses {
                            walrus_of(c, bound);
                            walk(b, aug, bound);
                        }
                        if let Some(e) = else_body {
                            walk(e, aug, bound);
                        }
                    }
                    StmtKind::While {
                        test,
                        body,
                        else_body,
                    } => {
                        walrus_of(test, bound);
                        walk(body, aug, bound);
                        if let Some(e) = else_body {
                            walk(e, aug, bound);
                        }
                    }
                    StmtKind::With { items, body, .. } => {
                        for it in items {
                            walrus_of(&it.context_expr, bound);
                            if let Some(v) = &it.optional_var {
                                names_of(v, bound);
                            }
                        }
                        walk(body, aug, bound);
                    }
                    StmtKind::Try {
                        body,
                        handlers,
                        else_body,
                        finally_body,
                    } => {
                        walk(body, aug, bound);
                        for h in handlers {
                            if let Some(n) = &h.name {
                                bound.insert(n.clone());
                            }
                            walk(&h.body, aug, bound);
                        }
                        if let Some(e) = else_body {
                            walk(e, aug, bound);
                        }
                        if let Some(f) = finally_body {
                            walk(f, aug, bound);
                        }
                    }
                    StmtKind::Match { subject, cases } => {
                        walrus_of(subject, bound);
                        for c in cases {
                            pat_captures(&c.pattern, bound);
                            walk(&c.body, aug, bound);
                        }
                    }
                    // Nested scopes: the def/class NAME binds here, but its
                    // body is a separate scope — do not descend.
                    StmtKind::FuncDef { name, .. } | StmtKind::ClassDef { name, .. } => {
                        bound.insert(name.clone());
                    }
                    _ => {}
                }
            }
        }
        let mut aug: Vec<String> = Vec::new();
        let mut bound: HashSet<String> = HashSet::new();
        walk(body, &mut aug, &mut bound);
        aug.into_iter().filter(|n| !bound.contains(n)).collect()
    }

    /// Returns each hoist-eligible name with a `promoted` flag: `true` when
    /// the name's first binding is a depth-0 statement (`x = 5` before a
    /// loop that rebinds it) — see the #288 promotion pass at the bottom.
    /// Promoted module-scope names must keep the `export` their inline
    /// first assignment would have carried.
    fn collect_hoisted_names(body: &[Stmt]) -> Vec<(String, bool)> {
        fn record(seen: &mut Vec<(String, u32)>, name: &str, depth: u32) {
            if !seen.iter().any(|(n, _)| n == name) {
                seen.push((name.to_string(), depth));
            }
        }
        fn target_names(expr: &Expr, out: &mut Vec<String>) {
            match &expr.kind {
                ExprKind::Name(n) => out.push(n.clone()),
                ExprKind::Tuple(elts) | ExprKind::List(elts) => {
                    for e in elts {
                        target_names(e, out);
                    }
                }
                ExprKind::Starred(inner) => target_names(inner, out),
                _ => {} // attribute / subscript targets don't bind a new local
            }
        }
        fn assign_targets(target: &Expr, depth: u32, seen: &mut Vec<(String, u32)>) {
            let mut names = Vec::new();
            target_names(target, &mut names);
            for n in names {
                record(seen, &n, depth);
            }
        }
        // Walrus (`NamedExpr`) targets assign in EXPRESSION position, so no
        // statement ever emits their `let` — they must always be hoisted
        // (recorded at depth 1 so the `d > 0` filter below keeps them).
        // This also matches PEP 572 scoping: `:=` inside a comprehension
        // binds the enclosing function/module scope, which the hoisted
        // declaration plus the arrow-closure assignment reproduces.
        fn walrus_in_expr(expr: &Expr, seen: &mut Vec<(String, u32)>) {
            match &expr.kind {
                ExprKind::NamedExpr { target, value } => {
                    if let ExprKind::Name(n) = &target.kind {
                        record(seen, n, 1);
                    }
                    walrus_in_expr(value, seen);
                }
                ExprKind::BinOp { left, right, .. } => {
                    walrus_in_expr(left, seen);
                    walrus_in_expr(right, seen);
                }
                ExprKind::UnaryOp { operand, .. } => walrus_in_expr(operand, seen),
                ExprKind::Compare { left, comparisons } => {
                    walrus_in_expr(left, seen);
                    for (_, e) in comparisons {
                        walrus_in_expr(e, seen);
                    }
                }
                ExprKind::Call {
                    func, args, kwargs, ..
                } => {
                    walrus_in_expr(func, seen);
                    for a in args {
                        walrus_in_expr(a, seen);
                    }
                    for k in kwargs {
                        walrus_in_expr(&k.value, seen);
                    }
                }
                ExprKind::Attribute { value, .. } => walrus_in_expr(value, seen),
                ExprKind::Subscript { value, index, .. } => {
                    walrus_in_expr(value, seen);
                    walrus_in_expr(index, seen);
                }
                ExprKind::Slice { lower, upper, step } => {
                    for e in [lower, upper, step].into_iter().flatten() {
                        walrus_in_expr(e, seen);
                    }
                }
                ExprKind::List(elts) | ExprKind::Tuple(elts) | ExprKind::Set(elts) => {
                    for e in elts {
                        walrus_in_expr(e, seen);
                    }
                }
                ExprKind::Dict { items } => {
                    for item in items {
                        match item {
                            DictItem::KeyValue { key, value } => {
                                walrus_in_expr(key, seen);
                                walrus_in_expr(value, seen);
                            }
                            DictItem::Spread(e) => walrus_in_expr(e, seen),
                        }
                    }
                }
                ExprKind::FString { parts } => {
                    for p in parts {
                        if let FStringPart::Expr(e) = p {
                            walrus_in_expr(e, seen);
                        }
                    }
                }
                ExprKind::ListComp { elt, generators }
                | ExprKind::SetComp { elt, generators }
                | ExprKind::GeneratorExp { elt, generators } => {
                    walrus_in_expr(elt, seen);
                    for g in generators {
                        walrus_in_expr(&g.iter, seen);
                        for c in &g.ifs {
                            walrus_in_expr(c, seen);
                        }
                    }
                }
                ExprKind::DictComp {
                    key,
                    value,
                    generators,
                } => {
                    walrus_in_expr(key, seen);
                    walrus_in_expr(value, seen);
                    for g in generators {
                        walrus_in_expr(&g.iter, seen);
                        for c in &g.ifs {
                            walrus_in_expr(c, seen);
                        }
                    }
                }
                ExprKind::Lambda { params, body } => {
                    // A lambda is its own scope in Python, but hoisting the
                    // rare lambda-body walrus at the enclosing scope is
                    // still preferable to an undeclared strict-mode
                    // assignment (ReferenceError).
                    for p in params {
                        if let Some(d) = &p.default {
                            walrus_in_expr(d, seen);
                        }
                    }
                    walrus_in_expr(body, seen);
                }
                ExprKind::IfExpr {
                    test,
                    body,
                    else_body,
                } => {
                    walrus_in_expr(test, seen);
                    walrus_in_expr(body, seen);
                    walrus_in_expr(else_body, seen);
                }
                ExprKind::Starred(e) | ExprKind::Await(e) | ExprKind::YieldFrom(e) => {
                    walrus_in_expr(e, seen)
                }
                ExprKind::Yield(Some(e)) => {
                    walrus_in_expr(e, seen);
                }
                _ => {}
            }
        }
        // Scan the expressions directly attached to one statement (nested
        // statement bodies are reached by `walk`'s own recursion).
        fn walrus_in_stmt(stmt: &Stmt, seen: &mut Vec<(String, u32)>) {
            match &stmt.kind {
                StmtKind::Expr(e) | StmtKind::Return(Some(e)) | StmtKind::Raise(Some(e), _) => {
                    walrus_in_expr(e, seen)
                }
                StmtKind::Assign { targets, value } => {
                    for t in targets {
                        walrus_in_expr(t, seen);
                    }
                    walrus_in_expr(value, seen);
                }
                StmtKind::AugAssign { target, value, .. } => {
                    walrus_in_expr(target, seen);
                    walrus_in_expr(value, seen);
                }
                StmtKind::AnnAssign { value: Some(v), .. } => walrus_in_expr(v, seen),
                StmtKind::If {
                    test, elif_clauses, ..
                } => {
                    walrus_in_expr(test, seen);
                    for (c, _) in elif_clauses {
                        walrus_in_expr(c, seen);
                    }
                }
                StmtKind::While { test, .. } => walrus_in_expr(test, seen),
                StmtKind::For { iter, .. } => walrus_in_expr(iter, seen),
                StmtKind::With { items, .. } => {
                    for it in items {
                        walrus_in_expr(&it.context_expr, seen);
                    }
                }
                StmtKind::Match { subject, cases } => {
                    walrus_in_expr(subject, seen);
                    for c in cases {
                        if let Some(g) = &c.guard {
                            walrus_in_expr(g, seen);
                        }
                    }
                }
                StmtKind::Assert { test, msg } => {
                    walrus_in_expr(test, seen);
                    if let Some(m) = msg {
                        walrus_in_expr(m, seen);
                    }
                }
                StmtKind::Del(exprs) => {
                    for e in exprs {
                        walrus_in_expr(e, seen);
                    }
                }
                _ => {}
            }
        }
        struct WalkCtx {
            seen: Vec<(String, u32)>,
            /// #288: for-target names that are "reused" (reassigned or
            /// leaked-read elsewhere) — candidates for depth-0 promotion.
            promote: std::collections::HashSet<String>,
            /// Names bound by a def/class in this body: never promoted
            /// (`let f;` + `function f` is a JS SyntaxError).
            defclass: std::collections::HashSet<String>,
        }
        fn walk(
            stmts: &[Stmt],
            depth: u32,
            ctx: &mut WalkCtx,
            reassigned: &std::collections::HashSet<String>,
        ) {
            for stmt in stmts {
                walrus_in_stmt(stmt, &mut ctx.seen);
                match &stmt.kind {
                    StmtKind::Assign { targets, .. } => {
                        for t in targets {
                            assign_targets(t, depth, &mut ctx.seen);
                        }
                    }
                    StmtKind::AnnAssign {
                        target,
                        value: Some(_),
                        ..
                    } => assign_targets(target, depth, &mut ctx.seen),
                    StmtKind::AugAssign { target, .. } => {
                        assign_targets(target, depth, &mut ctx.seen)
                    }
                    StmtKind::For {
                        target,
                        body,
                        else_body,
                        ..
                    } => {
                        // #220: a for-loop target is emitted as a block-scoped
                        // `const`, but Python scopes it to the enclosing function
                        // and it persists after the loop. If the SAME name is also
                        // assigned elsewhere in this body (`for i in ...:` then
                        // `i = 0`), the block-scoped `const` leaves that reuse
                        // binding-less (ReferenceError). Hoist to a function-scope
                        // `let` (recorded at depth+1) only in that reuse case; a
                        // loop variable that is never reassigned keeps its plain
                        // per-iteration `const` (no output churn).
                        let mut tnames = Vec::new();
                        target_names(target, &mut tnames);
                        let reused = tnames.iter().any(|n| reassigned.contains(n));
                        if reused {
                            // #288 (B): remember every name of a reused
                            // for-target — if its FIRST binding was a depth-0
                            // statement it still needs hoisting, or the loop
                            // shadows the real binding (promotion pass below).
                            for n in &tnames {
                                ctx.promote.insert(n.clone());
                            }
                        }
                        assign_targets(
                            target,
                            if reused { depth + 1 } else { depth },
                            &mut ctx.seen,
                        );
                        walk(body, depth + 1, ctx, reassigned);
                        if let Some(e) = else_body {
                            walk(e, depth + 1, ctx, reassigned);
                        }
                    }
                    StmtKind::If {
                        body,
                        elif_clauses,
                        else_body,
                        ..
                    } => {
                        walk(body, depth + 1, ctx, reassigned);
                        for (_, eb) in elif_clauses {
                            walk(eb, depth + 1, ctx, reassigned);
                        }
                        if let Some(e) = else_body {
                            walk(e, depth + 1, ctx, reassigned);
                        }
                    }
                    StmtKind::While {
                        body, else_body, ..
                    } => {
                        walk(body, depth + 1, ctx, reassigned);
                        if let Some(e) = else_body {
                            walk(e, depth + 1, ctx, reassigned);
                        }
                    }
                    StmtKind::Try {
                        body,
                        handlers,
                        else_body,
                        finally_body,
                    } => {
                        walk(body, depth + 1, ctx, reassigned);
                        for h in handlers {
                            if let Some(n) = &h.name {
                                record(&mut ctx.seen, n, depth + 1);
                            }
                            walk(&h.body, depth + 1, ctx, reassigned);
                        }
                        if let Some(e) = else_body {
                            walk(e, depth + 1, ctx, reassigned);
                        }
                        if let Some(f) = finally_body {
                            walk(f, depth + 1, ctx, reassigned);
                        }
                    }
                    StmtKind::With { items, body, .. } => {
                        for item in items {
                            if let Some(v) = &item.optional_var {
                                assign_targets(v, depth, &mut ctx.seen);
                            }
                        }
                        walk(body, depth + 1, ctx, reassigned);
                    }
                    StmtKind::Match { cases, .. } => {
                        for case in cases {
                            walk(&case.body, depth + 1, ctx, reassigned);
                        }
                    }
                    // Nested defs/classes have their own scope; the bound name
                    // is recorded at the current depth but we don't recurse.
                    StmtKind::FuncDef { name, .. } | StmtKind::ClassDef { name, .. } => {
                        record(&mut ctx.seen, name, depth);
                        ctx.defclass.insert(name.clone());
                    }
                    _ => {}
                }
            }
        }
        let mut ctx = WalkCtx {
            seen: Vec::new(),
            promote: std::collections::HashSet::new(),
            defclass: std::collections::HashSet::new(),
        };
        let mut reassigned = Self::reassigned_names(body);
        // #269 (R17): a for-loop target that is READ outside its own loop leaks
        // out of the loop in Python with its final value. Fold those names into
        // the same hoist channel as reassigned ones so they get a function/module
        // `let`; emit_for then writes that binding instead of a fresh `const`.
        for n in Self::leaked_for_read_names(body) {
            reassigned.insert(n);
        }
        walk(body, 0, &mut ctx, &reassigned);
        // #288 (B): a reused for-target whose FIRST binding is a depth-0
        // statement (`x = 5` before `for x in …`) was recorded at depth 0, so
        // it never hoisted and emit_for shadowed the real binding with a
        // per-iteration `const`. Promote it to hoist depth: the hoisted `let`
        // takes over the declaration (the inline assignment then emits bare)
        // and the loop writes the function/module-scope binding. Def/class
        // names are excluded — `let f;` + `function f` is a JS SyntaxError.
        let mut promoted: std::collections::HashSet<String> = std::collections::HashSet::new();
        for (n, d) in ctx.seen.iter_mut() {
            if *d == 0 && ctx.promote.contains(n) && !ctx.defclass.contains(n) {
                *d = 1;
                promoted.insert(n.clone());
            }
        }
        ctx.seen
            .into_iter()
            .filter(|(_, d)| *d > 0)
            .map(|(n, _)| {
                let p = promoted.contains(&n);
                (n, p)
            })
            .collect()
    }

    /// #269 (R17): for-loop target names that are READ somewhere outside the
    /// loop that binds them. Python scopes a loop variable to the enclosing
    /// function/module and leaks its final value after the loop, so such a name
    /// must be hoisted and the loop must assign the hoisted binding (bare
    /// `for (i of ...)`), not a per-iteration `const`. A read that occurs INSIDE
    /// the binding loop is covered by that loop's own scope and does NOT count —
    /// this keeps the common `for i in xs: use(i)` case emitting a plain `const`.
    /// Nested def/class bodies are their own scope and are not descended into
    /// (matching `reassigned_names`).
    fn leaked_for_read_names(body: &[Stmt]) -> std::collections::HashSet<String> {
        use std::collections::HashSet;
        fn names_in_expr(e: &Expr, out: &mut Vec<String>) {
            match &e.kind {
                ExprKind::Name(n) => out.push(n.clone()),
                ExprKind::BinOp { left, right, .. } => {
                    names_in_expr(left, out);
                    names_in_expr(right, out);
                }
                ExprKind::UnaryOp { operand, .. } => names_in_expr(operand, out),
                ExprKind::Compare { left, comparisons } => {
                    names_in_expr(left, out);
                    for (_, e) in comparisons {
                        names_in_expr(e, out);
                    }
                }
                ExprKind::Call {
                    func, args, kwargs, ..
                } => {
                    names_in_expr(func, out);
                    for a in args {
                        names_in_expr(a, out);
                    }
                    for k in kwargs {
                        names_in_expr(&k.value, out);
                    }
                }
                ExprKind::Attribute { value, .. } => names_in_expr(value, out),
                ExprKind::Subscript { value, index, .. } => {
                    names_in_expr(value, out);
                    names_in_expr(index, out);
                }
                ExprKind::Slice { lower, upper, step } => {
                    for e in [lower, upper, step].into_iter().flatten() {
                        names_in_expr(e, out);
                    }
                }
                ExprKind::List(xs) | ExprKind::Tuple(xs) | ExprKind::Set(xs) => {
                    for e in xs {
                        names_in_expr(e, out);
                    }
                }
                ExprKind::Dict { items } => {
                    for it in items {
                        match it {
                            DictItem::KeyValue { key, value } => {
                                names_in_expr(key, out);
                                names_in_expr(value, out);
                            }
                            DictItem::Spread(e) => names_in_expr(e, out),
                        }
                    }
                }
                ExprKind::FString { parts } => {
                    for p in parts {
                        if let FStringPart::Expr(e) = p {
                            names_in_expr(e, out);
                        }
                    }
                }
                ExprKind::ListComp { elt, generators }
                | ExprKind::SetComp { elt, generators }
                | ExprKind::GeneratorExp { elt, generators } => {
                    names_in_expr(elt, out);
                    for g in generators {
                        names_in_expr(&g.iter, out);
                        for c in &g.ifs {
                            names_in_expr(c, out);
                        }
                    }
                }
                ExprKind::DictComp {
                    key,
                    value,
                    generators,
                } => {
                    names_in_expr(key, out);
                    names_in_expr(value, out);
                    for g in generators {
                        names_in_expr(&g.iter, out);
                        for c in &g.ifs {
                            names_in_expr(c, out);
                        }
                    }
                }
                ExprKind::Lambda { params, body } => {
                    for p in params {
                        if let Some(d) = &p.default {
                            names_in_expr(d, out);
                        }
                    }
                    names_in_expr(body, out);
                }
                ExprKind::IfExpr {
                    test,
                    body,
                    else_body,
                } => {
                    names_in_expr(test, out);
                    names_in_expr(body, out);
                    names_in_expr(else_body, out);
                }
                ExprKind::Starred(e) | ExprKind::Await(e) | ExprKind::YieldFrom(e) => {
                    names_in_expr(e, out)
                }
                ExprKind::Yield(Some(e)) => {
                    names_in_expr(e, out);
                }
                ExprKind::NamedExpr { target, value } => {
                    names_in_expr(target, out);
                    names_in_expr(value, out);
                }
                _ => {}
            }
        }
        fn target_names(e: &Expr, out: &mut Vec<String>) {
            match &e.kind {
                ExprKind::Name(n) => out.push(n.clone()),
                ExprKind::Tuple(elts) | ExprKind::List(elts) => {
                    for x in elts {
                        target_names(x, out);
                    }
                }
                ExprKind::Starred(inner) => target_names(inner, out),
                _ => {}
            }
        }
        // Reads from the *evaluated* part of an assignment target: a plain Name
        // target is a write (skip), but `a[i]` / `a.b` evaluate `a` (and `i`).
        fn target_reads(e: &Expr, out: &mut Vec<String>) {
            match &e.kind {
                ExprKind::Subscript { value, index, .. } => {
                    names_in_expr(value, out);
                    names_in_expr(index, out);
                }
                ExprKind::Attribute { value, .. } => names_in_expr(value, out),
                ExprKind::Tuple(elts) | ExprKind::List(elts) => {
                    for x in elts {
                        target_reads(x, out);
                    }
                }
                ExprKind::Starred(inner) => target_reads(inner, out),
                _ => {}
            }
        }
        fn record(e: &Expr, active: &[String], leaked: &mut HashSet<String>) {
            let mut ns = Vec::new();
            names_in_expr(e, &mut ns);
            for n in ns {
                if !active.contains(&n) {
                    leaked.insert(n);
                }
            }
        }
        fn record_reads(reads: &[String], active: &[String], leaked: &mut HashSet<String>) {
            for n in reads {
                if !active.contains(n) {
                    leaked.insert(n.clone());
                }
            }
        }
        fn walk(stmts: &[Stmt], active: &mut Vec<String>, leaked: &mut HashSet<String>) {
            for s in stmts {
                match &s.kind {
                    StmtKind::Expr(e) | StmtKind::Return(Some(e)) => record(e, active, leaked),
                    StmtKind::Raise(e1, e2) => {
                        for e in [e1, e2].into_iter().flatten() {
                            record(e, active, leaked);
                        }
                    }
                    StmtKind::Assign { targets, value } => {
                        record(value, active, leaked);
                        for t in targets {
                            let mut r = Vec::new();
                            target_reads(t, &mut r);
                            record_reads(&r, active, leaked);
                        }
                    }
                    StmtKind::AugAssign { target, value, .. } => {
                        // `a += b` reads `a` too.
                        record(target, active, leaked);
                        record(value, active, leaked);
                    }
                    StmtKind::AnnAssign {
                        target,
                        value: Some(v),
                        ..
                    } => {
                        record(v, active, leaked);
                        let mut r = Vec::new();
                        target_reads(target, &mut r);
                        record_reads(&r, active, leaked);
                    }
                    StmtKind::Assert { test, msg } => {
                        record(test, active, leaked);
                        if let Some(m) = msg {
                            record(m, active, leaked);
                        }
                    }
                    StmtKind::Del(exprs) => {
                        for e in exprs {
                            record(e, active, leaked);
                        }
                    }
                    StmtKind::If {
                        test,
                        body,
                        elif_clauses,
                        else_body,
                    } => {
                        record(test, active, leaked);
                        walk(body, active, leaked);
                        for (c, b) in elif_clauses {
                            record(c, active, leaked);
                            walk(b, active, leaked);
                        }
                        if let Some(b) = else_body {
                            walk(b, active, leaked);
                        }
                    }
                    StmtKind::While {
                        test,
                        body,
                        else_body,
                    } => {
                        record(test, active, leaked);
                        walk(body, active, leaked);
                        if let Some(b) = else_body {
                            walk(b, active, leaked);
                        }
                    }
                    StmtKind::For {
                        target,
                        iter,
                        body,
                        else_body,
                        ..
                    } => {
                        // The iterable is evaluated in the enclosing scope.
                        record(iter, active, leaked);
                        let mut tn = Vec::new();
                        target_names(target, &mut tn);
                        let added = tn.len();
                        for n in &tn {
                            active.push(n.clone());
                        }
                        walk(body, active, leaked);
                        for _ in 0..added {
                            active.pop();
                        }
                        // The for-`else` block is emitted OUTSIDE the loop's JS
                        // block scope, so a target read there is NOT covered by
                        // the loop binding — walk it with the target inactive.
                        if let Some(b) = else_body {
                            walk(b, active, leaked);
                        }
                    }
                    StmtKind::With { items, body, .. } => {
                        for it in items {
                            record(&it.context_expr, active, leaked);
                        }
                        walk(body, active, leaked);
                    }
                    StmtKind::Try {
                        body,
                        handlers,
                        else_body,
                        finally_body,
                    } => {
                        walk(body, active, leaked);
                        for h in handlers {
                            walk(&h.body, active, leaked);
                        }
                        if let Some(b) = else_body {
                            walk(b, active, leaked);
                        }
                        if let Some(b) = finally_body {
                            walk(b, active, leaked);
                        }
                    }
                    StmtKind::Match { subject, cases } => {
                        record(subject, active, leaked);
                        for c in cases {
                            if let Some(g) = &c.guard {
                                record(g, active, leaked);
                            }
                            walk(&c.body, active, leaked);
                        }
                    }
                    // Nested def/class = own scope; not descended into.
                    _ => {}
                }
            }
        }
        // Only names actually bound by some for-loop in this body matter; a
        // leaked read of a non-loop name is handled by ordinary first-assignment.
        fn collect_for_targets(stmts: &[Stmt], out: &mut HashSet<String>) {
            for s in stmts {
                match &s.kind {
                    StmtKind::For {
                        target,
                        body,
                        else_body,
                        ..
                    } => {
                        let mut tn = Vec::new();
                        target_names(target, &mut tn);
                        for n in tn {
                            out.insert(n);
                        }
                        collect_for_targets(body, out);
                        if let Some(b) = else_body {
                            collect_for_targets(b, out);
                        }
                    }
                    StmtKind::If {
                        body,
                        elif_clauses,
                        else_body,
                        ..
                    } => {
                        collect_for_targets(body, out);
                        for (_, b) in elif_clauses {
                            collect_for_targets(b, out);
                        }
                        if let Some(b) = else_body {
                            collect_for_targets(b, out);
                        }
                    }
                    StmtKind::While {
                        body, else_body, ..
                    } => {
                        collect_for_targets(body, out);
                        if let Some(b) = else_body {
                            collect_for_targets(b, out);
                        }
                    }
                    StmtKind::With { body, .. } => collect_for_targets(body, out),
                    StmtKind::Try {
                        body,
                        handlers,
                        else_body,
                        finally_body,
                    } => {
                        collect_for_targets(body, out);
                        for h in handlers {
                            collect_for_targets(&h.body, out);
                        }
                        if let Some(b) = else_body {
                            collect_for_targets(b, out);
                        }
                        if let Some(b) = finally_body {
                            collect_for_targets(b, out);
                        }
                    }
                    StmtKind::Match { cases, .. } => {
                        for c in cases {
                            collect_for_targets(&c.body, out);
                        }
                    }
                    _ => {}
                }
            }
        }
        let mut leaked = HashSet::new();
        let mut active = Vec::new();
        walk(body, &mut active, &mut leaked);
        let mut for_targets = HashSet::new();
        collect_for_targets(body, &mut for_targets);
        leaked.retain(|n| for_targets.contains(n));
        leaked
    }

    /// PBT-2 / #288: hoisted for-loop targets eligible for the __UNBOUND
    /// sentinel — leaked (read outside their binding loop, per
    /// `leaked_for_read_names`) AND every for-target pattern binding them
    /// consists entirely of names in `hoisted` (the `collect_hoisted_names`
    /// output for this body). That guarantees every binding loop emits a
    /// bare write that clears the sentinel; a loop that still declares its
    /// own per-iteration binding (any pattern-mate not hoisted) would leave
    /// the sentinel set after a nonempty loop → false raise, so such names
    /// are excluded. #288 extended bare writes to tuple/list destructuring
    /// targets, so compound patterns qualify like simple names now.
    fn sentinel_for_names(
        body: &[Stmt],
        hoisted: &std::collections::HashSet<String>,
    ) -> std::collections::HashSet<String> {
        fn collect(stmts: &[Stmt], groups: &mut Vec<Vec<String>>) {
            for s in stmts {
                match &s.kind {
                    StmtKind::For {
                        target,
                        body,
                        else_body,
                        ..
                    } => {
                        match &target.kind {
                            ExprKind::Name(n) => {
                                groups.push(vec![n.clone()]);
                            }
                            ExprKind::Tuple(elts) | ExprKind::List(elts) => {
                                let mut tn = Vec::new();
                                JsCodegen::collect_pattern_names(elts, &mut tn);
                                groups.push(tn);
                            }
                            _ => {}
                        }
                        collect(body, groups);
                        if let Some(b) = else_body {
                            collect(b, groups);
                        }
                    }
                    StmtKind::If {
                        body,
                        elif_clauses,
                        else_body,
                        ..
                    } => {
                        collect(body, groups);
                        for (_, b) in elif_clauses {
                            collect(b, groups);
                        }
                        if let Some(b) = else_body {
                            collect(b, groups);
                        }
                    }
                    StmtKind::While {
                        body, else_body, ..
                    } => {
                        collect(body, groups);
                        if let Some(b) = else_body {
                            collect(b, groups);
                        }
                    }
                    StmtKind::With { body, .. } => collect(body, groups),
                    StmtKind::Try {
                        body,
                        handlers,
                        else_body,
                        finally_body,
                    } => {
                        collect(body, groups);
                        for h in handlers {
                            collect(&h.body, groups);
                        }
                        if let Some(b) = else_body {
                            collect(b, groups);
                        }
                        if let Some(b) = finally_body {
                            collect(b, groups);
                        }
                    }
                    StmtKind::Match { cases, .. } => {
                        for c in cases {
                            collect(&c.body, groups);
                        }
                    }
                    _ => {}
                }
            }
        }
        let mut groups: Vec<Vec<String>> = Vec::new();
        collect(body, &mut groups);
        let mut out = Self::leaked_for_read_names(body);
        out.retain(|n| {
            groups
                .iter()
                .filter(|g| g.iter().any(|m| m == n))
                .all(|g| g.iter().all(|m| hoisted.contains(m)))
        });
        out
    }

    fn emit_func_def(
        &mut self,
        name: &str,
        params: &[Param],
        body: &[Stmt],
        decorator_list: &[Expr],
        is_async: bool,
    ) {
        // Check for @component decorator
        let is_component = decorator_list
            .iter()
            .any(|d| matches!(&d.kind, ExprKind::Name(n) if n == "component"));

        // B-029 follow-up (C): @handler at module level → auto-emit
        // `export default { fetch: <fnName> }` after the function body.
        // The decorator is consumed by codegen (stripped from the
        // runtime-application loop below) so the emitted JS never
        // calls an undefined `handler` symbol.  The function itself
        // compiles as a plain named function — the default export
        // wires it directly without wrapping.
        let is_handler = decorator_list
            .iter()
            .any(|d| matches!(&d.kind, ExprKind::Name(n) if n == "handler"));

        // React Refresh emission gate: must be enabled, must be a
        // `@component`, and the name must start with an uppercase
        // letter (the React convention for components). Lowercase
        // functions — even decorated `@component` — are not treated
        // as components by React, so emitting Refresh boilerplate for
        // them would just be dead code.
        // #168: module-level components only — babel-plugin-react-refresh
        // likewise skips nested components (they're re-created per render,
        // so registering them with the Refresh runtime is meaningless).
        let emit_refresh = self.react_refresh
            && is_component
            && self.indent == 0
            && name.chars().next().is_some_and(|c| c.is_ascii_uppercase());
        let refresh_sig = if emit_refresh {
            refresh_hook_signature(body)
        } else {
            String::new()
        };
        let refresh_local = if emit_refresh {
            format!("_s_{}", name)
        } else {
            String::new()
        };
        // Check for @psx decorator. `@psx` is a lighter-weight opt-in:
        // it enables PSX-mode emission (HTML element calls become
        // `createElement(...)`) without imposing @component's named
        // export, kwargs-as-props destructuring, or known-class call
        // disambiguation. Use it for render-prop helpers, HOCs, and
        // any utility function that builds JSX subtrees.
        let is_psx = decorator_list
            .iter()
            .any(|d| matches!(&d.kind, ExprKind::Name(n) if n == "psx"));
        // #121: a CAPITALIZED def with no @component/@psx whose return
        // expression is an UNDECLARED known-HTML-tag call is unmistakably
        // a PSX component helper — without the transform, the emitted
        // `section(...)` is a guaranteed ReferenceError at mount (the
        // behavioral oracle's movie_rows failure). Treat it as @psx (PSX
        // mode only; none of @component's export/props machinery). Any
        // real binding for the tag name (import, def, local) defeats the
        // "undeclared" test, so valid non-PSX code cannot be claimed.
        let is_implicit_psx = !is_component
            && !is_psx
            && name
                .chars()
                .next()
                .is_some_and(|ch| ch.is_ascii_uppercase())
            && self.body_returns_unbound_html_call(body);
        let is_psx = is_psx || is_implicit_psx;

        // Check for Next.js export function names
        let js_name = react::nextjs_export_mapping(name)
            .map(|s| s.to_string())
            .unwrap_or_else(|| Self::sanitize_ident(name).into_owned());

        let is_nextjs_export = react::is_nextjs_export(name);

        // React Refresh: emit the per-component signature hook before
        // the function declaration. Hoisting to module top isn't
        // required — `const _s_X = $RefreshSig$()` is a regular
        // statement that just needs to precede the function's first
        // call to `_s_X()`.
        if emit_refresh {
            self.write_indent();
            self.write(&format!("const {} = $RefreshSig$();\n", refresh_local));
        }

        // F6: hoist each default param expression to a once-evaluated const so
        // Python's "defaults evaluated once at def time" semantics hold (a bare
        // JS default param re-runs the expression every call — `def f(xs=[])`
        // would hand out a fresh list per call). Component params destructure
        // props with defaults that are re-created per render (correct React
        // behavior), so they're intentionally skipped.
        self.param_default_hoists.clear();
        if !is_component {
            for param in params {
                if param.is_args || param.is_kwargs || param.name == "self" || param.name == "cls" {
                    continue;
                }
                if let Some(default) = &param.default {
                    let hidden = format!("__def${}_{}", self.default_hoist_counter, param.name);
                    self.default_hoist_counter += 1;
                    self.write_indent();
                    self.write(&format!("const {} = ", hidden));
                    self.emit_expr(default);
                    self.write(";\n");
                    self.param_default_hoists.insert(param.name.clone(), hidden);
                }
            }
        }

        // Emit export prefix for module-level functions (Python modules export
        // every top-level name; ESM needs it explicit), @component, or Next.js
        // functions. Named export (not `export default`) so multiple functions
        // coexist in one module — callers use `import { Foo } from "./mod.js"`.
        // Exceptions: nested defs (closures, indent > 0) stay local, and `@psx`
        // helpers are intentionally internal (test_psx_does_not_imply_export).
        // #168: the nested-def exception applies to @component / Next.js
        // functions too — `export` is only legal at module scope, so a nested
        // `@component` must compile as a local declaration (it still gets the
        // full component transform: PSX mode + props destructuring).
        // #350: a module-level redefinition (a second `def name` after the name
        // was already declared at module scope) is Python last-wins. Emit it as
        // a reassignment (`name = function …`) so JS doesn't reject a duplicate
        // `function name` declaration ("Identifier already declared"). The first
        // declaration stays a normal (hoisted) `export function name`.
        //
        // B18: the redefinition bookkeeping (`module_decl_names.insert`) MUST run
        // for @component / Next.js exports too. It used to be short-circuited
        // behind `!is_component && !is_nextjs_export` in this `&&` chain, so a
        // SECOND `@component def App` (or Next-exported def) never recorded the
        // name and emitted a duplicate `export function App` — invalid ESM
        // ("Identifier 'App' has already been declared"). Insert unconditionally
        // at module scope; a redefined component/export now reuses the
        // assignment form (`App = function …`) — it is already exported by its
        // first declaration.
        let redefine = self.indent == 0 && !self.module_decl_names.insert(name.to_string());
        self.write_indent();
        if !redefine && self.indent == 0 && (is_component || is_nextjs_export || !is_psx) {
            self.write("export ");
        }
        if redefine {
            self.write(&format!("{} = ", js_name));
        }
        let is_generator = body_contains_yield(body);
        if is_async {
            self.write("async ");
        }
        if is_generator {
            if redefine {
                self.write("function* (");
            } else {
                self.write(&format!("function* {}(", js_name));
            }
        } else if redefine {
            self.write("function (");
        } else {
            self.write(&format!("function {}(", js_name));
        }
        // @component props convention (#351, re-fixed after the #353 regression):
        //
        // Named params are PROP NAMES. `def Frontier(data):` is a component
        // with one prop named `data`; React calls the function positionally
        // with the flat props object (`createElement(Frontier, {data})`), so
        // the definition destructures it: `function Frontier({data} = {})`.
        // This holds at EVERY arity — including arity 1, which PR #353 wrongly
        // special-cased to positional binding (regressing every arity-1
        // named-prop component: `Frontier(data)` suddenly received the whole
        // props object and `data["points"]` keyed into `{data: …}`).
        //
        // The WHOLE-props-object convention is spelled unambiguously instead:
        //   * `def C(**props):` — Pythonic form; the single kwargs param binds
        //     the props object itself (`function C(props = {})`, via
        //     emit_params' kwargs lowering). Zero overlap with prop names.
        //   * `def C(props):` — pragmatic alias: a single no-default param
        //     LITERALLY named `props` also binds the whole object
        //     (`function C(props)`). This is what #351's consumers wrote and
        //     what a React author means by it. Corpus-checked: no component
        //     anywhere declares a PROP named "props". A DEFAULTED
        //     `def C(props=…)` stays a named-prop destructure.
        //
        // Everything else — multi named params, defaulted params, named params
        // + `**rest` — destructures the props object
        // (`function Header({title, on_click, ...rest} = {})`), the convention
        // verified by reference-app's 184 frontend tests + FlameReact 33/33.
        let effective_params: Vec<&pyths_syntax::ast::Param> = params
            .iter()
            .filter(|p| p.name != "self" && p.name != "cls")
            .collect();
        let whole_props_object = effective_params.len() == 1
            && !effective_params[0].is_args
            && effective_params[0].default.is_none()
            && (effective_params[0].is_kwargs || effective_params[0].name == "props");
        // React calls components as `Component(props)` with a single props
        // object. Emit the function as `function Header({title, refresh_at,
        // children} = {})` so the user's named params bind correctly.
        if is_component && !params.is_empty() && !whole_props_object {
            self.write("{");
            let mut first = true;
            for param in params {
                if param.name == "self" || param.name == "cls" || param.is_args || param.is_kwargs {
                    continue;
                }
                if !first {
                    self.write(", ");
                }
                first = false;
                self.write(&param.name);
                if let Some(default) = &param.default {
                    self.write(" = ");
                    self.emit_expr(default);
                }
            }
            // Track-B: `**rest` in a @component signature is a rest-
            // destructure of the props object. Previously dropped from the
            // pattern while the body still referenced the name — a
            // guaranteed ReferenceError on first render.
            for param in params {
                if param.is_kwargs {
                    if !first {
                        self.write(", ");
                    }
                    first = false;
                    self.write(&format!("...{}", param.name));
                }
            }
            self.write("} = {}");
        } else {
            self.emit_params(params);
        }
        // F6: consumed — clear so nested defs / methods / lambdas emitted
        // inside this body don't reference this function's hoisted consts.
        self.param_default_hoists.clear();
        self.write(") {\n");
        self.indent += 1;
        self.push_scope();
        // Enable PSX mode inside @component or @psx functions.
        // Both paths flip the same flag — the difference is the rest of
        // @component's machinery (export, props destructuring) doesn't
        // run for @psx, so a @psx helper is a regular function that
        // happens to emit JSX-style createElement calls.
        let prev_in_component = self.in_component;
        if is_component || is_psx {
            self.in_component = true;
        }
        // Declare parameters in function scope + seed their inferred
        // types from annotations (`: list`, `: dict`, etc.) so subscript/
        // equality/truthiness sites inside the body know the shape.
        for param in params {
            if param.name != "self" && param.name != "cls" {
                self.declare(&param.name);
            }
        }
        self.record_param_types(params);
        // React Refresh: `_s_X()` must be the first call inside the
        // function body so the signature runtime can capture the
        // hook-call order in this render. babel-plugin-react-refresh
        // emits the same shape.
        if emit_refresh {
            self.write_indent();
            self.write(&format!("{}();\n", refresh_local));
        }
        // #199: names declared `global`/`nonlocal` in this body rebind an
        // outer binding — mark them declared up front so neither the hoist
        // pass below nor a first inline assignment emits a shadowing `let`.
        for g in Self::collect_global_names(body) {
            self.declare(&g);
        }
        // B-023: hoist `let` for locals first-assigned inside a nested block so
        // they are function-scoped (Python semantics) rather than block-scoped.
        // PBT-2: sentinel-initialize hoisted for-targets with no other
        // guaranteed binding — reads route through __pyChkLocal so a
        // zero-iteration loop leaves them raising UnboundLocalError (CPython)
        // instead of reading as undefined→None.
        let hoisted_names = Self::collect_hoisted_names(body);
        let hoisted_set: HashSet<String> = hoisted_names.iter().map(|(n, _)| n.clone()).collect();
        let mut sentinels = Self::sentinel_for_names(body, &hoisted_set);
        // #288: a promoted name's depth-0 first assignment executes before
        // any of its loops — the binding is guaranteed, so no sentinel.
        for (n, promoted) in &hoisted_names {
            if *promoted {
                sentinels.remove(n);
            }
        }
        // #325: a name that is ONLY ever aug-assigned (never plainly bound) is
        // an unbound local — force it to sentinel even when it was hoisted via
        // an aug-assign inside a nested block (`try: y += 1`), so the guarded
        // read raises UnboundLocalError instead of poisoning with undefined.
        let aug_only: HashSet<String> = Self::aug_only_locals(body).into_iter().collect();
        for n in &aug_only {
            sentinels.insert(n.clone());
        }
        for (hoisted, _promoted) in hoisted_names {
            if !self.is_declared(&hoisted) {
                self.write_indent();
                if sentinels.contains(&hoisted) {
                    self.need_runtime("__UNBOUND");
                    self.write(&format!(
                        "let {} = __UNBOUND;\n",
                        Self::sanitize_ident(&hoisted)
                    ));
                    self.mark_sentinel(&hoisted);
                } else {
                    self.write(&format!("let {};\n", Self::sanitize_ident(&hoisted)));
                }
                self.declare(&hoisted);
            }
            // #269: genuine function-scope `let` (or a param) — bare for-target safe.
            self.mark_hoisted(&hoisted);
        }
        // #325: a name that is ONLY ever aug-assigned in this body (never
        // plainly bound) is an unbound function-local — `x += …` reads it
        // before it is ever set, which CPython raises UnboundLocalError for.
        // Sentinel-hoist it (shadowing any enclosing binding of the same
        // name) so the guarded read raises instead of silently mutating the
        // outer binding. Params / global / nonlocal names are already
        // declared, so the `!is_declared` guard skips them.
        for name in Self::aug_only_locals(body) {
            if !self.is_declared(&name) {
                self.need_runtime("__UNBOUND");
                self.write_indent();
                self.write(&format!(
                    "let {} = __UNBOUND;\n",
                    Self::sanitize_ident(&name)
                ));
                self.declare(&name);
                self.mark_sentinel(&name);
                self.mark_hoisted(&name);
            }
        }
        // Round-4 sweep: `await` is only legal inside this body if the
        // function is async (async generators included).
        let prev_await_ok = self.await_ok;
        self.await_ok = is_async;
        for stmt in body {
            self.emit_stmt(stmt);
        }
        self.await_ok = prev_await_ok;
        self.in_component = prev_in_component;
        self.pop_scope();
        self.indent -= 1;
        // #350: a redefinition is a `name = function(){…}` assignment statement,
        // which needs a terminating semicolon (a `function` declaration doesn't).
        self.writeln(if redefine { "};" } else { "}" });

        // Round-2 pythonic sweep: attach keyword-binding metadata so
        // __pyCallKw can map named/**-spread keyword arguments onto
        // positional parameter slots (Python calling convention). Plain
        // undecorated functions only — components/handlers/decorated
        // functions and *args signatures keep the legacy options-object
        // convention (JS can't positionally bind past a rest param).
        if !is_component
            && !is_handler
            && decorator_list.is_empty()
            && !params.iter().any(|p| p.is_args)
        {
            // B1: store the RAW Python param name. The binding is positional
            // by index (__pyKwArgs), and the call site emits raw keyword names
            // (emit_kwargs_value), so __pyparams__ must match those raw names —
            // NOT the sanitized JS parameter declaration. A param named like a
            // JS reserved word (`default`) is declared `default$` in the
            // signature but must appear as "default" here or every keyword call
            // misses (TypeError: unexpected keyword argument). Mirrors the
            // dataclass path, which already stores raw field names.
            let names: Vec<String> = params
                .iter()
                .filter(|p| !p.is_kwargs && p.name != "self" && p.name != "cls")
                .map(|p| format!("\"{}\"", p.name))
                .collect();
            let has_kw = params.iter().any(|p| p.is_kwargs);
            if !names.is_empty() || has_kw {
                self.write_indent();
                // B9: the assignment target must be the SAME emitted function
                // name used by the declaration above (js_name — the
                // nextjs_export_mapping / sanitized name computed once at the
                // top). Using sanitize_ident(name) here diverged for Next.js
                // renamed exports (`generate_metadata` declared as
                // `generateMetadata`) → ReferenceError at module load.
                self.write(&format!(
                    "{}.__pyparams__ = [{}];\n",
                    js_name,
                    names.join(", ")
                ));
                if has_kw {
                    self.write_indent();
                    self.write(&format!("{}.__pykw__ = true;\n", js_name));
                }
            }
        }

        // React Refresh: install the signature and register the
        // component type with the runtime. The signature string is
        // stable across renders of the same source; if the user
        // changes hook order or adds/removes a hook, the signature
        // changes and React safely remounts the component instead of
        // attempting state preservation against a mismatched layout.
        if emit_refresh {
            self.write_indent();
            self.write(&format!(
                "{}({}, {});\n",
                refresh_local,
                js_name,
                js_string_literal(&refresh_sig),
            ));
            self.write_indent();
            self.write(&format!(
                "$RefreshReg$({}, {});\n",
                js_name,
                js_string_literal(&js_name),
            ));
        }

        // Apply decorators (bottom-up), skipping codegen-meta ones
        // (@component, @psx, @staticmethod, @handler, @wasm) which don't have
        // runtime effect — they're consumed by the codegen and stripped
        // here so the emitted JS doesn't reference an undefined symbol.
        // @wasm is a compile-time WASM-placement assertion (#357), never a
        // runtime decorator; emitting `f = wasm(f)` would ReferenceError, and
        // the JS twin of an @wasm function must be plain JS.
        for decorator in decorator_list.iter().rev() {
            let skip = matches!(
                &decorator.kind,
                ExprKind::Name(n) if n == "component" || n == "psx" || n == "staticmethod" || n == "handler" || n == "wasm"
            );
            if !skip {
                self.write_indent();
                self.write(&format!("{} = ", js_name));
                self.emit_expr(decorator);
                self.write(&format!("({});\n", js_name));
            }
        }

        // B-029 follow-up (C): emit `export default { fetch: <fn> };`
        // for top-level @handler functions.  Only at module level
        // (self.indent == 0 after the closing `}`); nested @handler
        // is silently ignored (no sensible meaning).
        if is_handler && self.indent == 0 {
            self.write(&format!("export default {{ fetch: {} }};\n", js_name));
        }
    }

    fn emit_params(&mut self, params: &[Param]) {
        let mut first = true;
        for param in params {
            if param.name == "self" || param.name == "cls" {
                continue;
            }
            if !first {
                self.write(", ");
            }
            first = false;
            if param.is_args {
                self.write("...");
            }
            self.write(&Self::sanitize_ident(&param.name));
            if let Some(default) = &param.default {
                self.write(" = ");
                // F6: reference the once-evaluated hoisted const (set up in
                // emit_func_def) instead of re-emitting the default expression,
                // which JS would re-run on every defaulted call.
                if let Some(hidden) = self.param_default_hoists.get(&param.name).cloned() {
                    self.write(&hidden);
                } else {
                    self.emit_expr(default);
                }
            } else if param.is_kwargs {
                // `**kwargs` with no keyword arguments supplied is an
                // EMPTY dict in Python, not undefined (round-2 sweep).
                self.write(" = {}");
            }
        }
    }

    /// Seed the current scope's type tracker from parameter annotations.
    /// Called *after* `push_scope()` so the types land in the function's
    /// own scope, not the enclosing one. Recognizes `: list`/`: dict`/
    /// `: int` etc., plus `*args` (always List) and `**kwargs` (always Dict).
    fn record_param_types(&mut self, params: &[Param]) {
        for param in params {
            if param.name == "self" || param.name == "cls" {
                continue;
            }
            if param.is_args {
                self.record_type(&param.name, JsInferredType::List);
                continue;
            }
            if param.is_kwargs {
                self.record_type(&param.name, JsInferredType::Dict);
                continue;
            }
            if let Some(ann) = &param.annotation {
                let ty = self.js_type_from_annotation(ann);
                if !matches!(ty, JsInferredType::Unknown) {
                    self.record_type(&param.name, ty);
                }
            }
        }
    }

    fn emit_class_def(
        &mut self,
        name: &str,
        bases: &[Expr],
        body: &[Stmt],
        decorator_list: &[Expr],
    ) {
        // Check for @dataclass or @dataclass(frozen=True)
        let mut dc_opts = DataclassOptions::default();
        let is_dataclass = decorator_list.iter().any(|d| {
            let (is_dc, opts) = parse_dataclass_decorator(d);
            if is_dc {
                dc_opts = opts;
            }
            is_dc
        });

        // #350: a module-level class redefinition (a name already declared at
        // module scope) is Python last-wins — emit it as a `name = class …`
        // assignment so JS doesn't reject a duplicate `class name` declaration.
        let redefine = self.indent == 0 && !self.module_decl_names.insert(name.to_string());
        self.write_indent();
        // Module-level classes export (Python exports all top-level names);
        // nested classes (indent > 0) stay local.
        if !redefine && self.indent == 0 {
            self.write("export ");
        }
        if redefine {
            self.write(&format!("{} = class", Self::sanitize_ident(name)));
        } else {
            self.write(&format!("class {}", Self::sanitize_ident(name)));
        }
        // Does the first base extend a builtin JS-level class (Exception &
        // friends)? Those keep the native `extends` + native-`super()` path;
        // the cooperative-MRO machinery is only wired for pure-PythScribe
        // class hierarchies.
        let first_base_is_exception = bases
            .first()
            .map(|b| matches!(&b.kind, ExprKind::Name(n) if is_builtin_exception(n)))
            .unwrap_or(false);
        // A3: is the first base an EXTERNAL/native class — i.e. a `Name`
        // that isn't `class`-defined anywhere in this file (so it must have
        // come from an import: `Component` from react, a native `Error`
        // subclass, etc.)? The cooperative model relies on every class in
        // the chain routing construction through `PyObject`'s constructor,
        // which walks `new.target.__mro__` to dispatch `__init__`. A native
        // base's own constructor (e.g. `React.Component`) does no such
        // thing, so when `__init__` is emitted as a mixed-in prototype
        // method (not the JS `constructor`) it is simply never called —
        // verified empirically: `self.state` assigned in `__init__` stays
        // `undefined`/`null` at runtime (A3 finding). Treat an external
        // first base the same as an exception base: native `extends` +
        // native `super()` constructor.
        //
        // #300: a base imported RELATIVELY (`from .shape import Shape`) is
        // NOT external — it is another module of this same project, lowered
        // by this same compiler with the PyObject model. It must take the
        // same cooperative path as a same-file base; the native path emitted
        // a derived constructor with no `super()` ("Must call super
        // constructor" at `new`). Cross-module MRO works because the
        // imported class object carries its own `__mro__` (installed by
        // `__pyClass` in its defining module).
        let first_base_is_external_native = bases
            .first()
            .map(|b| {
                matches!(&b.kind, ExprKind::Name(n)
                    if !self.known_classes.contains(n)
                        && !self.local_module_imports.contains(n))
            })
            .unwrap_or(false);
        // Regular (non-dataclass, non-exception, non-external-native)
        // classes use the cooperative PyObject object model: `__init__`
        // becomes a prototype method dispatched via the MRO, so cooperative
        // `super().__init__()` works across multiple inheritance.
        // @dataclass keeps its generated constructor; exception subclasses
        // and external/native bases keep native `extends` + `constructor`.
        let pyobject_model =
            !is_dataclass && !first_base_is_exception && !first_base_is_external_native;
        if !bases.is_empty() {
            // Auto-import a builtin exception base (Exception, ValueError, …)
            // when subclassed — mirrors the `raise X(...)` auto-import. Without
            // this, `class Foo(Exception)` emits `extends Exception` with no
            // import → ReferenceError at load.
            if let ExprKind::Name(base_name) = &bases[0].kind {
                if is_builtin_exception(base_name) {
                    self.need_runtime(base_name);
                }
            }
            // Only the FIRST base goes on the JS prototype chain (single
            // `extends`); methods from the remaining bases are mixed in by
            // `__pyClass` below, in C3-MRO order. For regular hierarchies the
            // chain bottoms out at `PyObject` via the root class, enabling
            // cooperative MRO `__init__` dispatch.
            self.write(" extends ");
            self.emit_expr(&bases[0]);
        } else if pyobject_model {
            // No explicit base → extend the runtime `PyObject` so `new C(...)`
            // routes through its cooperative `__init__` dispatcher.
            self.need_runtime("PyObject");
            self.write(" extends PyObject");
        }
        self.write(" {\n");
        self.indent += 1;
        self.push_scope();
        self.class_stack.push(ClassCtx {
            name: name.to_string(),
            pyobject_model,
            has_bases: !bases.is_empty(),
        });

        if is_dataclass {
            self.emit_dataclass_body(name, body, &dc_opts);
        } else {
            self.emit_class_body(body);
        }

        self.pop_scope();
        self.class_stack.pop();
        self.indent -= 1;
        // #350: the redefinition form is a `name = class {…}` assignment
        // statement — terminate it with a semicolon before the post-class
        // (__mro__, method mix-in) statements that reference the name.
        self.writeln(if redefine { "};" } else { "}" });

        // Install the cooperative-MRO object model for every regular class:
        // compute `__mro__` via C3 linearization (PyObject appended as the
        // root), and mix in methods from non-first bases (first MRO winner
        // wins). Even no-base classes need this so `new.target.__mro__`
        // exists for the PyObject `__init__` dispatcher. Dataclasses and
        // exception subclasses keep their native paths.
        if pyobject_model {
            self.need_runtime("__pyClass");
            self.write_indent();
            self.write(&format!("__pyClass({}, [", Self::sanitize_ident(name)));
            for (i, base) in bases.iter().enumerate() {
                if i > 0 {
                    self.write(", ");
                }
                self.emit_expr(base);
            }
            self.write("]);\n");
        }

        // ── Round-3 pythonic sweep: post-class installation ─────────────
        let js_name = Self::sanitize_ident(name);
        // 0. Round-4 sweep: `type(x).__name__` reads `constructor.__name__`
        //    — JS classes don't have it, so install the Python name (also
        //    used by exception repr and the runtime base constructor to
        //    stamp `.name` on subclass instances).
        self.write_indent();
        self.write(&format!("{}.__name__ = \"{}\";\n", js_name, name));
        // 1. Class attributes: `Cls.attr = value` + a live prototype
        //    accessor so instances read through and instance assignment
        //    shadows (Python attribute lookup).
        for stmt in body {
            let (target, value) = match &stmt.kind {
                StmtKind::Assign { targets, value } if targets.len() == 1 => (&targets[0], value),
                StmtKind::AnnAssign {
                    target,
                    value: Some(v),
                    ..
                } => (target, v),
                _ => continue,
            };
            if is_dataclass {
                continue; // dataclass fields are constructor params
            }
            if let ExprKind::Name(attr_name) = &target.kind {
                self.need_runtime("__pyClassAttr");
                self.write_indent();
                self.write(&format!(
                    "__pyClassAttr({}, \"{}\", ",
                    js_name,
                    Self::sanitize_ident(attr_name)
                ));
                self.emit_expr(value);
                self.write(");\n");
            }
        }
        // 2. static/class methods reachable from instances (Python allows
        //    `instance.staticmethod()` / `instance.classmethod()`), and a
        //    toString alias so JS string coercion uses __str__/__repr__.
        let mut has_str = false;
        let mut has_repr = false;
        for stmt in body {
            if let StmtKind::FuncDef {
                name: m_name,
                params: m_params,
                decorator_list: m_decs,
                ..
            } = &stmt.kind
            {
                has_str |= m_name == "__str__";
                has_repr |= m_name == "__repr__";
                // #299: the class body emits methods under their RAW Python
                // name (JS allows reserved words as method/property names —
                // `default(obj) {}` is legal), so the alias/metadata targets
                // below must use the raw name too. Sanitizing here produced
                // `X.prototype.default$.__pyparams__ = ...` — a TypeError on
                // undefined — for any method named like a JS keyword.
                let m_js = m_name.clone();
                let is_static_m = m_decs
                    .iter()
                    .any(|d| matches!(&d.kind, ExprKind::Name(n) if n == "staticmethod"));
                let is_class_m = m_decs
                    .iter()
                    .any(|d| matches!(&d.kind, ExprKind::Name(n) if n == "classmethod"));
                let is_accessor = m_decs.iter().any(|d| {
                    matches!(&d.kind, ExprKind::Name(n) if n == "property")
                        || matches!(&d.kind, ExprKind::Attribute { attr, .. } if attr == "setter")
                });
                if is_static_m {
                    self.write_indent();
                    self.write(&format!(
                        "{cls}.prototype.{m} = {cls}.{m};\n",
                        cls = js_name,
                        m = m_js
                    ));
                } else if is_class_m {
                    // Instance access dispatches on the instance's actual
                    // class (subclass-aware, like Python).
                    self.write_indent();
                    self.write(&format!(
                        "{cls}.prototype.{m} = function (...a) {{ return this.constructor.{m}(...a); }};\n",
                        cls = js_name,
                        m = m_js
                    ));
                }
                // 3. kwargs-binding metadata (round-3): keyword calls bind
                //    by parameter name via __pyKwArgs. __init__ params land
                //    on the class object (constructor binding); plain
                //    methods on the prototype function; static/class
                //    methods on the class function. *args signatures and
                //    accessors keep the legacy options-object path.
                if !is_accessor && !m_params.iter().any(|p| p.is_args) {
                    // B1: RAW param name (see function-site rationale). The
                    // call site emits raw keyword names; __pyparams__ must
                    // match them, not the sanitized JS signature form.
                    let names: Vec<String> = m_params
                        .iter()
                        .filter(|p| !p.is_kwargs && p.name != "self" && p.name != "cls")
                        .map(|p| format!("\"{}\"", p.name))
                        .collect();
                    let has_kw = m_params.iter().any(|p| p.is_kwargs);
                    if !names.is_empty() || has_kw {
                        let target = if m_name == "__init__" {
                            js_name.to_string()
                        } else if is_static_m || is_class_m {
                            format!("{}.{}", js_name, m_js)
                        } else {
                            format!("{}.prototype.{}", js_name, m_js)
                        };
                        self.write_indent();
                        self.write(&format!(
                            "{}.__pyparams__ = [{}];\n",
                            target,
                            names.join(", ")
                        ));
                        if has_kw {
                            self.write_indent();
                            self.write(&format!("{}.__pykw__ = true;\n", target));
                        }
                    }
                }
            }
        }
        if is_dataclass {
            // Dataclass constructors bind keywords by field order.
            let fields = collect_dataclass_fields(body);
            if !fields.is_empty() {
                self.write_indent();
                self.write(&format!(
                    "{}.__pyparams__ = [{}];\n",
                    js_name,
                    fields
                        .iter()
                        .map(|f| format!("\"{}\"", f.name))
                        .collect::<Vec<_>>()
                        .join(", ")
                ));
            }
        }
        if has_str || has_repr {
            let src = if has_str { "__str__" } else { "__repr__" };
            self.write_indent();
            self.write(&format!(
                "{cls}.prototype.toString = {cls}.prototype.{src};\n",
                cls = js_name,
                src = src
            ));
        }

        // Apply class decorators (skip dataclass/component and their call forms)
        for decorator in decorator_list.iter().rev() {
            let (is_dc, _) = parse_dataclass_decorator(decorator);
            let skip = is_dc || matches!(&decorator.kind, ExprKind::Name(n) if n == "component");
            if !skip {
                self.write_indent();
                self.write(&format!("{} = ", Self::sanitize_ident(name)));
                self.emit_expr(decorator);
                self.write(&format!("({});\n", Self::sanitize_ident(name)));
            }
        }
    }

    fn emit_class_body(&mut self, body: &[Stmt]) {
        for stmt in body {
            match &stmt.kind {
                StmtKind::FuncDef {
                    name: method_name,
                    params,
                    body: method_body,
                    decorator_list: method_decorators,
                    is_async,
                    ..
                } => {
                    self.emit_class_method(
                        method_name,
                        params,
                        method_body,
                        method_decorators,
                        *is_async,
                    );
                }
                // Round-3 pythonic sweep: class-level attribute assignments
                // previously fell through to emit_stmt, producing an invalid
                // `let x = ...;` INSIDE the class body. They are collected by
                // emit_class_def and installed after the class via
                // __pyClassAttr (class-object attribute + a live prototype
                // accessor, so instances read through and instance
                // assignment shadows — Python lookup semantics).
                StmtKind::Assign { targets, .. }
                    if targets.len() == 1 && matches!(&targets[0].kind, ExprKind::Name(_)) => {}
                StmtKind::AnnAssign {
                    target,
                    value: Some(_),
                    ..
                } if matches!(&target.kind, ExprKind::Name(_)) => {}
                StmtKind::Assign { .. } | StmtKind::AnnAssign { .. } => {
                    self.emit_stmt(stmt);
                }
                StmtKind::Pass => {}
                _ => self.emit_stmt(stmt),
            }
        }
    }

    fn emit_dataclass_body(&mut self, class_name: &str, body: &[Stmt], opts: &DataclassOptions) {
        let fields = collect_dataclass_fields(body);

        // Collect @validator methods: (field_name, method_name)
        let mut validators: Vec<(String, String)> = Vec::new();
        // Collect @check methods: method_name
        let mut checks: Vec<String> = Vec::new();
        for stmt in body {
            if let StmtKind::FuncDef {
                name: method_name,
                decorator_list: method_decorators,
                ..
            } = &stmt.kind
            {
                for dec in method_decorators {
                    match &dec.kind {
                        ExprKind::Call { func, args, .. } => {
                            if let ExprKind::Name(n) = &func.kind {
                                if n == "validator" {
                                    if let Some(arg) = args.first() {
                                        if let ExprKind::StringLiteral(field_name) = &arg.kind {
                                            validators
                                                .push((field_name.clone(), method_name.clone()));
                                        }
                                    }
                                }
                            }
                        }
                        ExprKind::Name(n) if n == "check" => {
                            checks.push(method_name.clone());
                        }
                        _ => {}
                    }
                }
            }
        }

        // Emit constructor
        self.write_indent();
        self.write("constructor(");
        for (i, field) in fields.iter().enumerate() {
            if i > 0 {
                self.write(", ");
            }
            self.write(&field.name);
            // default_factory → default value in param
            if let Some(factory) = &field.constraints.default_factory {
                self.write(" = ");
                match factory.as_str() {
                    "list" => self.write("[]"),
                    "dict" => self.write("{}"),
                    // #297: canonicalizing PySet.
                    "set" => {
                        self.need_runtime("PySet");
                        self.write("new PySet()");
                    }
                    other => self.write(&format!("{}()", other)),
                }
            } else if let Some(val) = &field.default {
                self.write(" = ");
                self.emit_expr(val);
            }
        }
        self.write(") {\n");
        self.indent += 1;

        // Allow `new Foo({a, b, c})` (kwargs-object style — the natural
        // form when the caller used `Foo(a=..., b=..., c=...)` in .ps)
        // in addition to `new Foo(a, b, c)` (positional). Detect a
        // single plain-object first arg and destructure into the params.
        if !fields.is_empty() {
            self.write_indent();
            self.write("if (arguments.length === 1 && ");
            self.write(&fields[0].name);
            self.write(" !== null && typeof ");
            self.write(&fields[0].name);
            self.write(" === \"object\" && !Array.isArray(");
            self.write(&fields[0].name);
            self.write(")) {\n");
            self.indent += 1;
            self.write_indent();
            self.write("({");
            for (i, field) in fields.iter().enumerate() {
                if i > 0 {
                    self.write(", ");
                }
                self.write(&field.name);
                // Preserve declared defaults when destructuring from object.
                if let Some(factory) = &field.constraints.default_factory {
                    self.write(" = ");
                    match factory.as_str() {
                        "list" => self.write("[]"),
                        "dict" => self.write("{}"),
                        // #297: canonicalizing PySet.
                        "set" => {
                            self.need_runtime("PySet");
                            self.write("new PySet()");
                        }
                        other => self.write(&format!("{}()", other)),
                    }
                } else if let Some(val) = &field.default {
                    self.write(" = ");
                    self.emit_expr(val);
                }
            }
            self.write("} = ");
            self.write(&fields[0].name);
            self.write(");\n");
            self.indent -= 1;
            self.write_indent();
            self.write("}\n");
        }

        // 1. Coercion (if coerce=True)
        if opts.coerce {
            for field in &fields {
                if let Some(ann) = field.annotation {
                    self.emit_coercion(class_name, &field.name, ann);
                }
            }
        }

        // 2. Error collection setup (if collect_errors=True)
        if opts.collect_errors {
            self.write_indent();
            self.write("const __errors = [];\n");
            self.collecting_errors = true;
        }

        // 3. Type validation per field
        for field in &fields {
            if let Some(ann) = field.annotation {
                self.emit_type_validation(class_name, &field.name, ann);
            }
        }

        // 4. String transforms (trim, to_lower, to_upper) — before constraint validation
        for field in &fields {
            self.emit_transform_constraints(&field.name, &field.constraints);
        }

        // 5. Constraint validation per field
        for field in &fields {
            self.emit_constraint_validation(class_name, &field.name, &field.constraints);
        }

        // 6. Throw collected errors (if collect_errors=True)
        if opts.collect_errors {
            self.collecting_errors = false;
            self.write_indent();
            self.write("if (__errors.length > 0) throw new TypeError(__errors.join(\"; \"));\n");
        }

        // 7. Field assignments
        for field in &fields {
            self.write_indent();
            self.write(&format!("this.{f} = {f};\n", f = field.name));
        }

        // 8. @validator calls
        for (field_name, method_name) in &validators {
            self.write_indent();
            self.write(&format!(
                "this.{f} = this.{m}(this.{f});\n",
                f = field_name,
                m = method_name
            ));
        }

        // 9. @check calls (cross-field validation)
        for method_name in &checks {
            self.write_indent();
            self.write(&format!("this.{}();\n", method_name));
        }

        // 10. Object.freeze for frozen dataclass
        if opts.frozen {
            self.write_indent();
            self.write("Object.freeze(this);\n");
        }

        self.indent -= 1;
        self.writeln("}");

        // Emit __repr__ (round-3: repr-exact — fields render via pyRepr so
        // string fields quote like CPython's generated __repr__) with
        // toString delegating for JS string coercion. A user-defined
        // __repr__ later in the body wins (JS: last same-name member).
        self.need_runtime("pyRepr");
        self.write_indent();
        self.write("__repr__() {\n");
        self.indent += 1;
        self.write_indent();
        self.write(&format!("return \"{}(\"", class_name));
        for (i, field) in fields.iter().enumerate() {
            if i > 0 {
                self.write(" + \", \"");
            }
            self.write(&format!(
                " + \"{}=\" + pyRepr(this.{})",
                field.name, field.name
            ));
        }
        self.write(" + \")\";\n");
        self.indent -= 1;
        self.writeln("}");
        self.write_indent();
        self.write("toString() {\n");
        self.indent += 1;
        self.write_indent();
        self.write("return this.__repr__();\n");
        self.indent -= 1;
        self.writeln("}");

        // Emit __eq__
        self.write_indent();
        self.write("__eq__(other) {\n");
        self.indent += 1;
        self.write_indent();
        self.write(&format!(
            "return other instanceof {}",
            Self::sanitize_ident(class_name)
        ));
        for field in &fields {
            self.write(&format!(" && this.{} === other.{}", field.name, field.name));
        }
        self.write(";\n");
        self.indent -= 1;
        self.writeln("}");

        // Emit toDict()
        self.write_indent();
        self.write("toDict() {\n");
        self.indent += 1;
        self.write_indent();
        self.write("return { ");
        for (i, field) in fields.iter().enumerate() {
            if i > 0 {
                self.write(", ");
            }
            let tc = field
                .annotation
                .map(resolve_type_check)
                .unwrap_or(TypeCheck::None);
            match &tc {
                TypeCheck::Instance(_) => {
                    self.write(&format!(
                        "{f}: this.{f}?.toDict?.() ?? this.{f}",
                        f = field.name
                    ));
                }
                TypeCheck::List(Some(inner)) => {
                    if matches!(inner.as_ref(), TypeCheck::Instance(_)) {
                        self.write(&format!(
                            "{f}: this.{f}.map(x => x?.toDict?.() ?? x)",
                            f = field.name
                        ));
                    } else {
                        self.write(&format!("{f}: this.{f}", f = field.name));
                    }
                }
                _ => {
                    self.write(&format!("{f}: this.{f}", f = field.name));
                }
            }
        }
        self.write(" };\n");
        self.indent -= 1;
        self.writeln("}");

        // Emit static fromDict(data)
        self.write_indent();
        self.write("static fromDict(data) {\n");
        self.indent += 1;
        self.write_indent();
        self.write(&format!("return new {}(", Self::sanitize_ident(class_name)));
        for (i, field) in fields.iter().enumerate() {
            if i > 0 {
                self.write(", ");
            }
            let tc = field
                .annotation
                .map(resolve_type_check)
                .unwrap_or(TypeCheck::None);
            if let TypeCheck::Instance(cls_name) = &tc {
                self.write(&format!("{}.fromDict(data.{})", cls_name, field.name));
            } else {
                self.write(&format!("data.{}", field.name));
            }
        }
        self.write(");\n");
        self.indent -= 1;
        self.writeln("}");

        // Emit any additional methods defined in the body (skip @validator-decorated ones from decoration but still emit the method)
        for stmt in body {
            match &stmt.kind {
                StmtKind::FuncDef {
                    name: method_name,
                    params,
                    body: method_body,
                    decorator_list: method_decorators,
                    is_async,
                    ..
                } => {
                    // Skip decorator application for @validator and @check but still emit the method
                    let filtered_decorators: Vec<Expr> = method_decorators
                        .iter()
                        .filter(|d| {
                            !matches!(&d.kind, ExprKind::Call { func, .. }
                                if matches!(&func.kind, ExprKind::Name(n) if n == "validator"))
                                && !matches!(&d.kind, ExprKind::Name(n) if n == "check")
                        })
                        .cloned()
                        .collect();
                    self.emit_class_method(
                        method_name,
                        params,
                        method_body,
                        &filtered_decorators,
                        *is_async,
                    );
                }
                StmtKind::Pass => {}
                _ => {}
            }
        }
    }

    /// Emit a validation error — either `throw` or `__errors.push(...)` depending on mode.
    fn emit_validation_error(&mut self, condition: &str, msg_expr: &str) {
        self.write_indent();
        if self.collecting_errors {
            self.write(&format!(
                "if ({}) __errors.push({});\n",
                condition, msg_expr
            ));
        } else {
            self.write(&format!(
                "if ({}) throw new TypeError({});\n",
                condition, msg_expr
            ));
        }
    }

    /// Emit type validation checks for a dataclass field.
    fn emit_type_validation(&mut self, class_name: &str, field_name: &str, annotation: &Expr) {
        let tc = resolve_type_check(annotation);
        self.emit_type_check(class_name, field_name, field_name, &tc);
    }

    /// Recursively emit type check for a value identified by `var`.
    fn emit_type_check(&mut self, class_name: &str, field_name: &str, var: &str, tc: &TypeCheck) {
        match tc {
            TypeCheck::None => {}
            TypeCheck::Int => {
                self.emit_validation_error(
                    &format!(
                        "typeof {v} !== \"number\" || !Number.isInteger({v})",
                        v = var
                    ),
                    &format!(
                        "\"{c}.{f}: expected int, got \" + typeof {v}",
                        v = var,
                        c = class_name,
                        f = field_name
                    ),
                );
            }
            TypeCheck::Float => {
                self.emit_validation_error(
                    &format!("typeof {v} !== \"number\"", v = var),
                    &format!(
                        "\"{c}.{f}: expected float, got \" + typeof {v}",
                        v = var,
                        c = class_name,
                        f = field_name
                    ),
                );
            }
            TypeCheck::Str => {
                self.emit_validation_error(
                    &format!("typeof {v} !== \"string\"", v = var),
                    &format!(
                        "\"{c}.{f}: expected str, got \" + typeof {v}",
                        v = var,
                        c = class_name,
                        f = field_name
                    ),
                );
            }
            TypeCheck::Bool => {
                self.emit_validation_error(
                    &format!("typeof {v} !== \"boolean\"", v = var),
                    &format!(
                        "\"{c}.{f}: expected bool, got \" + typeof {v}",
                        v = var,
                        c = class_name,
                        f = field_name
                    ),
                );
            }
            TypeCheck::List(inner) => {
                self.emit_validation_error(
                    &format!("!Array.isArray({v})", v = var),
                    &format!(
                        "\"{c}.{f}: expected list, got \" + typeof {v}",
                        v = var,
                        c = class_name,
                        f = field_name
                    ),
                );
                if let Some(element_tc) = inner {
                    if !matches!(element_tc.as_ref(), TypeCheck::None) {
                        let elem_var = format!("{}_el", var.replace('.', "_"));
                        self.write_indent();
                        self.write(&format!("for (const {} of {}) {{\n", elem_var, var));
                        self.indent += 1;
                        self.emit_type_check(class_name, field_name, &elem_var, element_tc);
                        self.indent -= 1;
                        self.writeln("}");
                    }
                }
            }
            TypeCheck::Dict(_, _) => {
                self.emit_validation_error(
                    &format!(
                        "typeof {v} !== \"object\" || {v} === null || Array.isArray({v})",
                        v = var
                    ),
                    &format!(
                        "\"{c}.{f}: expected dict, got \" + typeof {v}",
                        v = var,
                        c = class_name,
                        f = field_name
                    ),
                );
            }
            TypeCheck::Instance(cls) => {
                self.emit_validation_error(
                    &format!("!({v} instanceof {cls})", v = var, cls = cls),
                    &format!(
                        "\"{c}.{f}: expected {cls}, got \" + typeof {v}",
                        v = var,
                        c = class_name,
                        f = field_name,
                        cls = cls
                    ),
                );
            }
            TypeCheck::Optional(inner) => {
                self.write_indent();
                self.write(&format!(
                    "if ({v} !== null && {v} !== undefined) {{\n",
                    v = var
                ));
                self.indent += 1;
                self.emit_type_check(class_name, field_name, var, inner);
                self.indent -= 1;
                self.writeln("}");
            }
        }
    }

    /// Emit constraint validation checks for a dataclass field.
    fn emit_constraint_validation(
        &mut self,
        class_name: &str,
        field_name: &str,
        constraints: &FieldConstraints,
    ) {
        if let Some(gt) = constraints.gt {
            self.emit_validation_error(
                &format!("{f} <= {v}", f = field_name, v = format_f64(gt)),
                &format!(
                    "\"{c}.{f}: must be > {v}, got \" + {f}",
                    f = field_name,
                    v = format_f64(gt),
                    c = class_name
                ),
            );
        }
        if let Some(ge) = constraints.ge {
            self.emit_validation_error(
                &format!("{f} < {v}", f = field_name, v = format_f64(ge)),
                &format!(
                    "\"{c}.{f}: must be >= {v}, got \" + {f}",
                    f = field_name,
                    v = format_f64(ge),
                    c = class_name
                ),
            );
        }
        if let Some(lt) = constraints.lt {
            self.emit_validation_error(
                &format!("{f} >= {v}", f = field_name, v = format_f64(lt)),
                &format!(
                    "\"{c}.{f}: must be < {v}, got \" + {f}",
                    f = field_name,
                    v = format_f64(lt),
                    c = class_name
                ),
            );
        }
        if let Some(le) = constraints.le {
            self.emit_validation_error(
                &format!("{f} > {v}", f = field_name, v = format_f64(le)),
                &format!(
                    "\"{c}.{f}: must be <= {v}, got \" + {f}",
                    f = field_name,
                    v = format_f64(le),
                    c = class_name
                ),
            );
        }
        if let Some(min_len) = constraints.min_length {
            self.emit_validation_error(
                &format!("{f}.length < {v}", f = field_name, v = min_len),
                &format!(
                    "\"{c}.{f}: length must be >= {v}\"",
                    f = field_name,
                    v = min_len,
                    c = class_name
                ),
            );
        }
        if let Some(max_len) = constraints.max_length {
            self.emit_validation_error(
                &format!("{f}.length > {v}", f = field_name, v = max_len),
                &format!(
                    "\"{c}.{f}: length must be <= {v}\"",
                    f = field_name,
                    v = max_len,
                    c = class_name
                ),
            );
        }
        if let Some(pattern) = &constraints.pattern {
            self.emit_validation_error(
                &format!("!/{p}/.test({f})", f = field_name, p = pattern),
                &format!(
                    "\"{c}.{f}: must match pattern /{p}/\"",
                    f = field_name,
                    p = pattern,
                    c = class_name
                ),
            );
        }
        // String validators
        if constraints.email {
            self.emit_validation_error(
                &format!(
                    "!/^[^\\s@]+@[^\\s@]+\\.[^\\s@]+$/.test({f})",
                    f = field_name
                ),
                &format!(
                    "\"{c}.{f}: must be a valid email\"",
                    f = field_name,
                    c = class_name
                ),
            );
        }
        if constraints.url {
            self.emit_validation_error(
                &format!("!/^https?:\\/\\/.+/.test({f})", f = field_name),
                &format!(
                    "\"{c}.{f}: must be a valid URL\"",
                    f = field_name,
                    c = class_name
                ),
            );
        }
        if constraints.uuid {
            self.emit_validation_error(
                &format!("!/^[0-9a-f]{{8}}-[0-9a-f]{{4}}-[0-9a-f]{{4}}-[0-9a-f]{{4}}-[0-9a-f]{{12}}$/i.test({f})", f = field_name),
                &format!("\"{c}.{f}: must be a valid UUID\"", f = field_name, c = class_name),
            );
        }
        if let Some(prefix) = &constraints.starts_with {
            self.emit_validation_error(
                &format!("!{f}.startsWith(\"{p}\")", f = field_name, p = prefix),
                &format!(
                    "\"{c}.{f}: must start with '{p}'\"",
                    f = field_name,
                    p = prefix,
                    c = class_name
                ),
            );
        }
        if let Some(suffix) = &constraints.ends_with {
            self.emit_validation_error(
                &format!("!{f}.endsWith(\"{s}\")", f = field_name, s = suffix),
                &format!(
                    "\"{c}.{f}: must end with '{s}'\"",
                    f = field_name,
                    s = suffix,
                    c = class_name
                ),
            );
        }
        if let Some(substr) = &constraints.includes {
            self.emit_validation_error(
                &format!("!{f}.includes(\"{s}\")", f = field_name, s = substr),
                &format!(
                    "\"{c}.{f}: must include '{s}'\"",
                    f = field_name,
                    s = substr,
                    c = class_name
                ),
            );
        }
        // Number validators
        if constraints.positive {
            self.emit_validation_error(
                &format!("{f} <= 0", f = field_name),
                &format!(
                    "\"{c}.{f}: must be positive\"",
                    f = field_name,
                    c = class_name
                ),
            );
        }
        if constraints.negative {
            self.emit_validation_error(
                &format!("{f} >= 0", f = field_name),
                &format!(
                    "\"{c}.{f}: must be negative\"",
                    f = field_name,
                    c = class_name
                ),
            );
        }
        if constraints.nonnegative {
            self.emit_validation_error(
                &format!("{f} < 0", f = field_name),
                &format!(
                    "\"{c}.{f}: must be nonnegative\"",
                    f = field_name,
                    c = class_name
                ),
            );
        }
        if let Some(divisor) = constraints.multiple_of {
            self.emit_validation_error(
                &format!("{f} % {d} !== 0", f = field_name, d = format_f64(divisor)),
                &format!(
                    "\"{c}.{f}: must be a multiple of {d}\"",
                    f = field_name,
                    d = format_f64(divisor),
                    c = class_name
                ),
            );
        }
        if constraints.finite {
            self.emit_validation_error(
                &format!("!Number.isFinite({f})", f = field_name),
                &format!(
                    "\"{c}.{f}: must be finite\"",
                    f = field_name,
                    c = class_name
                ),
            );
        }
        // Choices
        if !constraints.choices.is_empty() {
            // SECURITY (A5): a string choice value is source-derived and gets
            // spliced into BOTH the emitted `includes([...])` array AND the
            // error-message literal. Route string choices through the escaper
            // so they can't break out of their JS string literal, and build the
            // error message by wrapping the rendered list in js_string_literal
            // rather than splicing raw JS into a `"..."` template.
            let items: Vec<String> = constraints
                .choices
                .iter()
                .map(|cv| match cv {
                    ChoiceValue::Str(s) => js_string_literal(s),
                    ChoiceValue::Int(n) => format!("{}", n),
                    ChoiceValue::Float(n) => format_f64(*n),
                })
                .collect();
            let items_str = items.join(", ");
            let msg = format!(
                "{c}.{f}: must be one of [{items}]",
                f = field_name,
                c = class_name,
                items = items_str
            );
            self.emit_validation_error(
                &format!(
                    "![{items}].includes({f})",
                    f = field_name,
                    items = items_str
                ),
                &js_string_literal(&msg),
            );
        }
    }

    /// Emit type coercion for a dataclass field (when coerce=True).
    fn emit_coercion(&mut self, class_name: &str, field_name: &str, annotation: &Expr) {
        let tc = resolve_type_check(annotation);
        self.emit_coercion_for_type(class_name, field_name, field_name, &tc);
    }

    /// Recursively emit coercion logic for a type.
    fn emit_coercion_for_type(
        &mut self,
        class_name: &str,
        field_name: &str,
        var: &str,
        tc: &TypeCheck,
    ) {
        match tc {
            TypeCheck::Int => {
                self.write_indent();
                self.write(&format!(
                    "if (typeof {v} === \"string\") {{ {v} = parseInt({v}, 10); if (isNaN({v})) throw new TypeError(\"{c}.{f}: cannot coerce to int\"); }}\n",
                    v = var, c = class_name, f = field_name
                ));
            }
            TypeCheck::Float => {
                self.write_indent();
                self.write(&format!(
                    "if (typeof {v} === \"string\") {{ {v} = parseFloat({v}); if (isNaN({v})) throw new TypeError(\"{c}.{f}: cannot coerce to float\"); }}\n",
                    v = var, c = class_name, f = field_name
                ));
            }
            TypeCheck::Str => {
                self.write_indent();
                self.write(&format!(
                    "if (typeof {v} !== \"string\") {{ {v} = String({v}); }}\n",
                    v = var
                ));
            }
            TypeCheck::Bool => {
                self.write_indent();
                self.write(&format!(
                    "if (typeof {v} === \"string\") {{ {v} = {v} === \"true\"; }} else if (typeof {v} === \"number\") {{ {v} = {v} !== 0; }}\n",
                    v = var
                ));
            }
            TypeCheck::Optional(inner) => {
                self.write_indent();
                self.write(&format!(
                    "if ({v} !== null && {v} !== undefined) {{\n",
                    v = var
                ));
                self.indent += 1;
                self.emit_coercion_for_type(class_name, field_name, var, inner);
                self.indent -= 1;
                self.writeln("}");
            }
            _ => {} // List, Dict, Instance, None — no coercion
        }
    }

    /// Emit string transform operations (trim, to_lower, to_upper) for a field.
    fn emit_transform_constraints(&mut self, field_name: &str, constraints: &FieldConstraints) {
        if constraints.trim {
            self.write_indent();
            self.write(&format!("{f} = {f}.trim();\n", f = field_name));
        }
        if constraints.to_lower {
            self.write_indent();
            self.write(&format!("{f} = {f}.toLowerCase();\n", f = field_name));
        }
        if constraints.to_upper {
            self.write_indent();
            self.write(&format!("{f} = {f}.toUpperCase();\n", f = field_name));
        }
    }

    fn emit_class_method(
        &mut self,
        name: &str,
        params: &[Param],
        body: &[Stmt],
        decorators: &[Expr],
        is_async: bool,
    ) {
        let is_static = decorators
            .iter()
            .any(|d| matches!(&d.kind, ExprKind::Name(n) if n == "staticmethod"));
        // Round-3: @classmethod → JS static method with `cls` bound to
        // `this` (the class — subclass-aware through static dispatch).
        let is_classmethod = decorators
            .iter()
            .any(|d| matches!(&d.kind, ExprKind::Name(n) if n == "classmethod"));
        let is_property = decorators
            .iter()
            .any(|d| matches!(&d.kind, ExprKind::Name(n) if n == "property"));
        // Round-3: @<prop>.setter → JS `set <prop>(v)` accessor, so plain
        // attribute assignment (`t.celsius = 100`) invokes it.
        let is_setter = decorators
            .iter()
            .any(|d| matches!(&d.kind, ExprKind::Attribute { attr, .. } if attr == "setter"));

        self.write_indent();

        if is_static || is_classmethod {
            self.write("static ");
        }
        if is_property {
            self.write("get ");
        }
        if is_setter {
            self.write("set ");
        }
        let is_generator = body_contains_yield(body);
        if is_async {
            self.write("async ");
        }

        // In the cooperative PyObject model, `__init__` is a prototype method
        // dispatched via the MRO (so `super().__init__()` chains across
        // multiple inheritance), NOT the JS `constructor`. Exception
        // subclasses (non-PyObject) keep `__init__` → `constructor` + the
        // native single-inheritance super hoist.
        let pyobject_model = self
            .class_stack
            .last()
            .map(|c| c.pyobject_model)
            .unwrap_or(false);
        let emit_as_constructor = name == "__init__" && !pyobject_model;

        // Round-3: `__repr__`/`__str__` keep their REAL names so
        // repr()/str() can dispatch on them distinctly; emit_class_def
        // installs a `toString` alias (preferring __str__) for JS string
        // coercion (template literals, string concat).
        let js_name = if emit_as_constructor {
            "constructor"
        } else {
            name
        };

        if is_generator {
            self.write(&format!("*{}(", js_name));
        } else {
            self.write(&format!("{}(", js_name));
        }
        self.emit_params(params);
        self.write(") {\n");
        self.indent += 1;

        // @classmethod body: `cls` is the class — in a JS static method
        // that is `this` (subclass static dispatch keeps it accurate).
        let prev_in_classmethod = self.in_classmethod;
        if is_classmethod {
            self.in_classmethod = true;
            self.write_indent();
            self.write("const cls = this;\n");
        }
        // Round-4 sweep: awaiting is only legal inside async method bodies
        // (async generator methods included).
        let prev_await_ok = self.await_ok;
        self.await_ok = is_async;

        // For a native constructor (exception subclass __init__), emit
        // this.x = x for each self.x = x assignment, hoisting super().
        if emit_as_constructor {
            // A subclass constructor must call super() before touching `this`.
            // Python writes `super().__init__(args)` (anywhere in the body); JS
            // needs `super(args)` first. Hoist it to the top and skip it below.
            if let Some(super_args) = body.iter().find_map(super_init_args) {
                self.write_indent();
                self.write("super(");
                for (i, arg) in super_args.iter().enumerate() {
                    if i > 0 {
                        self.write(", ");
                    }
                    self.emit_expr_rewrite_self(arg);
                }
                self.write(");\n");
            } else if self.class_stack.last().is_some_and(|c| c.has_bases) {
                // #300: Python does NOT require `__init__` to call
                // `super().__init__()`, but a JS derived constructor MUST
                // call `super()` before touching `this` — without it, `new`
                // throws "Must call super constructor" unconditionally.
                // Synthesize a bare `super();`. (Closest match to Python:
                // the base `__init__` isn't invoked with the derived args;
                // JS offers no way to skip base construction entirely.)
                self.write_indent();
                self.write("super();\n");
            }
            for stmt in body {
                if super_init_args(stmt).is_some() {
                    continue; // already emitted as the hoisted super(...) call
                }
                match &stmt.kind {
                    StmtKind::Assign { targets, value } => {
                        if let Some(target) = targets.first() {
                            if let ExprKind::Attribute {
                                value: obj, attr, ..
                            } = &target.kind
                            {
                                if matches!(&obj.kind, ExprKind::Name(n) if n == "self") {
                                    self.write_indent();
                                    self.write(&format!("this.{} = ", attr));
                                    self.emit_expr_rewrite_self(value);
                                    self.write(";\n");
                                    continue;
                                }
                            }
                        }
                        self.emit_stmt(stmt);
                    }
                    _ => self.emit_stmt(stmt),
                }
            }
        } else {
            for stmt in body {
                self.emit_stmt(stmt);
            }
        }

        self.await_ok = prev_await_ok;
        self.in_classmethod = prev_in_classmethod;
        self.indent -= 1;
        self.writeln("}");
    }

    /// Emit an expression, rewriting `self.x` → `this.x` and `self` → `this`.
    fn emit_expr_rewrite_self(&mut self, expr: &Expr) {
        // Second recursive expression walk (self-attribute rewriting): a deep
        // `self.a.b.c…` chain recurses here without touching `emit_expr`, so it
        // needs its own share of the same depth budget.
        let _guard = match EmitDepthGuard::enter() {
            Some(g) => g,
            None => {
                self.record_emit_overflow(expr.span.start);
                return;
            }
        };
        match &expr.kind {
            ExprKind::Name(n) if n == "self" => self.write("this"),
            ExprKind::Attribute {
                value,
                attr,
                optional,
            } => {
                if matches!(&value.kind, ExprKind::Name(n) if n == "self") {
                    self.write(&format!("this.{}", attr));
                } else {
                    self.emit_expr_rewrite_self(value);
                    let dot = if *optional { "?." } else { "." };
                    self.write(&format!("{}{}", dot, attr));
                }
            }
            _ => self.emit_expr(expr),
        }
    }

    fn emit_if(
        &mut self,
        test: &Expr,
        body: &[Stmt],
        elif_clauses: &[(Expr, Vec<Stmt>)],
        else_body: &Option<Vec<Stmt>>,
    ) {
        self.write_indent();
        self.write("if (");
        self.emit_test_expr(test);
        self.write(") {\n");
        self.indent += 1;
        for stmt in body {
            self.emit_stmt(stmt);
        }
        self.indent -= 1;

        for (cond, stmts) in elif_clauses {
            self.write_indent();
            self.write("} else if (");
            self.emit_test_expr(cond);
            self.write(") {\n");
            self.indent += 1;
            for stmt in stmts {
                self.emit_stmt(stmt);
            }
            self.indent -= 1;
        }

        if let Some(stmts) = else_body {
            self.writeln("} else {");
            self.indent += 1;
            for stmt in stmts {
                self.emit_stmt(stmt);
            }
            self.indent -= 1;
        }

        self.writeln("}");
    }

    fn emit_while(&mut self, test: &Expr, body: &[Stmt], else_body: &Option<Vec<Stmt>>) {
        // #91: unique per-loop break flag, set by `break` (see
        // StmtKind::Break) so the else clause is suppressed on break.
        let flag = else_body.as_ref().map(|_| {
            let f = format!("__while_broke_{}", self.loop_flag_counter);
            self.loop_flag_counter += 1;
            f
        });
        if let Some(f) = &flag {
            self.writeln(&format!("let {} = false;", f));
        }
        self.loop_flag_stack.push(flag.clone());

        self.write_indent();
        self.write("while (");
        self.emit_test_expr(test);
        self.write(") {\n");
        self.indent += 1;
        for stmt in body {
            self.emit_stmt(stmt);
        }
        self.indent -= 1;
        self.writeln("}");
        self.loop_flag_stack.pop();

        if let Some(stmts) = else_body {
            self.writeln(&format!("if (!{}) {{", flag.as_ref().unwrap()));
            self.indent += 1;
            for stmt in stmts {
                self.emit_stmt(stmt);
            }
            self.indent -= 1;
            self.writeln("}");
        }
    }

    /// #235 / #349(b): try to lower `for <name> in range(<a,b,c>):` to a
    /// C-style counting loop. Returns true iff it emitted the loop.
    ///
    /// The loop drives a private counter (`__ri_N`) and copies it into the
    /// Python loop variable at the top of each iteration. That copy preserves
    /// every Python semantic a raw C-style counter would break:
    /// - **leak value**: after the loop the variable holds the LAST yielded
    ///   value (not `stop`), because the last write is the last counter value;
    /// - **possibly-unbound**: a zero-iteration loop never writes the
    ///   variable, so a hoisted sentinel target stays `__UNBOUND` (CPython
    ///   raises on a post-loop read) — matching the for-of path;
    /// - **body rebind is per-iteration**: `for i in range(n): i = 99` does
    ///   not bleed into the next iteration, because the counter is independent.
    ///
    /// Guards (fall back to the generic pyRange for-of when any fails):
    /// - target is a simple name (range yields scalars, never a tuple target);
    /// - `range` is the builtin (not shadowed by a user binding);
    /// - 1..=3 positional args, no kwargs, non-zero literal step.
    fn try_emit_range_for(
        &mut self,
        target: &Expr,
        iter: &Expr,
        body: &[Stmt],
        else_body: &Option<Vec<Stmt>>,
    ) -> bool {
        let name = match &target.kind {
            ExprKind::Name(n) => n,
            _ => return false,
        };
        let (args, kwargs) = match &iter.kind {
            ExprKind::Call {
                func,
                args,
                kwargs,
                optional: false,
            } => match &func.kind {
                ExprKind::Name(fname)
                    if fname == "range" && !self.is_declared_in_any_scope("range") =>
                {
                    (args, kwargs)
                }
                _ => return false,
            },
            _ => return false,
        };
        if !kwargs.is_empty() || args.is_empty() || args.len() > 3 {
            return false;
        }
        // A literal step of 0 is a Python ValueError; leave it to pyRange.
        if args.len() == 3 {
            if let ExprKind::IntLiteral(0) = &args[2].kind {
                return false;
            }
        }

        let k = self.default_hoist_counter;
        self.default_hoist_counter += 1;
        let jname = Self::sanitize_ident(name).into_owned();
        let ri = format!("__ri_{}", k);
        let start_t = format!("__r_start_{}", k);
        let stop_t = format!("__r_stop_{}", k);
        let step_t = format!("__r_step_{}", k);
        // A hoisted target is a pre-declared outer `let`; write it bare. A
        // fresh target gets a per-iteration `let` (block-scoped, so closures
        // capture the right value — same as the for-of `const` binding).
        let hoisted = self.is_hoisted(name);

        // break flag for the for-else clause (same mechanism as emit_for).
        let flag = else_body.as_ref().map(|_| {
            let f = format!("__for_broke_{}", self.loop_flag_counter);
            self.loop_flag_counter += 1;
            f
        });
        if let Some(f) = &flag {
            self.writeln(&format!("let {} = false;", f));
        }
        self.loop_flag_stack.push(flag.clone());

        // Evaluate the bounds ONCE, left-to-right (Python arg-eval order),
        // scoped in a block so the temps don't leak.
        self.writeln("{");
        self.indent += 1;
        // start
        self.write_indent();
        self.write(&format!("const {} = ", start_t));
        if args.len() == 1 {
            self.write("0");
        } else {
            self.emit_expr(&args[0]);
        }
        self.write(";\n");
        // stop
        let stop_arg = if args.len() == 1 { &args[0] } else { &args[1] };
        self.write_indent();
        self.write(&format!("const {} = ", stop_t));
        self.emit_expr(stop_arg);
        self.write(";\n");
        // step (literal 1 when absent)
        let step_lit: Option<i128> = if args.len() == 3 {
            self.write_indent();
            self.write(&format!("const {} = ", step_t));
            self.emit_expr(&args[2]);
            self.write(";\n");
            if let ExprKind::IntLiteral(v) = &args[2].kind {
                Some(*v)
            } else {
                None
            }
        } else {
            Some(1)
        };
        let step_ref = if args.len() == 3 {
            step_t.as_str()
        } else {
            "1"
        };

        // condition + update over the private counter
        let (cond, update) = match step_lit {
            Some(v) if v > 0 => (
                format!("{} < {}", ri, stop_t),
                format!("{} += {}", ri, step_ref),
            ),
            Some(v) if v < 0 => (
                format!("{} > {}", ri, stop_t),
                format!("{} += {}", ri, step_t),
            ),
            Some(_) => unreachable!(),
            None => (
                format!("({0} > 0 ? {1} < {2} : {1} > {2})", step_t, ri, stop_t),
                format!("{} += {}", ri, step_t),
            ),
        };
        self.write_indent();
        self.write(&format!(
            "for (let {} = {}; {}; {}) {{\n",
            ri, start_t, cond, update
        ));
        self.indent += 1;
        // Copy the counter into the Python loop variable (see doc comment).
        self.writeln(&format!(
            "{}{} = {};",
            if hoisted { "" } else { "let " },
            jname,
            ri
        ));
        self.declare_target(target);
        for stmt in body {
            self.emit_stmt(stmt);
        }
        self.indent -= 1;
        self.writeln("}");
        self.indent -= 1;
        self.writeln("}");
        self.loop_flag_stack.pop();

        if let Some(stmts) = else_body {
            self.writeln(&format!("if (!{}) {{", flag.as_ref().unwrap()));
            self.indent += 1;
            for stmt in stmts {
                self.emit_stmt(stmt);
            }
            self.indent -= 1;
            self.writeln("}");
        }
        true
    }

    fn emit_for(
        &mut self,
        target: &Expr,
        iter: &Expr,
        body: &[Stmt],
        else_body: &Option<Vec<Stmt>>,
        is_async: bool,
    ) {
        // #235 / #349(b): `for i in range(...)` over a native counting loop —
        // no pyRange array materialisation (fixes the `range(a, huge)` OOM,
        // MBPP/100) and the fastest possible integer loop (removes per-iter
        // pyRange allocation, the EvalPerf range-hot tail). Falls back to the
        // generic for-of path when the pattern isn't a plain non-leaking name.
        if !is_async && self.try_emit_range_for(target, iter, body, else_body) {
            return;
        }
        // #91: unique per-loop break flag, set by `break` (see
        // StmtKind::Break) so the else clause is suppressed on break.
        let flag = else_body.as_ref().map(|_| {
            let f = format!("__for_broke_{}", self.loop_flag_counter);
            self.loop_flag_counter += 1;
            f
        });
        if let Some(f) = &flag {
            self.writeln(&format!("let {} = false;", f));
        }
        self.loop_flag_stack.push(flag.clone());

        self.write_indent();
        // #262: a for-target REASSIGNED inside the body (`for k, v in ...:
        // v = 99`, or `for i in ...: i = i*2`) can't be a `const` — use a
        // block-scoped `let` (per-iteration, reassignable; Python's rebind
        // doesn't affect the next iteration either).
        // #269 (R17) / #220: a simple-name target already declared in this scope
        // was hoisted (because it is read or reassigned outside the loop) — the
        // loop must WRITE that hoisted binding, not shadow it with a fresh
        // per-iteration declaration, so Python's post-loop variable leak holds.
        // Emit a bare assignment target (no `const`/`let`). Fresh names keep the
        // per-iteration `const`; a target reassigned inside the body needs `let`.
        // #288: same rule for tuple/list destructuring targets — when EVERY
        // name in the pattern is a hoisted function/module-scope `let`, the
        // loop must WRITE those bindings (`for ([a, b] of …)`), not shadow
        // them with a fresh per-iteration `const [a, b]`; Python leaks every
        // name of the tuple target. (`collect_hoisted_names` hoists all names
        // of a reused pattern together, so the all() guard is normally
        // all-or-nothing; a pathological partial pattern keeps the old
        // shadowing path.)
        let bare_target = match &target.kind {
            ExprKind::Name(n) => self.is_hoisted(n),
            ExprKind::Tuple(elts) | ExprKind::List(elts) => {
                let mut tn = Vec::new();
                Self::collect_pattern_names(elts, &mut tn);
                !tn.is_empty() && tn.iter().all(|n| self.is_hoisted(n))
            }
            _ => false,
        };
        let binder = if bare_target {
            ""
        } else if Self::for_target_reassigned(target, body) {
            "let "
        } else {
            "const "
        };
        if is_async {
            self.write("for await (");
        } else {
            self.write("for (");
        }
        self.write(binder);

        // Handle tuple unpacking in for target (#85: nested patterns
        // recurse — `for a, (b, c) in ...` → `for (const [a, [b, c]] of ...)`).
        if let ExprKind::Tuple(elts) | ExprKind::List(elts) = &target.kind {
            // PBT-2 lesson: the pattern is a WRITE position — sentinel-guarded
            // names must emit bare (`[a, ...b]`), never as __pyChkLocal reads.
            let was_lhs = self.in_lhs_target;
            self.in_lhs_target = true;
            self.emit_destructure_pattern(elts);
            self.in_lhs_target = was_lhs;
        } else {
            // PBT-2: the target is a WRITE position — a sentinel-guarded name
            // must emit bare here (`for (v of …)`), not as a __pyChkLocal read.
            let was_lhs = self.in_lhs_target;
            self.in_lhs_target = true;
            self.emit_expr(target);
            self.in_lhs_target = was_lhs;
        }

        // Mark for-loop targets as declared in enclosing scope
        self.declare_target(target);

        self.write(" of ");
        // #83: `for k in d` iterates KEYS in Python; Dict-typed iterables
        // get the shape-dispatching pyDictKeys wrap (plain objects aren't
        // iterable in JS at all; Map-backed dicts would yield keys anyway).
        //
        // Round-4 sweep: `async for` over a Python-protocol object
        // (__aiter__/__anext__ + StopAsyncIteration) isn't a JS async
        // iterable — bridge it. Native async iterables (async generators)
        // pass through the helper untouched.
        if is_async {
            // #239: an async iterable is bridged by __pyAsyncIter (async
            // protocol / Symbol.asyncIterator); it is never a dict, so it must
            // NOT go through emit_iterable's pyForIter/pyDictKeys sync wrapping.
            self.need_runtime("__pyAsyncIter");
            self.write("__pyAsyncIter(");
            self.emit_expr(iter);
            self.write(")");
        } else {
            self.emit_iterable(iter);
        }
        self.write(") {\n");
        self.indent += 1;
        for stmt in body {
            self.emit_stmt(stmt);
        }
        self.indent -= 1;
        self.writeln("}");
        self.loop_flag_stack.pop();

        if let Some(stmts) = else_body {
            self.writeln(&format!("if (!{}) {{", flag.as_ref().unwrap()));
            self.indent += 1;
            for stmt in stmts {
                self.emit_stmt(stmt);
            }
            self.indent -= 1;
            self.writeln("}");
        }
    }

    /// Auto-import a builtin exception class used at a raise-site.
    /// `TypeError` is skipped — it is a real JS global; importing the
    /// runtime's class would only shadow it (same rule as except-sites).
    fn import_builtin_exception(&mut self, name: &str) {
        if is_builtin_exception(name) && name != "TypeError" {
            self.runtime_imports.insert(name.to_string());
        }
    }

    /// Emit one operand of a `raise` statement (the value or the `from`
    /// cause). A bare class name (`raise StopIteration`) is instantiated the
    /// way Python does it — round-3 sweep found it emitted as an undefined
    /// bare name; round-4 extends the same rule to the `from Y` cause.
    fn emit_raise_operand(&mut self, expr: &Expr) {
        if let ExprKind::Name(name) = &expr.kind {
            if is_builtin_exception(name) || self.known_classes.contains(name) {
                self.import_builtin_exception(name);
                self.write(&format!("new {}()", Self::sanitize_ident(name)));
                return;
            }
        }
        self.emit_expr(expr);
    }

    fn emit_try(
        &mut self,
        body: &[Stmt],
        handlers: &[ExceptHandler],
        else_body: &Option<Vec<Stmt>>,
        finally_body: &Option<Vec<Stmt>>,
    ) {
        // Python's `else` clause runs only when the try body completed
        // without raising — and, critically, exceptions raised *in* the else
        // clause are NOT caught by this try's handlers. Inlining the else
        // body at the end of the try block (the old lowering) got the
        // when-it-runs part right but wrongly routed else-raised exceptions
        // into the handlers (found by the round-4 sweep). Lower with a
        // completion flag instead:
        //
        //     let __else_N = false;
        //     try { <body> __else_N = true; } catch (__exc) { ... }
        //     if (__else_N) { <else body> }
        //
        // A `return`/`break`/`continue` in the body skips the flag set, so
        // the else clause is skipped — exactly CPython's rule. When a
        // `finally` also exists, the try/catch + else-if nest inside an
        // outer try/finally so the else clause still runs before finally
        // and its exceptions still pass through finally.
        let else_flag = if else_body.is_some() {
            let n = self.default_hoist_counter;
            self.default_hoist_counter += 1;
            let flag = format!("__else_{}", n);
            self.writeln(&format!("let {} = false;", flag));
            Some(flag)
        } else {
            None
        };
        let outer_finally = finally_body.is_some() && else_body.is_some();
        if outer_finally {
            self.writeln("try {");
            self.indent += 1;
        }

        self.writeln("try {");
        self.indent += 1;
        for stmt in body {
            self.emit_stmt(stmt);
        }
        if let Some(flag) = &else_flag {
            self.writeln(&format!("{} = true;", flag));
        }
        self.indent -= 1;

        if !handlers.is_empty() {
            self.writeln("} catch (__exc) {");
            self.indent += 1;

            for (i, handler) in handlers.iter().enumerate() {
                let keyword = if i == 0 { "if" } else { "} else if" };

                // `except Exception` / `except BaseException` are Python's
                // catch-all bases with no JS runtime class — emitting
                // `__exc instanceof Exception` references an undefined global
                // and throws a ReferenceError the moment the handler runs. Treat
                // them as an unconditional catch (like a bare `except:`).
                let is_catch_all = matches!(
                    &handler.exc_type,
                    Some(t) if matches!(&t.kind, ExprKind::Name(n) if n == "Exception" || n == "BaseException")
                );

                match &handler.exc_type {
                    Some(exc_type) if !is_catch_all => {
                        self.write_indent();
                        self.write(&format!("{} (", keyword));
                        self.emit_except_condition(exc_type);
                        self.write(") {\n");
                    }
                    // Catch-all (`except:`, `except Exception`, `except
                    // BaseException`): open a block so the trailing `}` balances.
                    _ => {
                        if i > 0 {
                            self.writeln("} else {");
                        } else {
                            self.writeln("{");
                        }
                    }
                }

                self.indent += 1;
                if let Some(name) = &handler.name {
                    self.writeln(&format!("let {} = __exc;", name));
                }
                for stmt in &handler.body {
                    self.emit_stmt(stmt);
                }
                self.indent -= 1;
            }

            // Python semantics: an exception no handler matched propagates.
            // Without this else-branch, a non-matching exception was silently
            // SWALLOWED (found by the Twitter clone: `except ValueError:`
            // around a KeyError ate the KeyError). Only needed when no
            // catch-all handler exists (a catch-all already covers the tail).
            let has_catch_all = handlers.iter().any(|h| {
                h.exc_type.is_none()
                    || matches!(&h.exc_type,
                        Some(t) if matches!(&t.kind, ExprKind::Name(n) if n == "Exception" || n == "BaseException"))
            });
            if !has_catch_all {
                self.writeln("} else {");
                self.indent += 1;
                self.writeln("throw __exc;");
                self.indent -= 1;
            }

            self.writeln("}");
            self.indent -= 1;
            // NOTE: the catch block itself is still open here — closed below.
            // (Round-4 sweep: the old code only closed it on the no-finally
            // path, so try/except/finally emitted `} else {…}\n finally {` —
            // a JS SyntaxError from the unbalanced catch.)
        }

        // Exactly one block (the try, or its catch) is open at this point.
        match &else_flag {
            None => {
                if let Some(stmts) = finally_body {
                    self.write_indent();
                    self.write("} finally {\n");
                    self.indent += 1;
                    for stmt in stmts {
                        self.emit_stmt(stmt);
                    }
                    self.indent -= 1;
                    self.writeln("}");
                } else {
                    self.writeln("}");
                }
            }
            Some(flag) => {
                self.writeln("}");
                self.writeln(&format!("if ({}) {{", flag));
                self.indent += 1;
                if let Some(stmts) = else_body {
                    for stmt in stmts {
                        self.emit_stmt(stmt);
                    }
                }
                self.indent -= 1;
                self.writeln("}");
                if outer_finally {
                    self.indent -= 1;
                    self.write_indent();
                    self.write("} finally {\n");
                    self.indent += 1;
                    if let Some(stmts) = finally_body {
                        for stmt in stmts {
                            self.emit_stmt(stmt);
                        }
                    }
                    self.indent -= 1;
                    self.writeln("}");
                }
            }
        }
    }

    /// Emit the match condition for one `except <type>` handler.
    ///
    /// Builtin exceptions need NAME-based matching, not bare `instanceof`:
    /// runtime-raised builtins (pyGetItem's KeyError, ZeroDivisionError, …)
    /// are plain `Error` objects with `.name` set — they are never
    /// `instanceof` the runtime's exception classes. And the class itself was
    /// never auto-imported at except-sites (only at raise-sites), so
    /// `__exc instanceof ValueError` was a ReferenceError the moment the
    /// catch ran (found by the Twitter clone). Builtins now emit
    /// `(__exc != null && (__exc.name === "X" || __exc instanceof X))` with
    /// the runtime class auto-imported — the instanceof leg keeps Python's
    /// parent-catches-subclass semantics for user classes extending a
    /// builtin. User-defined classes keep plain `instanceof`. Tuple form
    /// `except (A, B):` ORs the per-element conditions.
    fn emit_except_condition(&mut self, exc_type: &Expr) {
        match &exc_type.kind {
            ExprKind::Tuple(elts) => {
                for (i, elt) in elts.iter().enumerate() {
                    if i > 0 {
                        self.write(" || ");
                    }
                    self.write("(");
                    self.emit_except_condition(elt);
                    self.write(")");
                }
            }
            ExprKind::Name(name) if is_builtin_exception(name) => {
                // `TypeError` is a real JS global — importing the runtime's
                // class would shadow it for no gain (the name check already
                // matches both the runtime's name-tagged Errors and native
                // JS TypeErrors).
                if name != "TypeError" {
                    self.runtime_imports.insert(name.clone());
                }
                // A JS TDZ error (`let` read before init) surfaces as a native
                // ReferenceError; CPython raises UnboundLocalError there and
                // NameError for a truly-undefined name (UnboundLocalError ⊂
                // NameError). Match ReferenceError so `except NameError:` /
                // `except UnboundLocalError:` catch it like CPython.
                let js_ref_leg = match name.as_str() {
                    "NameError" => {
                        " || __exc.name === \"ReferenceError\""
                    }
                    "UnboundLocalError" => {
                        " || (__exc.name === \"ReferenceError\" && /before initialization/.test(__exc.message || \"\"))"
                    }
                    _ => "",
                };
                // Drift fix (exception hierarchy): match the class name OR any
                // subclass name, so `except LookupError:` catches a name-tagged
                // KeyError raised by the inline runtime (`pyths run`) exactly as
                // the `instanceof` leg catches a real KeyError under `compile`.
                let names = builtin_exception_descendants(name);
                let name_legs = if names.len() > 1 {
                    let checks: Vec<String> = names
                        .iter()
                        .map(|n| format!("__exc.name === \"{}\"", n))
                        .collect();
                    format!("({})", checks.join(" || "))
                } else {
                    format!("__exc.name === \"{}\"", name)
                };
                self.write(&format!(
                    "__exc != null && ({} || __exc instanceof {}{})",
                    name_legs, name, js_ref_leg
                ));
            }
            _ => {
                self.write("__exc instanceof ");
                self.emit_expr(exc_type);
            }
        }
    }

    fn emit_with(&mut self, items: &[WithItem], body: &[Stmt], is_async: bool) {
        // Round-3 pythonic sweep: real context-manager protocol. The bound
        // name is __enter__'s RESULT (not the manager); __exit__ runs on
        // every exit path and can suppress the exception by returning a
        // truthy value. Managers without the protocol keep the previous
        // best-effort `close()` behavior (JS interop). Multiple items nest
        // like Python's equivalent nested `with` blocks.
        //
        // Round-4 sweep: `async with` dispatches the ASYNC protocol
        // (`__aenter__`/`__aexit__`, both awaited) — it previously fell
        // through to the sync probe, silently skipping the manager.
        self.emit_with_level(items, 0, body, is_async);
    }

    fn emit_with_level(&mut self, items: &[WithItem], idx: usize, body: &[Stmt], is_async: bool) {
        if idx >= items.len() {
            for stmt in body {
                self.emit_stmt(stmt);
            }
            return;
        }
        let item = &items[idx];
        let n = self.default_hoist_counter;
        self.default_hoist_counter += 1;
        let mgr = format!("__cm{}", n);
        let (enter, exit, aw) = if is_async {
            ("__aenter__", "__aexit__", "await ")
        } else {
            ("__enter__", "__exit__", "")
        };

        self.write_indent();
        self.write(&format!("const {} = ", mgr));
        self.emit_expr(&item.context_expr);
        self.write(";\n");
        self.write_indent();
        if let Some(var) = &item.optional_var {
            self.write("const ");
            // PBT-2: write position — never guard-wrap a sentinel name here.
            let was_lhs = self.in_lhs_target;
            self.in_lhs_target = true;
            self.emit_expr(var);
            self.in_lhs_target = was_lhs;
            self.write(&format!(
                " = ({m} !== null && typeof {m}.{e} === \"function\") ? {aw}{m}.{e}() : {m};\n",
                m = mgr,
                e = enter,
                aw = aw
            ));
        } else {
            self.write(&format!(
                "if ({m} !== null && typeof {m}.{e} === \"function\") {aw}{m}.{e}();\n",
                m = mgr,
                e = enter,
                aw = aw
            ));
        }
        self.writeln(&format!("let {}_exc = null;", mgr));
        self.writeln("try {");
        self.indent += 1;
        self.emit_with_level(items, idx + 1, body, is_async);
        self.indent -= 1;
        self.writeln(&format!("}} catch ({}_e) {{", mgr));
        self.indent += 1;
        self.writeln(&format!("{m}_exc = {m}_e;", m = mgr));
        self.writeln(&format!(
            "if ({m} !== null && typeof {m}.{x} === \"function\") {{ if (!({aw}{m}.{x}({m}_e, {m}_e, null))) throw {m}_e; }}",
            m = mgr, x = exit, aw = aw
        ));
        self.writeln(&format!("else {{ {m}?.close?.(); throw {m}_e; }}", m = mgr));
        self.indent -= 1;
        self.writeln("} finally {");
        self.indent += 1;
        self.writeln(&format!(
            "if ({m}_exc === null) {{ if ({m} !== null && typeof {m}.{x} === \"function\") {aw}{m}.{x}(null, null, null); else {m}?.close?.(); }}",
            m = mgr, x = exit, aw = aw
        ));
        self.indent -= 1;
        self.writeln("}");
    }

    fn emit_match(&mut self, subject: &Expr, cases: &[MatchCase]) {
        // #324: unique per-statement subject name. `const __match = ...` at a
        // fixed name collided when two sibling match statements shared a block
        // scope (module top level, or the same function body) → "Identifier
        // '__match' has already been declared". A per-statement counter (the
        // default_hoist_counter precedent) keeps each match's binding distinct
        // and also lets a nested match inside a case body not shadow the outer.
        let subj = format!("__match{}", self.default_hoist_counter);
        self.default_hoist_counter += 1;
        self.write_indent();
        self.write(&format!("const {} = ", subj));
        self.emit_expr(subject);
        self.write(";\n");

        for (i, case) in cases.iter().enumerate() {
            self.write_indent();
            if i == 0 {
                self.write("if (");
            } else {
                self.write("} else if (");
            }

            // Wildcard case _ is always true
            if matches!(&case.pattern, Pattern::Wildcard) {
                self.write("true");
            } else {
                self.emit_pattern_condition(&case.pattern, &subj);
            }

            // Guard clause
            if let Some(guard) = &case.guard {
                self.write(" && (");
                self.emit_expr(guard);
                self.write(")");
            }

            self.write(") {\n");
            self.indent += 1;

            // Emit pattern bindings
            self.emit_pattern_bindings(&case.pattern, &subj);

            for stmt in &case.body {
                self.emit_stmt(stmt);
            }
            self.indent -= 1;
        }

        if !cases.is_empty() {
            self.writeln("}");
        }
    }

    /// Emit the condition check for a pattern against a given JS expression.
    fn emit_pattern_condition(&mut self, pattern: &Pattern, subject: &str) {
        match pattern {
            Pattern::Wildcard | Pattern::Capture(_) => {
                self.write("true");
            }
            Pattern::Literal(expr) => {
                self.write(&format!("{} === ", subject));
                self.emit_expr(expr);
            }
            Pattern::Or(alternatives) => {
                self.write("(");
                for (i, alt) in alternatives.iter().enumerate() {
                    if i > 0 {
                        self.write(" || ");
                    }
                    self.emit_pattern_condition(alt, subject);
                }
                self.write(")");
            }
            Pattern::Sequence(patterns) => {
                // Round-2 pythonic sweep: a star sub-pattern makes the
                // length check a MINIMUM (`case [a, *rest]:` matches any
                // length >= 1); fixed elements after the star index from
                // the end.
                let star_idx = patterns.iter().position(|p| matches!(p, Pattern::Star(_)));
                if let Some(si) = star_idx {
                    self.write(&format!(
                        "(Array.isArray({}) && {}.length >= {}",
                        subject,
                        subject,
                        patterns.len() - 1
                    ));
                    for (i, pat) in patterns.iter().enumerate() {
                        if !matches!(
                            pat,
                            Pattern::Wildcard | Pattern::Capture(_) | Pattern::Star(_)
                        ) {
                            let elem = if i < si {
                                format!("{}[{}]", subject, i)
                            } else {
                                format!("{}[{}.length - {}]", subject, subject, patterns.len() - i)
                            };
                            self.write(" && ");
                            self.emit_pattern_condition(pat, &elem);
                        }
                    }
                } else {
                    self.write(&format!(
                        "(Array.isArray({}) && {}.length === {}",
                        subject,
                        subject,
                        patterns.len()
                    ));
                    for (i, pat) in patterns.iter().enumerate() {
                        if !matches!(
                            pat,
                            Pattern::Wildcard | Pattern::Capture(_) | Pattern::Star(_)
                        ) {
                            self.write(" && ");
                            self.emit_pattern_condition(pat, &format!("{}[{}]", subject, i));
                        }
                    }
                }
                self.write(")");
            }
            Pattern::Mapping(pairs) => {
                // Round-2 pythonic sweep: `k in subject` throws TypeError
                // on primitive subjects (`case {'k': v}` vs a string), so
                // guard the shape first; Map-backed PyDicts need `.has`
                // instead of `in`. The shape guard alone also makes the
                // empty mapping pattern (`case {}:`) valid JS.
                self.write(&format!(
                    "({s} !== null && typeof {s} === \"object\" && !Array.isArray({s})",
                    s = subject
                ));
                for (key, _pat) in pairs.iter() {
                    self.write(&format!(
                        " && ({} instanceof Map ? {}.has(",
                        subject, subject
                    ));
                    self.emit_expr(key);
                    self.write(") : (");
                    self.emit_expr(key);
                    self.write(&format!(" in {}))", subject));
                }
                self.write(")");
            }
            Pattern::Class { cls, args } => {
                // Round-2 pythonic sweep: builtin type names have no JS
                // value (`__match instanceof int` → ReferenceError), so
                // class patterns on builtins dispatch through the same
                // __pyIsInstance string sentinels isinstance() uses.
                let is_builtin_ty = matches!(
                    cls.as_str(),
                    "list" | "tuple" | "str" | "int" | "float" | "bool" | "dict" | "set"
                );
                if is_builtin_ty {
                    self.need_runtime("__pyIsInstance");
                    self.write(&format!("(__pyIsInstance({}, \"{}\")", subject, cls));
                } else {
                    self.write(&format!("({} instanceof {}", subject, cls));
                }
                // For class patterns with args, check positional fields
                // This is simplified — real impl would need class field metadata
                for (i, pat) in args.iter().enumerate() {
                    if !matches!(pat, Pattern::Wildcard | Pattern::Capture(_)) {
                        self.write(" && ");
                        self.emit_pattern_condition(
                            pat,
                            &format!("Object.values({})[{}]", subject, i),
                        );
                    }
                }
                self.write(")");
            }
            Pattern::Value(expr) => {
                self.write(&format!("{} === ", subject));
                self.emit_expr(expr);
            }
            Pattern::As { pattern, .. } => {
                self.emit_pattern_condition(pattern, subject);
            }
            Pattern::Star(_) => {
                self.write("true");
            }
        }
    }

    /// Emit variable bindings extracted from a pattern.
    fn emit_pattern_bindings(&mut self, pattern: &Pattern, subject: &str) {
        match pattern {
            Pattern::Capture(name) => {
                // PBT-2: a capture name with a genuine function/module-scope
                // `let` (hoisted — e.g. a sentinel-initialized for-target)
                // must WRITE that binding (Python function-scopes it), not
                // shadow it with a case-block `let` — the old shadowing left
                // the outer binding untouched (post-match reads saw a stale
                // value/None; with sentinel-guarding it would false-raise).
                // NOTE: is_hoisted, not is_declared — a per-iteration `const`
                // for-target is marked declared without any function-scope
                // binding to write.
                if self.is_hoisted(name) {
                    self.writeln(&format!("{} = {};", Self::sanitize_ident(name), subject));
                } else {
                    self.writeln(&format!("let {} = {};", name, subject));
                    self.declare(name);
                }
            }
            Pattern::Sequence(patterns) => {
                // Star-aware (round-2): `case [first, *rest, last]:` binds
                // `rest` to the middle slice and indexes trailing fixed
                // elements from the end.
                let star_idx = patterns.iter().position(|p| matches!(p, Pattern::Star(_)));
                if let Some(si) = star_idx {
                    let post = patterns.len() - si - 1;
                    for (i, pat) in patterns.iter().enumerate() {
                        match pat {
                            Pattern::Star(Some(name)) => {
                                // PBT-2: same hoisted-name rule as Capture.
                                let binder = if self.is_hoisted(name) { "" } else { "let " };
                                self.writeln(&format!(
                                    "{}{} = {}.slice({}, {}.length - {});",
                                    binder,
                                    Self::sanitize_ident(name),
                                    subject,
                                    si,
                                    subject,
                                    post
                                ));
                                self.declare(name);
                            }
                            Pattern::Star(None) => {}
                            _ if i < si => {
                                self.emit_pattern_bindings(pat, &format!("{}[{}]", subject, i))
                            }
                            _ => self.emit_pattern_bindings(
                                pat,
                                &format!(
                                    "{}[{}.length - {}]",
                                    subject,
                                    subject,
                                    patterns.len() - i
                                ),
                            ),
                        }
                    }
                } else {
                    for (i, pat) in patterns.iter().enumerate() {
                        self.emit_pattern_bindings(pat, &format!("{}[{}]", subject, i));
                    }
                }
            }
            Pattern::Mapping(pairs) => {
                for (key, pat) in pairs {
                    if let ExprKind::StringLiteral(s) = &key.kind {
                        // Map-aware access (round-2): Map-backed PyDicts
                        // read via .get, plain-object dicts via subscript.
                        self.emit_pattern_bindings(
                            pat,
                            &format!(
                                "({subj} instanceof Map ? {subj}.get(\"{k}\") : {subj}[\"{k}\"])",
                                subj = subject,
                                k = s
                            ),
                        );
                    }
                }
            }
            Pattern::Class { args, .. } => {
                for (i, pat) in args.iter().enumerate() {
                    self.emit_pattern_bindings(pat, &format!("Object.values({})[{}]", subject, i));
                }
            }
            Pattern::Or(alternatives) => {
                // Bind from the first alternative that has captures
                if let Some(alt) = alternatives.first() {
                    self.emit_pattern_bindings(alt, subject);
                }
            }
            Pattern::As { pattern, name } => {
                // PBT-2: same hoisted-name rule as Capture.
                if self.is_hoisted(name) {
                    self.writeln(&format!("{} = {};", Self::sanitize_ident(name), subject));
                } else {
                    self.writeln(&format!("let {} = {};", name, subject));
                    self.declare(name);
                }
                self.emit_pattern_bindings(pattern, subject);
            }
            Pattern::Wildcard | Pattern::Literal(_) | Pattern::Value(_) | Pattern::Star(_) => {}
        }
    }

    // ── Expressions ───────────────────────────────────────

    /// Depth-guarded entry point for the codegen expression walk. Every
    /// recursive `self.emit_expr(...)` call funnels through here, so a single
    /// guard bounds the walk. On overflow it records the offending span and
    /// emits a runtime-throwing placeholder (keeping the output syntactically
    /// valid) instead of recursing into a native stack overflow; the driver
    /// turns the recorded overflow into a clean compile error.
    fn emit_expr(&mut self, expr: &Expr) {
        let _guard = match EmitDepthGuard::enter() {
            Some(g) => g,
            None => {
                self.record_emit_overflow(expr.span.start);
                return;
            }
        };
        self.emit_expr_inner(expr);
    }

    /// Record a codegen depth-overflow and emit a placeholder that throws at
    /// runtime, so a program that is nonetheless executed fails cleanly rather
    /// than silently. The first overflow offset wins (it's the outermost).
    fn record_emit_overflow(&mut self, offset: usize) {
        EMIT_OVERFLOW.with(|c| {
            if c.get().is_none() {
                c.set(Some(offset));
            }
        });
        self.write(&format!(
            "(()=>{{throw new RangeError(\"expression nested too deeply (max {} levels)\");}})()",
            MAX_EMIT_DEPTH
        ));
    }

    fn emit_expr_inner(&mut self, expr: &Expr) {
        self.mark_mapping(expr.span.start);
        match &expr.kind {
            ExprKind::IntLiteral(n) => {
                // Small ints stay native Number (fast: indices, counters);
                // literals beyond 2**53 can't be represented exactly as a
                // JS Number, so emit a BigInt literal. Arithmetic helpers
                // keep the two representations interoperable and promote/
                // demote across the 2**53 boundary.
                if n.unsigned_abs() > 9_007_199_254_740_991 {
                    self.write(&format!("{}n", n));
                } else {
                    self.write(&n.to_string());
                }
            }
            ExprKind::FloatLiteral(n) => self.write(&format!("{}", n)),
            // #283: imaginary literal -> runtime complex value `pyComplex(0, n)`.
            // Arithmetic (`3 + 4j`) then flows through pyAdd/pySub/pyMul, which
            // dispatch to PyComplex's __radd__/__add__ etc.
            ExprKind::ImagLiteral(n) => {
                self.need_runtime("pyComplex");
                self.write(&format!("pyComplex(0, {})", n));
            }
            ExprKind::StringLiteral(s) => {
                self.write(&format!("\"{}\"", escape_js_string(s)));
            }
            ExprKind::FString { parts } => {
                self.write("`");
                for part in parts {
                    match part {
                        FStringPart::Literal(s) => {
                            self.write(&escape_template_literal(s));
                        }
                        FStringPart::Expr(e) => {
                            // A4: route every interpolated expression
                            // through pyStr so bool/None/floats/containers
                            // print CPython-style instead of JS's implicit
                            // template-literal ToString (`${true}` -> "true",
                            // not "True"). Safe for the format-spec case too
                            // (`{x:.2f}` lowers to `.toFixed(2)` or a
                            // pyFormatSpec(...) call, both already strings —
                            // pyStr(string) is an identity passthrough).
                            //
                            // Statically-known-float exprs go through
                            // pyFormatFloat directly instead (see the A4
                            // note above emit_call's print/str/repr
                            // handling) — pyStr's runtime number branch
                            // can't always tell a whole float from a small
                            // int by value alone.
                            self.write("${");
                            if self.is_definitely_float(e) {
                                self.need_runtime("pyFormatFloat");
                                self.write("pyFormatFloat(");
                                self.emit_expr(e);
                                self.write(")");
                            } else {
                                self.need_runtime("pyStr");
                                self.write("pyStr(");
                                self.emit_expr(e);
                                self.write(")");
                            }
                            self.write("}");
                        }
                    }
                }
                self.write("`");
            }
            ExprKind::BoolLiteral(b) => {
                self.write(if *b { "true" } else { "false" });
            }
            ExprKind::NoneLiteral => self.write("null"),
            ExprKind::Name(name) => {
                if name == "self" {
                    self.write("this");
                } else {
                    // Synthetic runtime-helper references injected by
                    // earlier lowering passes (e.g. pyFormatSpec from
                    // f-string spec lowering) need to be imported even
                    // though they appear as bare Name nodes in the AST.
                    if matches!(
                        name.as_str(),
                        // pyRepr/pyStr: injected by f-string `!r`/`!s`
                        // conversions and `{x=}` self-doc (Pythonic-checks).
                        "pyFormatSpec"
                            | "pyFormatDynamic"
                            | "pyNormalizeStyle"
                            | "pyFixed"
                            | "pyRepr"
                            | "pyStr"
                    ) && !self.is_declared(name)
                    {
                        self.need_runtime(name);
                    }
                    // #110: Python builtins referenced as VALUES (not
                    // called) — defaultdict(list), starmap(pow),
                    // key=len, map(int, ...). Previously these passed
                    // through as bare JS identifiers → ReferenceError.
                    // Shadowing (params, locals, imports, user defs)
                    // wins via the is_declared guard.
                    if !self.is_declared(name) {
                        if let Some((js, deps)) = crate::builtins::builtin_value_mapping(name) {
                            for d in deps {
                                self.need_runtime(d);
                            }
                            self.write(js);
                            return;
                        }
                    }
                    // Track-B: a READ of `undefined` that the user never
                    // bound refers to the JS global. sanitize_ident would
                    // rename it to `undefined$` — a silent ReferenceError.
                    // The load-bearing interop idiom is `x ?? undefined`
                    // for libraries that distinguish null from undefined
                    // (cva opts OUT of defaultVariants on null). A user
                    // binding `undefined = ...` still sanitizes via the
                    // is_declared guard.
                    if name == "undefined" && !self.is_declared(name) {
                        self.write("undefined");
                        return;
                    }
                    // Names imported from React-like modules without an
                    // alias were camelCased on the import line. Match
                    // that on the reference side so the JS binding
                    // resolves cleanly. Without this, `from foo import
                    // use_query; use_query()` emits `import { useQuery }
                    // from "foo"` followed by `use_query()` — the local
                    // binding mismatches and crashes at runtime.
                    if self.react_imports.contains(name) {
                        self.write(&react::snake_to_camel(name));
                    } else if !self.in_lhs_target && self.is_sentinel(name) {
                        // PBT-2: READ of a sentinel-initialized for-target —
                        // guard it so a zero-iteration loop raises
                        // UnboundLocalError/NameError like CPython. Writes
                        // (in_lhs_target) stay bare so assignments and the
                        // loop binding itself overwrite the sentinel.
                        let helper = if self.at_module_scope() {
                            "__pyChkGlobal"
                        } else {
                            "__pyChkLocal"
                        };
                        self.need_runtime(helper);
                        self.write(&format!(
                            "{}({}, \"{}\")",
                            helper,
                            Self::sanitize_ident(name),
                            name
                        ));
                    } else {
                        self.write(&Self::sanitize_ident(name));
                    }
                }
            }
            ExprKind::BinOp { left, op, right } => {
                self.emit_binop(left, *op, right);
            }
            ExprKind::UnaryOp { op, operand } => {
                self.emit_unary(*op, operand);
            }
            ExprKind::Compare { left, comparisons } => {
                self.emit_comparison(left, comparisons);
            }
            ExprKind::Call {
                func,
                args,
                kwargs,
                optional,
            } => {
                // Round-4 sweep: `asyncio.run(coro)` is Python's sync↔async
                // boundary — it BLOCKS until the coroutine finishes. The JS
                // shim returns a Promise, so without an await the program
                // reads results before they exist. ESM has top-level await;
                // wrap the call whenever awaiting is legal here (module top
                // level or an async function body). Inside sync functions
                // the Promise passes through unchanged (documented limit).
                let is_asyncio_run = match &func.kind {
                    ExprKind::Attribute { value, attr, .. } if attr == "run" => {
                        matches!(&value.kind, ExprKind::Name(n) if self.asyncio_namespaces.contains(n))
                    }
                    ExprKind::Name(n) => self.asyncio_run_fns.contains(n),
                    _ => false,
                };
                let wrap_await = is_asyncio_run && self.await_ok;
                if wrap_await {
                    self.write("(await ");
                }
                self.emit_call(func, args, kwargs, *optional);
                if wrap_await {
                    self.write(")");
                }
            }
            ExprKind::Attribute {
                value,
                attr,
                optional,
            } => {
                // Round-4 sweep: `X.__name__` — the codegen stamps
                // `__name__` on compiled classes and the runtime does the
                // same for its exception classes, but native JS classes
                // (TypeError, …) and functions only carry `.name`. Read
                // through with a fallback so `type(e).__name__` /
                // `f.__name__` work across all of them.
                if attr == "__name__" && !*optional {
                    self.write("((__o) => __o?.__name__ ?? __o?.name)(");
                    self.emit_expr(value);
                    self.write(")");
                    return;
                }
                // #266: a dict lookup method read as a VALUE (`g = d.get`,
                // `key=d.get`, `key=counter.get`) is a BOUND method — bind it to
                // its receiver so a detached call keeps `this`. Scoped to the
                // dict-lookup methods (which are the real extract-as-callback
                // case and don't collide with common data fields / array
                // methods like `.items`/`.map`); a stdlib module namespace is
                // excluded (its members are plain functions).
                if !*optional
                    && matches!(attr.as_str(), "get" | "setdefault")
                    && !matches!(&value.kind, ExprKind::Name(n) if self.module_namespaces.contains(n))
                {
                    self.need_runtime("pyBoundMethod");
                    self.write("pyBoundMethod(");
                    self.emit_expr(value);
                    self.write(&format!(", {:?})", attr));
                    return;
                }
                // `123.foo` is a JS syntax error (the lexer eats `123.`
                // as a numeric literal). Wrap int-literal receivers in
                // parens. Floats already have a `.` and are unaffected.
                let needs_paren = matches!(&value.kind, ExprKind::IntLiteral(_));
                if needs_paren {
                    self.write("(");
                }
                self.emit_expr(value);
                if needs_paren {
                    self.write(")");
                }
                if *optional {
                    self.write(&format!("?.{}", attr));
                } else {
                    self.write(&format!(".{}", attr));
                }
            }
            ExprKind::Subscript {
                value,
                index,
                optional,
            } => {
                // Capture + reset the LHS flag — the index sub-expression
                // is always a read context, even when the outer subscript
                // is being assigned to.
                let is_lhs = self.in_lhs_target;
                self.in_lhs_target = false;
                // Credible compilation (§7.2): the routing decision is
                // made by cert::route — the single decision procedure,
                // mirrored and safety-proved in Lean — and recorded into
                // the certificate. The emission below MATCHES on the
                // decided route rather than re-deriving it, so the
                // certificate and the artifact cannot disagree by
                // construction.
                let is_slice = matches!(index.kind, ExprKind::Slice { .. });
                // Record the STATIC EVIDENCE for the in-bounds fast path
                // (list-literal length + int-literal index) and derive the
                // `provably_inbounds` bit from it. The bit's predicate is
                // unchanged (so emitted JS is byte-identical), but the
                // certificate now carries the evidence so `check_certificate`
                // re-derives the bound instead of trusting the bit (Gap-2).
                let inbounds_evidence = if !is_lhs && !*optional && !is_slice {
                    if let (ExprKind::List(elts), ExprKind::IntLiteral(i)) =
                        (&value.kind, &index.kind)
                    {
                        Some(crate::cert::InboundsEvidence {
                            list_len: elts.len(),
                            index: *i,
                        })
                    } else {
                        None
                    }
                } else {
                    None
                };
                let provably_inbounds = inbounds_evidence
                    .map(crate::cert::InboundsEvidence::is_inbounds)
                    .unwrap_or(false);
                let recv_ty = if is_slice {
                    crate::cert::RecvTy::Unknown
                } else {
                    Self::recv_ty_of(self.infer_type(value))
                };
                let site = crate::cert::SiteInput {
                    is_slice,
                    is_lhs,
                    is_optional: *optional,
                    provably_inbounds,
                    inbounds_evidence,
                    recv_ty,
                };
                let decided = crate::cert::route(site);
                // Record the BODY-relative offset where this site's lowering
                // begins so `check_certificate` can positionally verify the
                // emitted JS matches the promised route (route-swap detector).
                // `js_end` is filled in AFTER the lowering block below; the
                // emitted bytes are NOT altered — only observed.
                let site_idx = self.certificate.sites.len();
                let js_start = self.output.len();
                self.certificate.sites.push(crate::cert::SiteRecord {
                    start: expr.span.start,
                    end: expr.span.end,
                    input: site,
                    route: decided,
                    js_start: Some(js_start),
                    js_end: None,
                });
                // Check for slice
                if let ExprKind::Slice { lower, upper, step } = &index.kind {
                    self.need_runtime("pySlice");
                    self.write("pySlice(");
                    self.emit_expr(value);
                    self.write(", ");
                    if let Some(l) = lower {
                        self.emit_expr(l);
                    } else {
                        self.write("null");
                    }
                    self.write(", ");
                    if let Some(u) = upper {
                        self.emit_expr(u);
                    } else {
                        self.write("null");
                    }
                    self.write(", ");
                    if let Some(s) = step {
                        self.emit_expr(s);
                    } else {
                        self.write("null");
                    }
                    self.write(")");
                } else {
                    match decided {
                        crate::cert::Route::PySlice => unreachable!("slice handled above"),
                        crate::cert::Route::NativeInbounds => {
                            // Fast path (Issue #22 follow-up B): a list
                            // literal indexed by a non-negative integer
                            // literal statically < the list length —
                            // provably in-bounds, so JS native `x[i]`
                            // can never return `undefined`. This is the
                            // ONLY safe case where pyGetItem is skipped
                            // for list reads.
                            self.emit_expr(value);
                            self.write("[");
                            self.emit_expr(index);
                            self.write("]");
                        }
                        crate::cert::Route::Helper => {
                            // Read-side Python semantics: out-of-range /
                            // missing-key reads throw Python-named
                            // IndexError / KeyError instead of silently
                            // returning undefined.
                            //
                            // Issue #22 attempted a native x[i] path for
                            // typed LIST VARIABLES but reverted it (the
                            // CLI test `test_run_explain_indexerror`
                            // proved the silent-undefined regression).
                            // Per "correctness > savings", pyGetItem is
                            // kept for all typed list/dict/tuple reads.
                            // F2: `Primitive` covers str — negative
                            // indices (`s[-1]`) and astral chars index
                            // by code point through the helper. #83:
                            // Unknown-typed receivers route through it
                            // too (a Map-backed dict on an unannotated
                            // channel must not fall back to raw `d[k]`);
                            // pyGetItem passes non-plain-prototype
                            // objects through natively for interop.
                            // The type-set is pinned by cert::route and
                            // its Lean twin (route_read_safety).
                            self.need_runtime("pyGetItem");
                            self.write("pyGetItem(");
                            self.emit_expr(value);
                            self.write(", ");
                            self.emit_expr(index);
                            self.write(")");
                        }
                        crate::cert::Route::Native => {
                            // LHS target (`a[i] = x`) or optional chain
                            // (`a?.[i]`) only. Every plain READ — of any
                            // receiver type, incl. Float/Set (non-subscriptable
                            // → the helper raises TypeError) — routes to Helper
                            // now; the receiver TYPE no longer forces native.
                            self.emit_expr(value);
                            if *optional {
                                self.write("?.[");
                            } else {
                                self.write("[");
                            }
                            self.emit_expr(index);
                            self.write("]");
                        }
                    }
                }
                // Close the positional record: the lowering for this site
                // occupies BODY offsets [js_start, self.output.len()).
                self.certificate.sites[site_idx].js_end = Some(self.output.len());
            }
            ExprKind::Slice { .. } => {
                // Handled in Subscript above
                self.write("null /* slice */");
            }
            ExprKind::List(elts) => {
                self.write("[");
                for (i, e) in elts.iter().enumerate() {
                    if i > 0 {
                        self.write(", ");
                    }
                    self.emit_expr(e);
                }
                self.write("]");
            }
            ExprKind::Tuple(elts) => {
                // A4 tuple investigation: tuple literals used to compile to
                // a bare array, byte-identical to list codegen — pyRepr's
                // `.__pytuple__` marker check existed but nothing ever set
                // it. Route through the pyTuple(...) helper so print/str/
                // repr can distinguish `(1, 2)` from `[1, 2]`. This is the
                // single codegen site that constructs a tuple *value*;
                // assignment-target unpacking (`a, b = ...`, emit_assign)
                // and for-target unpacking (emit_for_target) are separate
                // paths that never reach this arm, so they're unaffected.
                self.need_runtime("pyTuple");
                self.write("pyTuple(");
                for (i, e) in elts.iter().enumerate() {
                    if i > 0 {
                        self.write(", ");
                    }
                    self.emit_expr(e);
                }
                self.write(")");
            }
            ExprKind::Dict { items } => {
                self.emit_dict_literal(items);
            }
            ExprKind::Set(elts) => {
                // #297: Python set literals build the canonicalizing PySet
                // (bool/int/float hash identity, structural tuple members).
                self.need_runtime("PySet");
                self.write("new PySet([");
                for (i, e) in elts.iter().enumerate() {
                    if i > 0 {
                        self.write(", ");
                    }
                    self.emit_expr(e);
                }
                self.write("])");
            }
            ExprKind::ListComp { elt, generators } => {
                self.emit_list_comprehension(elt, generators);
            }
            ExprKind::DictComp {
                key,
                value,
                generators,
            } => {
                self.emit_dict_comprehension(key, value, generators);
            }
            ExprKind::SetComp { elt, generators } => {
                // #297: canonicalizing PySet (see ExprKind::Set).
                self.need_runtime("PySet");
                self.write("new PySet(");
                self.emit_list_comprehension(elt, generators);
                self.write(")");
            }
            ExprKind::GeneratorExp { elt, generators } => {
                // #155: genexps lower to REAL lazy JS generators (iterator
                // protocol), not eager arrays — `next(genexp, default)`,
                // laziness side-effect ordering, and iter() identity all
                // depend on it.
                self.emit_generator_exp(elt, generators);
            }
            ExprKind::Lambda { params, body } => {
                self.write("(");
                self.emit_params(params);
                self.write(") => ");
                // Round-4 sweep: a plain arrow can't await.
                let prev_await_ok = self.await_ok;
                self.await_ok = false;
                // Declare the lambda's params in a fresh scope so name-resolution
                // inside the body sees them — in particular so a param that
                // shadows a builtin (`lambda set: set(…)`) calls the PARAM, not
                // the `set()` builtin. Mirrors the def-body scope handling.
                self.push_scope();
                for p in params {
                    self.declare(&p.name);
                }
                self.emit_expr(body);
                self.pop_scope();
                self.await_ok = prev_await_ok;
            }
            ExprKind::IfExpr {
                test,
                body,
                else_body,
            } => {
                self.write("(");
                self.emit_test_expr(test);
                self.write(" ? ");
                self.emit_expr(body);
                self.write(" : ");
                self.emit_expr(else_body);
                self.write(")");
            }
            ExprKind::Starred(inner) => {
                self.write("...");
                self.emit_expr(inner);
            }
            ExprKind::Await(inner) => {
                self.write("await ");
                self.emit_expr(inner);
            }
            ExprKind::Yield(value) => {
                self.write("yield");
                if let Some(v) = value {
                    self.write(" ");
                    self.emit_expr(v);
                }
            }
            ExprKind::YieldFrom(inner) => {
                self.write("yield* ");
                self.emit_expr(inner);
            }
            ExprKind::NamedExpr { target, value } => {
                // Walrus operator: (target = value)
                // PBT-2: the target is a WRITE position — a sentinel-guarded
                // name must emit bare (`(i = v)`), not as a __pyChkLocal read
                // (which would be an invalid JS assignment target).
                self.write("(");
                let was_lhs = self.in_lhs_target;
                self.in_lhs_target = true;
                self.emit_expr(target);
                self.in_lhs_target = was_lhs;
                self.write(" = ");
                self.emit_expr(value);
                self.write(")");
            }
        }
    }

    /// Emit a numeric binary op as a runtime-helper call
    /// (`helper(left, right)`), importing the helper. Used for arithmetic
    /// that must stay arbitrary-precision-faithful (Number↔BigInt).
    fn emit_binop_helper(&mut self, helper: &str, left: &Expr, right: &Expr) {
        self.need_runtime(helper);
        self.write(helper);
        self.write("(");
        self.emit_expr(left);
        self.write(", ");
        self.emit_expr(right);
        self.write(")");
    }

    /// #319: like emit_binop_helper but passes a float-context flag (`, true`)
    /// when a statically-known float operand is present. A whole-valued float
    /// (`1.0`) is an indistinguishable JS number at runtime, so the flag tells
    /// the arithmetic helper to coerce a BigInt operand to float (raising
    /// OverflowError when too large) and format the result as a float — the
    /// same disambiguation pyDiv's `floatDiv` flag already carries.
    fn emit_binop_helper_fctx(&mut self, helper: &str, left: &Expr, right: &Expr) {
        self.need_runtime(helper);
        self.write(helper);
        self.write("(");
        self.emit_expr(left);
        self.write(", ");
        self.emit_expr(right);
        if self.is_definitely_float(left) || self.is_definitely_float(right) {
            self.write(", true");
        }
        self.write(")");
    }

    /// #343: like emit_binop_helper but threads a float-context BITMASK
    /// (`1`=left statically float, `2`=right, `3`=both) as a 3rd arg to the
    /// bitwise helper. A whole-valued float (`3.0`) is an indistinguishable JS
    /// number at runtime, so the codegen tells the helper which operand was a
    /// float — CPython rejects ANY float in a bitwise position with TypeError,
    /// regardless of value (`3.0 & 5` → TypeError, not `1`).
    fn emit_bitop_helper_fctx(&mut self, helper: &str, left: &Expr, right: &Expr) {
        let fctx =
            (self.is_definitely_float(left) as u8) | ((self.is_definitely_float(right) as u8) << 1);
        self.need_runtime(helper);
        self.write(helper);
        self.write("(");
        self.emit_expr(left);
        self.write(", ");
        self.emit_expr(right);
        if fctx != 0 {
            self.write(&format!(", {}", fctx));
        }
        self.write(")");
    }

    /// Emit a bare JS binary op `(left <op> right)`. Used by the P2 native
    /// fast path when both operands are provably `float` (always a JS
    /// Number — never BigInt-promoted — so the arbitrary-precision helper
    /// is unnecessary).
    fn emit_binop_bare(&mut self, left: &Expr, op: &str, right: &Expr) {
        self.write("(");
        self.emit_expr(left);
        self.write(" ");
        self.write(op);
        self.write(" ");
        self.emit_expr(right);
        self.write(")");
    }

    /// True when both operands are provably `float` — arithmetic can skip
    /// the BigInt-aware helper and emit a bare op.
    fn both_float(&self, left: &Expr, right: &Expr) -> bool {
        matches!(self.infer_type(left), JsInferredType::Float)
            && matches!(self.infer_type(right), JsInferredType::Float)
    }

    /// Conservative integer interval `[lo, hi]` for `expr`, when it is a
    /// **provably-bounded** int — i.e. one whose value cannot exceed the
    /// safe-integer range no matter the inputs. `None` means unknown or
    /// possibly-unbounded (the common case: any int that flows through a
    /// `Name`, parameter, subscript, or call) → must stay on the
    /// promoting helper to preserve arbitrary precision.
    ///
    /// Sound sources only:
    /// * int literals — exact.
    /// * `len(<list>)` — ECMAScript array indices are uint32, so an array's
    ///   length is ≤ 2³²−1 (well under 2⁵³). NOT applied to `len(str)` /
    ///   unknown, whose length the spec allows up to 2⁵³−1.
    /// * `+ - *` of bounded operands, via interval arithmetic.
    fn int_bound(&self, expr: &Expr) -> Option<(i64, i64)> {
        match &expr.kind {
            ExprKind::IntLiteral(n) => i64::try_from(*n).ok().map(|v| (v, v)),
            ExprKind::UnaryOp {
                op: pyths_syntax::operators::UnaryOp::Neg,
                operand,
            } => {
                let (lo, hi) = self.int_bound(operand)?;
                Some((hi.checked_neg()?, lo.checked_neg()?))
            }
            ExprKind::Call { func, args, .. } => {
                if let ExprKind::Name(n) = &func.kind {
                    if n == "len"
                        && args.len() == 1
                        && matches!(self.infer_type(&args[0]), JsInferredType::List)
                    {
                        return Some((0, 4_294_967_295)); // uint32 max
                    }
                }
                None
            }
            ExprKind::BinOp { left, op, right } => {
                let l = self.int_bound(left)?;
                let r = self.int_bound(right)?;
                combine_int_bound(l, *op, r)
            }
            _ => None,
        }
    }

    /// True when `left <op> right` is provably an int whose result stays
    /// within the safe-integer range — so a bare JS op is exact and the
    /// arbitrary-precision helper can be skipped.
    fn int_arith_provably_safe(&self, left: &Expr, op: BinOp, right: &Expr) -> bool {
        let l = match self.int_bound(left) {
            Some(b) => b,
            None => return false,
        };
        let r = match self.int_bound(right) {
            Some(b) => b,
            None => return false,
        };
        match combine_int_bound(l, op, r) {
            Some((lo, hi)) => lo >= -SAFE_INT_MAX && hi <= SAFE_INT_MAX,
            None => false,
        }
    }

    fn emit_binop(&mut self, left: &Expr, op: BinOp, right: &Expr) {
        match op {
            BinOp::Add => {
                let lt = self.infer_type(left);
                let rt = self.infer_type(right);
                // Python-faithful collection concat: list/tuple/set + same → spread.
                // Plain JS `+` on arrays/sets/dicts coerces to string (`[]+[]` → `""`).
                match (lt, rt) {
                    // Both provably lists → list concat via native spread
                    // (result is a plain list; no tuple-ness at stake). B-019.
                    (JsInferredType::List, JsInferredType::List) => {
                        self.write("[...");
                        self.emit_expr(left);
                        self.write(", ...");
                        self.emit_expr(right);
                        self.write("]");
                    }
                    // tuple+tuple, mixed list↔tuple, and list/tuple↔unknown fall
                    // through to pyAdd (the `_` arm below): it preserves
                    // tuple-ness (tuple+tuple → tuple), raises TypeError on
                    // list+tuple, and never lets JS `+` string-coerce arrays
                    // (B-019, e.g. `s["events"] + [x]` → "[object Object]").
                    // crit-13.
                    (JsInferredType::Set, JsInferredType::Set) => {
                        // #297: canonicalizing PySet result.
                        self.need_runtime("PySet");
                        self.write("new PySet([...");
                        self.emit_expr(left);
                        self.write(", ...");
                        self.emit_expr(right);
                        self.write("])");
                    }
                    // Fast path: float+float is always Number; provably-
                    // bounded int arithmetic can't overflow 2**53 → bare op.
                    (JsInferredType::Float, JsInferredType::Float) => {
                        self.emit_binop_bare(left, "+", right)
                    }
                    _ if self.int_arith_provably_safe(left, BinOp::Add, right) => {
                        self.emit_binop_bare(left, "+", right)
                    }
                    // Numeric / unknown / string `+` routes through pyAdd so
                    // arbitrary-precision ints stay exact (Number↔BigInt
                    // promotion across 2**53) and string concat still works.
                    // #319: float-context flag → BigInt+float overflow raises.
                    _ => self.emit_binop_helper_fctx("pyAdd", left, right),
                }
            }
            BinOp::Sub => {
                if self.both_float(left, right)
                    || self.int_arith_provably_safe(left, BinOp::Sub, right)
                {
                    self.emit_binop_bare(left, "-", right);
                } else {
                    self.emit_binop_helper("pySub", left, right);
                }
            }
            BinOp::Mul => {
                if self.both_float(left, right)
                    || self.int_arith_provably_safe(left, BinOp::Mul, right)
                {
                    self.emit_binop_bare(left, "*", right);
                } else {
                    // #319: float-context flag → BigInt*float overflow raises.
                    self.emit_binop_helper_fctx("pyMul", left, right);
                }
            }
            BinOp::Div => {
                // True division routes through pyDiv: always float + raises
                // ZeroDivisionError (bare `/` yields Infinity / loses the
                // int/BigInt distinction). F4: pass a `floatDiv` flag when an
                // operand is a statically-known float so `1.0/0.0` raises
                // "float division by zero" (a whole-valued float literal
                // compiles to the same JS number as an int, so the runtime
                // alone can't tell them apart).
                self.need_runtime("pyDiv");
                self.write("pyDiv(");
                self.emit_expr(left);
                self.write(", ");
                self.emit_expr(right);
                if self.is_definitely_float(left) || self.is_definitely_float(right) {
                    self.write(", true");
                }
                self.write(")");
            }
            BinOp::FloorDiv => {
                // Route through pyFloorDiv so dividing by zero throws
                // Python's ZeroDivisionError instead of silently
                // producing Infinity. The helper also handles the
                // floor-toward-negative-infinity case Python expects.
                self.need_runtime("pyFloorDiv");
                self.write("pyFloorDiv(");
                self.emit_expr(left);
                self.write(", ");
                self.emit_expr(right);
                self.write(")");
            }
            BinOp::Mod => {
                // Route through pyMod for ZeroDivisionError + sign-of-
                // divisor semantics (((a%b)+b)%b would produce NaN on
                // b===0, which Python catches as an error).
                self.need_runtime("pyMod");
                self.write("pyMod(");
                self.emit_expr(left);
                self.write(", ");
                self.emit_expr(right);
                self.write(")");
            }
            BinOp::Pow => {
                if self.both_float(left, right) {
                    self.emit_binop_bare(left, "**", right);
                } else {
                    // #319: float-context flag → float ** overflow raises
                    // OverflowError (int ** stays exact BigInt).
                    self.emit_binop_helper_fctx("pyPow", left, right);
                }
            }
            // #273: Python `and`/`or` short-circuit on PYTHON truthiness and
            // return the deciding operand. Raw JS `&&`/`||` matches only when the
            // LEFT operand's JS truthiness agrees with Python's — true for scalars
            // (int/bool/str/None/float; empty string is falsy in both). For a
            // container or Unknown left (an empty list/dict/set/deque is JS-truthy
            // but Python-falsy), route through pyAnd/pyOr, which test pyBool and
            // keep the right operand lazy via a thunk.
            BinOp::And => {
                if self.truthiness_agrees(left) {
                    self.write("(");
                    self.emit_expr(left);
                    self.write(" && ");
                    self.emit_expr(right);
                    self.write(")");
                } else {
                    self.need_runtime("pyAnd");
                    self.write("pyAnd(");
                    self.emit_expr(left);
                    self.write(", () => ");
                    self.emit_expr(right);
                    self.write(")");
                }
            }
            BinOp::Or => {
                if self.truthiness_agrees(left) {
                    self.write("(");
                    self.emit_expr(left);
                    self.write(" || ");
                    self.emit_expr(right);
                    self.write(")");
                } else {
                    self.need_runtime("pyOr");
                    self.write("pyOr(");
                    self.emit_expr(left);
                    self.write(", () => ");
                    self.emit_expr(right);
                    self.write(")");
                }
            }
            // #93: `|`/`&`/`^` on sets are Python set union/intersection/
            // symmetric-difference (and `|` merges dicts, PEP 584), but the
            // bare JS operators coerce a Set to NaN → 0 silently. Route
            // through runtime dispatchers unless both operands are provably
            // in-range ints (where the bare op is exact and faster).
            BinOp::BitAnd => {
                if self.int_arith_provably_safe(left, BinOp::BitAnd, right) {
                    self.emit_binop_bare(left, "&", right);
                } else {
                    // #343: float-context flag so a whole-valued float operand
                    // raises TypeError instead of silently bit-anding.
                    self.emit_bitop_helper_fctx("pyBitAnd", left, right);
                }
            }
            BinOp::BitOr => {
                if self.int_arith_provably_safe(left, BinOp::BitOr, right) {
                    self.emit_binop_bare(left, "|", right);
                } else {
                    self.emit_bitop_helper_fctx("pyBitOr", left, right);
                }
            }
            BinOp::BitXor => {
                if self.int_arith_provably_safe(left, BinOp::BitXor, right) {
                    self.emit_binop_bare(left, "^", right);
                } else {
                    self.emit_bitop_helper_fctx("pyBitXor", left, right);
                }
            }
            BinOp::ShiftLeft => {
                // #249: Python shifts are arbitrary-precision; raw JS `<<`
                // truncates to 32 bits and takes the count mod 32.
                // #343: float-context flag → whole-valued float shift TypeErrors.
                self.emit_bitop_helper_fctx("pyShiftLeft", left, right);
            }
            BinOp::ShiftRight => {
                self.emit_bitop_helper_fctx("pyShiftRight", left, right);
            }
            BinOp::In => {
                // `x in container` must dispatch by container type to match
                // Python semantics: arrays → element membership, plain objects
                // → KEY membership, strings → substring, Set/Map → .has.
                // Direct `.includes()` only handles arrays + strings and
                // crashes (TypeError: undefined) on plain objects.
                self.runtime_imports.insert("pyContains".to_string());
                self.write("pyContains(");
                self.emit_expr(right);
                self.write(", ");
                self.emit_expr(left);
                self.write(")");
            }
            BinOp::NotIn => {
                self.runtime_imports.insert("pyContains".to_string());
                self.write("!pyContains(");
                self.emit_expr(right);
                self.write(", ");
                self.emit_expr(left);
                self.write(")");
            }
            BinOp::Is => {
                // `x is None` uses loose `== null` so a JS `undefined` value
                // (e.g. from `dict.get(missing)`) is also treated as None,
                // matching Python semantics. Identity `is` against non-None
                // keeps strict `Object.is`.
                if matches!(left.kind, ExprKind::NoneLiteral)
                    || matches!(right.kind, ExprKind::NoneLiteral)
                {
                    self.write("(");
                    self.emit_expr(left);
                    self.write(" == ");
                    self.emit_expr(right);
                    self.write(")");
                } else {
                    self.write("Object.is(");
                    self.emit_expr(left);
                    self.write(", ");
                    self.emit_expr(right);
                    self.write(")");
                }
            }
            BinOp::IsNot => {
                if matches!(left.kind, ExprKind::NoneLiteral)
                    || matches!(right.kind, ExprKind::NoneLiteral)
                {
                    self.write("(");
                    self.emit_expr(left);
                    self.write(" != ");
                    self.emit_expr(right);
                    self.write(")");
                } else {
                    self.write("!Object.is(");
                    self.emit_expr(left);
                    self.write(", ");
                    self.emit_expr(right);
                    self.write(")");
                }
            }
            BinOp::NullishCoalesce => {
                self.write("(");
                self.emit_expr(left);
                self.write(" ?? ");
                self.emit_expr(right);
                self.write(")");
            }
            BinOp::Lt | BinOp::LtEq | BinOp::Gt | BinOp::GtEq => {
                // Scalar↔scalar (int/float/str/bool/None) keeps the bare JS
                // op — matches Python semantics already and avoids a
                // function-call tax on hot numeric comparisons. Anything
                // else (Unknown-typed values, or known custom-class
                // instances like Decimal/Fraction) routes through the
                // pyLt/pyLe/pyGt/pyGe helpers, which dispatch `__lt__` /
                // `__le__` / `__gt__` / `__ge__` (with reflected fallback)
                // before falling back to bare `<`/`<=`/`>`/`>=` — so
                // primitives flowing through Unknown-typed variables still
                // behave identically. Without this, `<`/`<=`/`>`/`>=` on
                // objects fell through to the catch-all bare-op arm below
                // and never dispatched comparison dunders at all.
                let lt = self.infer_type(left);
                let rt = self.infer_type(right);
                if lt.is_scalar() && rt.is_scalar() {
                    let op_str = match op {
                        BinOp::Lt => "<",
                        BinOp::LtEq => "<=",
                        BinOp::Gt => ">",
                        BinOp::GtEq => ">=",
                        _ => unreachable!(),
                    };
                    self.emit_binop_bare(left, op_str, right);
                } else {
                    let helper = match op {
                        BinOp::Lt => "pyLt",
                        BinOp::LtEq => "pyLe",
                        BinOp::Gt => "pyGt",
                        BinOp::GtEq => "pyGe",
                        _ => unreachable!(),
                    };
                    self.emit_binop_helper(helper, left, right);
                }
            }
            BinOp::Eq | BinOp::NotEq if Self::type_identity_cmp(left, right).is_some() => {
                // #251: `type(x) == int` — the builtin type name lowers to its
                // constructor function, so `pyEq(pyType(x), <fn>)` was always
                // false. Compare the runtime type's __name__ instead.
                let (arg, tyname) = Self::type_identity_cmp(left, right).unwrap();
                let cmp = if matches!(op, BinOp::Eq) {
                    "==="
                } else {
                    "!=="
                };
                self.need_runtime("pyType");
                self.write("(pyType(");
                self.emit_expr(arg);
                self.write(&format!(").__name__ {} {:?})", cmp, tyname));
            }
            BinOp::Eq => {
                let lt = self.infer_type(left);
                let rt = self.infer_type(right);
                // Python `==` on lists/dicts/sets/tuples is element-wise;
                // on custom-class instances (e.g. Decimal/Fraction) it
                // dispatches `__eq__`. JS `===` is reference equality —
                // wrap in pyEq whenever either side isn't provably a
                // scalar (int/float/str/bool/None), i.e. for collections
                // AND Unknown-typed values (which may hold a dunder-
                // bearing object at runtime). Scalar↔scalar keeps strict
                // `===`, which already matches Python for those and skips
                // the function-call tax on hot comparisons. Without the
                // Unknown case here, `Decimal('0.3') == Decimal('0.3')`
                // (two distinct instances) fell through to bare `===`
                // reference equality and was always false.
                // #241: a bool literal breaks the `===` fast path — `True === 1`
                // is false in JS but Python `True == 1` is true (bool ⊂ int).
                let bool_lit = matches!(&left.kind, ExprKind::BoolLiteral(_))
                    || matches!(&right.kind, ExprKind::BoolLiteral(_));
                if !(lt.is_scalar() && rt.is_scalar()) || bool_lit {
                    self.need_runtime("pyEq");
                    self.write("pyEq(");
                    self.emit_expr(left);
                    self.write(", ");
                    self.emit_expr(right);
                    self.write(")");
                } else {
                    self.write("(");
                    self.emit_expr(left);
                    self.write(" === ");
                    self.emit_expr(right);
                    self.write(")");
                }
            }
            BinOp::NotEq => {
                let lt = self.infer_type(left);
                let rt = self.infer_type(right);
                let bool_lit = matches!(&left.kind, ExprKind::BoolLiteral(_))
                    || matches!(&right.kind, ExprKind::BoolLiteral(_));
                if !(lt.is_scalar() && rt.is_scalar()) || bool_lit {
                    self.need_runtime("pyEq");
                    self.write("(!pyEq(");
                    self.emit_expr(left);
                    self.write(", ");
                    self.emit_expr(right);
                    self.write("))");
                } else {
                    self.write("(");
                    self.emit_expr(left);
                    self.write(" !== ");
                    self.emit_expr(right);
                    self.write(")");
                }
            }
            BinOp::Pipeline => {
                // data |> f       → f(data)
                // data |> f(a, b) → f(data, a, b)
                match &right.kind {
                    ExprKind::Call {
                        func, args, kwargs, ..
                    } => {
                        self.emit_expr(func);
                        self.write("(");
                        self.emit_expr(left);
                        for arg in args {
                            self.write(", ");
                            self.emit_expr(arg);
                        }
                        for kw in kwargs {
                            self.write(", ");
                            if let Some(name) = &kw.name {
                                self.write(name);
                                self.write(": ");
                            }
                            self.emit_expr(&kw.value);
                        }
                        self.write(")");
                    }
                    _ => {
                        // Bare name or other expression: treat as f(data)
                        self.emit_expr(right);
                        self.write("(");
                        self.emit_expr(left);
                        self.write(")");
                    }
                }
            }
        }
    }

    fn emit_unary(&mut self, op: UnaryOp, operand: &Expr) {
        match op {
            UnaryOp::Neg => {
                // Scalar operands (int/float) keep the bare JS `-` — fast
                // path, matches Python already. Anything else (Unknown, or
                // a known custom-class instance like Decimal/Fraction)
                // routes through pyNeg, which dispatches `__neg__`.
                // Without this, `-Decimal('5.5')` fell through to bare
                // `-x`, which coerces via `valueOf()` to a plain float
                // and silently loses the Decimal type.
                if self.infer_type(operand).is_scalar() {
                    self.write("(-");
                    self.emit_expr(operand);
                    self.write(")");
                } else {
                    self.need_runtime("pyNeg");
                    self.write("pyNeg(");
                    self.emit_expr(operand);
                    self.write(")");
                }
            }
            UnaryOp::Pos => {
                self.write("(+");
                self.emit_expr(operand);
                self.write(")");
            }
            UnaryOp::Not => {
                // #211: `not x` must use Python truthiness. For a scalar
                // (int/float/bool/str/None) JS `!` already matches Python, so
                // keep the bare fast path. For a collection or Unknown operand
                // an empty list/dict/set is FALSY in Python but TRUTHY in JS
                // (`![]` === false), so wrap in pyBool — same conservative
                // choice as `if x:` / `while x:`. This is why `if not strings:`
                // guards silently failed on empty inputs (HumanEval /5 /12).
                if self.infer_type(operand).is_scalar() {
                    self.write("(!");
                    self.emit_expr(operand);
                    self.write(")");
                } else {
                    self.need_runtime("pyBool");
                    self.write("(!pyBool(");
                    self.emit_expr(operand);
                    self.write("))");
                }
            }
            UnaryOp::BitNot => {
                // Wave-14 F9: raw JS `~` does ToInt32, so `~(2**40)` compiled
                // to -1 instead of CPython's -1099511627777 (ints through
                // 2^53-1 emit as JS Numbers). Route through the BigInt-aware
                // pyBitNot, mirroring how binary `&`/`|`/`^` route through
                // pyBitOr/pyBitAnd/pyBitXor. The float-context flag (#343
                // discipline, unary shape) makes a statically-float operand
                // raise TypeError like CPython (`~1.5`).
                self.need_runtime("pyBitNot");
                self.write("pyBitNot(");
                self.emit_expr(operand);
                if self.is_definitely_float(operand) {
                    self.write(", 1");
                }
                self.write(")");
            }
        }
    }

    fn emit_comparison(&mut self, left: &Expr, comparisons: &[(BinOp, Expr)]) {
        if comparisons.len() == 1 {
            let (op, right) = &comparisons[0];
            self.emit_binop(left, *op, right);
        } else {
            // Round-2 pythonic sweep: chained comparison `a < b() < c`
            // must evaluate each middle operand ONCE (previously the
            // operand expression was emitted twice — `mid()` ran twice).
            // Non-trivial operands are captured as arrow parameters,
            // which also preserves Python's left-to-right evaluation and
            // short-circuit order.
            self.emit_comparison_chain(left, comparisons);
        }
    }

    fn comparison_operand_trivial(e: &Expr) -> bool {
        matches!(
            &e.kind,
            ExprKind::Name(_)
                | ExprKind::IntLiteral(_)
                | ExprKind::FloatLiteral(_)
                | ExprKind::StringLiteral(_)
                | ExprKind::BoolLiteral(_)
                | ExprKind::NoneLiteral
        )
    }

    fn emit_comparison_chain(&mut self, left: &Expr, comparisons: &[(BinOp, Expr)]) {
        // A chained comparison `a < b < c < …` lowers by recursing on the tail
        // `comparisons[1..]`, so recursion depth tracks chain length rather than
        // AST nesting — the `emit_expr` guard would not see it. Charge each link
        // against the same depth budget so an absurdly long chain reports a
        // clean overflow instead of exhausting the native stack. (On overflow
        // the half-written output is discarded by the driver, so the unbalanced
        // placeholder written here is harmless.)
        let _guard = match EmitDepthGuard::enter() {
            Some(g) => g,
            None => {
                self.record_emit_overflow(left.span.start);
                return;
            }
        };
        let (op, right) = &comparisons[0];
        if comparisons.len() == 1 {
            self.emit_binop(left, *op, right);
            return;
        }
        if Self::comparison_operand_trivial(right) {
            // Trivial middle operand — re-emitting it is side-effect-free.
            self.write("(");
            self.emit_binop(left, *op, right);
            self.write(" && ");
            self.emit_comparison_chain(right, &comparisons[1..]);
            self.write(")");
            return;
        }
        // Capture the middle operand (and, on the first link, a
        // non-trivial left) as arrow parameters: arguments evaluate
        // left-to-right, matching Python.
        let n = self.default_hoist_counter;
        self.default_hoist_counter += 1;
        let tmp_r = format!("__cmp{}", n);
        let tmp_r_expr = Expr {
            kind: ExprKind::Name(tmp_r.clone()),
            span: right.span,
        };
        if Self::comparison_operand_trivial(left) {
            self.write(&format!("(({}) => (", tmp_r));
            self.emit_binop(left, *op, &tmp_r_expr);
            self.write(" && ");
            self.emit_comparison_chain(&tmp_r_expr, &comparisons[1..]);
            self.write("))(");
            self.emit_expr(right);
            self.write(")");
        } else {
            let tmp_l = format!("__cmpl{}", n);
            let tmp_l_expr = Expr {
                kind: ExprKind::Name(tmp_l.clone()),
                span: left.span,
            };
            self.write(&format!("(({}, {}) => (", tmp_l, tmp_r));
            self.emit_binop(&tmp_l_expr, *op, &tmp_r_expr);
            self.write(" && ");
            self.emit_comparison_chain(&tmp_r_expr, &comparisons[1..]);
            self.write("))(");
            self.emit_expr(left);
            self.write(", ");
            self.emit_expr(right);
            self.write(")");
        }
    }

    fn emit_call(&mut self, func: &Expr, args: &[Expr], kwargs: &[Keyword], optional: bool) {
        let open_paren = if optional { "?.(" } else { "(" };

        // #347: a whole-valued float in a format-spec (`f'{0.0:>6}'`) must
        // format as a float ('   0.0'), not an int ('     0'). The parser
        // lowers `{value:spec}` to `pyFormatSpec(value, opts)`; the value's
        // float-ness is a static fact only the codegen knows (a whole float is
        // the same JS number as an int at runtime). Thread it as a 3rd arg so
        // pyFormatSpec's no-type-char branch renders via the float formatter.
        if let ExprKind::Name(n) = &func.kind {
            if n == "pyFormatSpec"
                && args.len() == 2
                && kwargs.is_empty()
                && !self.is_declared(n)
                && self.is_definitely_float(&args[0])
            {
                self.need_runtime("pyFormatSpec");
                self.write("pyFormatSpec");
                self.write(open_paren);
                self.emit_expr(&args[0]);
                self.write(", ");
                self.emit_expr(&args[1]);
                self.write(", true)");
                return;
            }
        }

        // #260: `set.intersection(a, b)` is the unbound-method form and means
        // `a.intersection(b)` (first arg is `self`). Only when `set`/`frozenset`
        // is the builtin (not a shadowing local variable).
        if let ExprKind::Attribute {
            value,
            attr,
            optional: false,
        } = &func.kind
        {
            if !args.is_empty() {
                if let ExprKind::Name(n) = &value.kind {
                    if (n == "set" || n == "frozenset") && !self.is_declared(n) {
                        let bound = Expr {
                            kind: ExprKind::Attribute {
                                value: Box::new(args[0].clone()),
                                attr: attr.clone(),
                                optional: false,
                            },
                            span: func.span,
                        };
                        self.emit_call(&bound, &args[1..], kwargs, optional);
                        return;
                    }
                }
            }
        }

        // Bare `super()` inside a method → cooperative-MRO super proxy.
        // `super().greet()` becomes `__pySuper(B, this).greet()` where `B`
        // is the *defining* class; the helper finds `B` in the instance's
        // MRO and dispatches to the next class after it (so diamonds chain
        // L→R→Base rather than L→Base). Constructor `super().__init__(...)`
        // never reaches here — it's hoisted to a native `super(...)` call
        // in `emit_class_method`, preserving the single-inheritance path.
        if let ExprKind::Name(n) = &func.kind {
            if n == "super" && args.is_empty() && kwargs.is_empty() {
                if let Some(cls) = self.class_stack.last().map(|c| c.name.clone()) {
                    self.need_runtime("__pySuper");
                    self.write(&format!("__pySuper({}, this)", Self::sanitize_ident(&cls)));
                    return;
                }
            }
        }

        // PSX: Inside @component functions, element calls emit JSX
        if self.in_component {
            if let ExprKind::Name(name) = &func.kind {
                // Disambiguate dataclass instantiation from React-component
                // creation. Inside a @component, an uppercase Name like
                // `Metric(...)` is ambiguous: it could be a class (instance
                // construction → `new Metric(...)`) or a React component
                // (element creation → `createElement(Metric, ...)`). The
                // pre-scan in emit_module records every `class` name; if
                // the call site matches, route to `new` and bail out so
                // the PSX path doesn't claim it.
                if self.known_classes.contains(name) {
                    if !kwargs.is_empty() && !optional {
                        self.emit_ctor_kw_call(&Self::sanitize_ident(name), args, kwargs);
                        return;
                    }
                    self.write(&format!("new {}{}", Self::sanitize_ident(name), open_paren));
                    self.emit_call_args(args, kwargs);
                    self.write(")");
                    return;
                }
                // Known JS/DOM global constructors (EventSource, URL, FormData,
                // Date, Map, ...) are NOT React elements: inside a @component
                // `EventSource(url)` must emit `new EventSource(url)`, not
                // `createElement(EventSource, ...)`. These globals never appear
                // in `known_classes` (they aren't `class`-defined in the file),
                // so without this branch they fall to the capitalized-name PSX
                // fallback below and mis-emit as element creation. (B-037 /
                // pythscribe#53.) A user-defined class shadowing one of these
                // names is already caught by the `known_classes` branch above.
                if react::is_builtin_constructor(name) {
                    self.write(&format!("new {}{}", Self::sanitize_ident(name), open_paren));
                    self.emit_call_args(args, kwargs);
                    self.write(")");
                    return;
                }
                // Names imported from React-like modules win over the
                // HTML-element fallback. Otherwise, calling a hook
                // whose name collides with an HTML/SVG tag (e.g.,
                // React 19's `use()` collides with `<use>` SVG element)
                // would mis-emit as `createElement("use", ...)`.
                if self.react_imports.contains(name) {
                    // Capitalized React-core re-exports (Fragment, Suspense,
                    // StrictMode, Profiler, etc.) are not constructors —
                    // they're JSX element types. Without this branch they'd
                    // fall out of the @component block and hit the
                    // class-instantiation fallback below, emitting
                    // `new Fragment(...)` which throws "Fragment is not a
                    // constructor" at runtime. Route them through the PSX
                    // path so they wrap as `createElement(Fragment, ...)`
                    // — the same shape user-defined React components get.
                    // Lowercase hook imports (use_state, use_effect) keep
                    // the fall-through so they stay plain function calls.
                    if name.chars().next().is_some_and(|c| c.is_uppercase()) {
                        self.emit_psx_element(name, args, kwargs);
                        return;
                    }
                    // Fall through to the regular call emission below.
                } else if self.is_psx_tag_call(name) {
                    // DESIGN RULE — builtin ∩ HTML/SVG-element name collision:
                    // inside a @component, a name that is a KNOWN HTML/SVG
                    // element is lowered as that ELEMENT even when it is ALSO a
                    // Python builtin. **HTML wins the collision.** The only
                    // names in both sets are `map`/`input`/`object`
                    // (→ createElement("map"/"input"/"object", …)); the common
                    // data-builtins `filter`/`set`/`list`/`dict`/`zip`/`sorted`
                    // are NOT element names, so they keep their builtin lowering
                    // (no collision). Rationale: `<input>`/`<map>`/`<object>`
                    // are the overwhelmingly common in-component use, while the
                    // colliding builtins have alternatives (`map` → a list
                    // comprehension `[f(x) for x in xs]`; `input()`/`object()`
                    // are meaningless in a browser component). This branch runs
                    // AFTER `known_classes` (stdlib CapWords classes → `new X()`)
                    // and JS builtin-constructors, so those still win; and it
                    // only fires inside `in_component` (PSX context), so OUTSIDE
                    // a component the builtin lowering applies normally. Locked
                    // by `test_builtin_html_collision_prefers_element_in_component`.
                    self.emit_psx_element(name, args, kwargs);
                    return;
                }
            }
            // Member-expression components: `Ctx.Provider(value=v, children)`,
            // `Menu.Item(...)` — a CAPITALIZED attribute on a simple dotted
            // Name chain is a React component (the Context.Provider / dotted
            // sub-component convention), not a method call. Without this arm
            // the call fell through to plain-call emission with reordered
            // args → runtime TypeError (found by the Netflix clone). Lowercase
            // attributes (e.preventDefault(), obj.method()) are untouched.
            if let ExprKind::Attribute {
                value,
                attr,
                optional: false,
            } = &func.kind
            {
                // Flatten only simple Name(.attr)* chains; anything more
                // complex keeps plain-call semantics.
                fn flatten_chain(e: &Expr) -> Option<String> {
                    match &e.kind {
                        ExprKind::Name(n) => Some(n.clone()),
                        ExprKind::Attribute {
                            value,
                            attr,
                            optional: false,
                        } => flatten_chain(value).map(|b| format!("{}.{}", b, attr)),
                        _ => None,
                    }
                }
                let attr_is_component = attr.chars().next().is_some_and(|c| c.is_uppercase());
                if let Some(base) = flatten_chain(value) {
                    // Track-B: framer-motion's `motion.div` / `motion.span` —
                    // LOWERCASE members of a tracked base are components too.
                    // Without this the call fell through to plain-call
                    // emission (`motion.div(...)` invoked as a function —
                    // TypeError: motion.div is a component object).
                    let root = base.split('.').next().unwrap_or(&base);
                    if attr_is_component || self.react_member_component_bases.contains(root) {
                        let tag = format!("{}.{}", base, attr);
                        self.emit_psx_element(&tag, args, kwargs);
                        return;
                    }
                }
            }
            // Curried PSX form: `tag(props)(children)` — outer Call has the
            // PSX-element Call as its func and children as args. Flatten into
            // `createElement(tag, props, ...children)` so the result is a real
            // React element rather than `createElement(...)(...)` which isn't.
            if let ExprKind::Call {
                func: inner_func,
                args: inner_args,
                kwargs: inner_kwargs,
                ..
            } = &func.kind
            {
                if let ExprKind::Name(name) = &inner_func.kind {
                    if self.is_psx_tag_call(name) && inner_args.is_empty() {
                        // Combine: inner kwargs are the props, outer args are
                        // the children. Pass-through the merged form.
                        self.emit_psx_element_with_children(name, inner_kwargs, args);
                        return;
                    }
                }
            }
        }

        // Issue #22: native .length for statically-known list/tuple receivers.
        // When `len(x)` is called and `x` is provably a list or tuple (JS
        // Array), emit `x.length` directly — no helper import, no call overhead.
        // Dict/Set/Unknown keep pyLen: dict needs Object.keys(); set needs .size;
        // unknown could be either. Primitive (str/int/bool) also keeps pyLen
        // because JsInferredType::Primitive conflates str (has .length) with
        // int/bool/None (no .length) — we can't safely narrow here.
        if let ExprKind::Name(func_name) = &func.kind {
            if func_name == "len" && args.len() == 1 && kwargs.is_empty() {
                let arg_ty = self.infer_type(&args[0]);
                if matches!(arg_ty, JsInferredType::List | JsInferredType::Tuple) {
                    self.emit_expr(&args[0]);
                    self.write(".length");
                    return;
                }
                // Fall through to pyLen for Dict/Set/Primitive/Unknown.
            }
        }

        // A4: `print`/`str`/`repr`/f-string interpolation can't always tell
        // a Python int from a whole-number Python float at *runtime* — small
        // ints (abs <= 2**53-1) and BigInt-arithmetic results that fit in
        // the safe-integer range are demoted to plain JS `number` (see
        // ExprKind::IntLiteral / __norm() above), which is byte-identical
        // to how a whole float like `1.0` compiles (Rust's `{}` formatter
        // for f64 drops the trailing `.0`, and JS has one untagged numeric
        // primitive for both). `pyRepr`'s runtime number branch therefore
        // cannot always add `.0` correctly from the value alone.
        //
        // Where the argument is DEFINITELY a `float` — a literal `1.0`, or
        // unary +/- directly wrapping one — resolve the ambiguity at
        // compile time instead: pre-format with `pyFormatFloat` and pass
        // the resulting string straight through, bypassing the runtime
        // number branch's ambiguity entirely. `is_definitely_float` is
        // deliberately narrow (see its doc comment) — it does NOT reuse
        // `infer_type`'s general Name/BinOp propagation, because
        // `infer_type`'s `BinOp::Div => Float` rule is unconditional and
        // doesn't check operand types, which is wrong for classes
        // overriding `__truediv__` (Decimal, Fraction): treating
        // `Decimal(1) / Decimal(3)` as "definitely float" here would
        // bypass pyRepr's `__repr__` dispatch and corrupt Decimal/Fraction
        // division output (this was caught during A4 development via the
        // differential corpus — dec_div_third et al. — and is exactly why
        // this uses a dedicated whitelist instead of infer_type). This
        // narrows the compile-time fix to exactly the literal-argument
        // case from the bug report; a float value that only becomes known
        // at runtime through an untracked channel (e.g. a variable, a
        // list/dict element, or an unannotated function return) is a
        // documented, accepted residual gap — not a regression versus
        // current behavior.
        if let ExprKind::Name(name) = &func.kind {
            if kwargs.is_empty()
                && matches!(name.as_str(), "str" | "repr")
                && args.len() == 1
                && self.is_definitely_float(&args[0])
            {
                self.need_runtime("pyFormatFloat");
                self.write("pyFormatFloat(");
                self.emit_expr(&args[0]);
                self.write(")");
                return;
            }
            if name == "print"
                && kwargs.is_empty()
                && args.iter().any(|a| self.is_definitely_float(a))
            {
                self.need_runtime("pyPrint");
                self.need_runtime("pyFormatFloat");
                self.write("pyPrint(");
                for (i, a) in args.iter().enumerate() {
                    if i > 0 {
                        self.write(", ");
                    }
                    if self.is_definitely_float(a) {
                        self.write("pyFormatFloat(");
                        self.emit_expr(a);
                        self.write(")");
                    } else {
                        self.emit_expr(a);
                    }
                }
                self.write(")");
                return;
            }
        }

        // #225: `eval`/`exec`/`compile` are intentionally unsupported (running
        // arbitrary Python at runtime has no place in an AOT-compiled, edge-
        // deployable target). Emit a clear codegen diagnostic instead of the
        // cryptic `eval$ is not defined` the reserved-word rename produced.
        if let ExprKind::Name(name) = &func.kind {
            if matches!(name.as_str(), "eval" | "exec" | "compile") {
                let diag = format!(
                    "`{}()` is not supported: PythScribe is an ahead-of-time \
                     compiler and does not run arbitrary Python at runtime.",
                    name
                );
                eprintln!("error: {}", diag);
                self.codegen_errors.push(diag.clone());
                self.write(&format!(
                    "(() => {{ throw new Error({:?}); }})()",
                    format!("PythScribe: {}", diag)
                ));
                return;
            }
        }

        // Check for builtin function mapping
        if let ExprKind::Name(name) = &func.kind {
            // A local/param/enclosing binding SHADOWS a builtin of the same name
            // (Python scoping): a param or local named `set`/`list`/`dict`/… must
            // call THAT binding, not the builtin. Only apply the builtin lowering
            // when the name is NOT bound in any enclosing scope. Fixes Zustand's
            // idiomatic `create(lambda set: … set(…))`, where the `set` parameter
            // was mis-lowered to the `set()` builtin (`pySetOf`) so store updates
            // silently no-op'd. (Found by the dual-track client-state tests.)
            if let Some(mapping) =
                builtin_func_mapping(name).filter(|_| !self.is_declared_in_any_scope(name))
            {
                // #217: `list()` with no args maps to `Array.from()`, which
                // throws (`undefined is not iterable`). The empty constructor
                // is just `[]`. (HumanEval /67: `lis = list()`.)
                if name == "list" && args.is_empty() && kwargs.is_empty() {
                    self.write("[]");
                    return;
                }
                // #244: `dict()` assigned to a name later written with a
                // non-string key — build a Map-backed PyDict, not a plain
                // object (whose int keys stringify).
                if name == "dict"
                    && args.is_empty()
                    && kwargs.is_empty()
                    && std::mem::take(&mut self.force_pydict_literal)
                {
                    self.need_runtime("PyDict");
                    self.write("new PyDict([])");
                    return;
                }
                match mapping {
                    BuiltinMapping::Direct(js_name) => {
                        self.write(js_name);
                        self.write(open_paren);
                        self.emit_call_args(args, kwargs);
                        self.write(")");
                        return;
                    }
                    BuiltinMapping::Runtime(helper) => {
                        self.need_runtime(helper);
                        // isinstance(x, list) — builtin TYPE names have no JS
                        // value (`list` is unbound → ReferenceError; found by
                        // the Kanban clone). Lower builtin type names in the
                        // class position to STRING sentinels the runtime
                        // helper dispatches on ("list"/"dict"/...); user
                        // classes still pass as values. Tuple form maps each
                        // element.
                        if name == "isinstance" && args.len() == 2 {
                            let is_ty = |e: &Expr| {
                                matches!(&e.kind,
                                ExprKind::Name(n) if matches!(n.as_str(),
                                    "list" | "tuple" | "str" | "int" | "float"
                                    | "bool" | "dict" | "set"))
                            };
                            let emit_cls = |s: &mut Self, e: &Expr| {
                                if let ExprKind::Name(n) = &e.kind {
                                    if is_ty(e) {
                                        s.write(&format!("\"{}\"", n));
                                        return;
                                    }
                                }
                                s.emit_expr(e);
                            };
                            self.write(helper);
                            self.write(open_paren);
                            self.emit_expr(&args[0]);
                            self.write(", ");
                            if let ExprKind::Tuple(elts) = &args[1].kind {
                                self.write("[");
                                for (i, e) in elts.iter().enumerate() {
                                    if i > 0 {
                                        self.write(", ");
                                    }
                                    emit_cls(self, e);
                                }
                                self.write("]");
                            } else {
                                emit_cls(self, &args[1]);
                            }
                            self.write(")");
                            return;
                        }
                        self.write(helper);
                        self.write(open_paren);
                        self.emit_call_args(args, kwargs);
                        self.write(")");
                        return;
                    }
                }
            }

            // Check for React hook mapping (use_state → useState, etc.)
            if let Some(js_name) = react::react_hook_mapping(name) {
                self.write(js_name);
                self.write(open_paren);
                self.emit_call_args(args, kwargs);
                self.write(")");
                return;
            }
        }

        // Check for Class instantiation (calling Name with uppercase first letter)
        // #80: a top-level `def` with a capitalized name is a FUNCTION —
        // `new`-calling it returned `{}` instead of its return value. A
        // known class always wins (shadowing edge); otherwise a known
        // function suppresses the capitalization heuristic.
        if let ExprKind::Name(name) = &func.kind {
            // #253: a KNOWN class is always `new`-called, regardless of case
            // (datetime's classes are lowercase); otherwise the capitalization
            // heuristic applies to an unknown, non-function name.
            if self.known_classes.contains(name)
                || (name.chars().next().is_some_and(|c| c.is_uppercase())
                    && !self.known_functions.contains(name))
            {
                if !kwargs.is_empty() && !optional {
                    self.emit_ctor_kw_call(&Self::sanitize_ident(name), args, kwargs);
                    return;
                }
                self.write(&format!("new {}{}", Self::sanitize_ident(name), open_paren));
                self.emit_call_args(args, kwargs);
                self.write(")");
                return;
            }
        }

        // Round-3 pythonic sweep: `cls(...)` inside a @classmethod
        // constructs the class `cls` is bound to (subclass-aware).
        if self.in_classmethod && !optional {
            if let ExprKind::Name(n) = &func.kind {
                if n == "cls" {
                    if !kwargs.is_empty() {
                        self.emit_ctor_kw_call("cls", args, kwargs);
                        return;
                    }
                    self.write("new cls(");
                    self.emit_call_args(args, kwargs);
                    self.write(")");
                    return;
                }
            }
        }

        // Round-3 pythonic sweep: calling a method THROUGH a known class
        // (`Animal.speak(self)`, unbound-method style) crashed — instance
        // methods live on the prototype, not the class object. Route
        // through __pyClassCall, which dispatches statics/classmethods on
        // the class and plain methods as unbound (first argument = self).
        if !optional {
            if let ExprKind::Attribute {
                value: recv,
                attr,
                optional: false,
            } = &func.kind
            {
                if let ExprKind::Name(cls_name) = &recv.kind {
                    if self.known_classes.contains(cls_name) {
                        self.need_runtime("__pyClassCall");
                        self.write(&format!(
                            "__pyClassCall({}, \"{}\", [",
                            Self::sanitize_ident(cls_name),
                            attr
                        ));
                        self.emit_call_args(args, kwargs);
                        self.write("])");
                        return;
                    }
                }
            }
        }

        // Check for Python-method lowering on attribute access
        // (e.g. `xs.append(v)` → `xs.push(v)`, `s.lower()` → `s.toLowerCase()`).
        // The table is consulted by attribute-name only — receiver type is
        // unknown to JS codegen — and a fallthrough to verbatim emission
        // remains for any name not in the table.
        // #224: a Capitalized member of a stdlib module namespace is a class
        // constructor (`collections.Counter(xs)`, `collections.OrderedDict()`),
        // so emit `new` — the bare-name capitalization heuristic below only
        // fires for un-qualified names, and a module-qualified call otherwise
        // fell through to `collections.Counter(xs)` (invoked without `new`).
        if let ExprKind::Attribute {
            value,
            attr,
            optional: false,
        } = &func.kind
        {
            if matches!(&value.kind, ExprKind::Name(n) if self.module_namespaces.contains(n))
                && attr.chars().next().is_some_and(|c| c.is_uppercase())
            {
                self.write("new ");
                self.emit_expr(value);
                self.write(&format!(".{}(", attr));
                self.emit_call_args(args, kwargs);
                self.write(")");
                return;
            }
        }

        if let ExprKind::Attribute { value, attr, .. } = &func.kind {
            // #221: a call on a stdlib module namespace (`re.split`, `os.count`)
            // is a module function, not the string/list method with the same
            // name — skip the lowering table and emit it verbatim.
            let is_module_call =
                matches!(&value.kind, ExprKind::Name(n) if self.module_namespaces.contains(n));
            if !is_module_call {
                if let Some(lowering) = method_lowering(attr) {
                    if self.try_emit_method_lowering(value, attr, args, kwargs, lowering, optional)
                    {
                        return;
                    }
                }
            }
        }

        // Round-2 pythonic sweep: keyword arguments on a plain user
        // function bind BY NAME to positional parameters in Python; the
        // legacy lowering passed a trailing options object, which landed
        // an object in the first keyword parameter's slot (garbage).
        // Name-callee calls with kwargs route through __pyCallKw, which
        // consults the callee's __pyparams__ metadata (attached at
        // definition) and falls back to the options-object convention
        // for functions without it (JS interop, components, methods).
        // Attribute callees are excluded — extracting `obj.m` would lose
        // its `this` binding.
        if !kwargs.is_empty() && !optional && matches!(&func.kind, ExprKind::Name(_)) {
            self.need_runtime("__pyCallKw");
            self.write("__pyCallKw(");
            self.emit_expr(func);
            self.write(", [");
            for (i, a) in args.iter().enumerate() {
                if i > 0 {
                    self.write(", ");
                }
                self.emit_expr(a);
            }
            self.write("], ");
            self.emit_kwargs_value(kwargs);
            self.write(")");
            return;
        }

        // Round-3 pythonic sweep: METHOD calls with keyword arguments —
        // `obj.m(k=1)` binds by name via the method's __pyparams__
        // metadata (attached at class emission). Spreading at the call
        // site (`recv.m(...__pyKwArgs(recv.m, ...))`) keeps `this`;
        // metadata-less methods (JS interop, builtins that fell through
        // the lowering table) get the legacy trailing options object.
        // Non-simple receivers evaluate once via an arrow parameter.
        if !kwargs.is_empty() && !optional {
            if let ExprKind::Attribute {
                value: recv,
                attr,
                optional: false,
            } = &func.kind
            {
                self.need_runtime("__pyKwArgs");
                let simple = is_simple_receiver(recv);
                let recv_js = if simple {
                    None
                } else {
                    let n = self.default_hoist_counter;
                    self.default_hoist_counter += 1;
                    Some(format!("__recv{}", n))
                };
                if let Some(r) = &recv_js {
                    self.write(&format!(
                        "(({r}) => {r}.{attr}(...__pyKwArgs({r}.{attr}, [",
                        r = r,
                        attr = attr
                    ));
                } else {
                    self.emit_expr(recv);
                    self.write(&format!(".{}(...__pyKwArgs(", attr));
                    self.emit_expr(recv);
                    self.write(&format!(".{}, [", attr));
                }
                for (i, a) in args.iter().enumerate() {
                    if i > 0 {
                        self.write(", ");
                    }
                    self.emit_expr(a);
                }
                self.write("], ");
                self.emit_kwargs_value(kwargs);
                self.write("))");
                if recv_js.is_some() {
                    self.write(")(");
                    self.emit_expr(recv);
                    self.write(")");
                }
                return;
            }
        }

        // Regular function call. Low-precedence callees (a lambda IIFE
        // `(lambda a: ...)(x)`, a conditional) MUST be parenthesized — an
        // unwrapped arrow in callee position binds the call to its BODY:
        // `(a) => pyMul(a, 2)(21)` instead of `((a) => pyMul(a, 2))(21)`
        // (silently wrong; found by the Spotify clone).
        let needs_parens = matches!(
            &func.kind,
            ExprKind::Lambda { .. } | ExprKind::IfExpr { .. } | ExprKind::Await(_)
        );
        if needs_parens {
            self.write("(");
        }
        self.emit_expr(func);
        if needs_parens {
            self.write(")");
        }
        self.write(open_paren);
        self.emit_call_args(args, kwargs);
        self.write(")");
    }

    /// Attempt to emit a Python→JS method lowering. Returns `true` on success.
    /// Falls back (returns `false`) when the lowering can't safely apply
    /// (e.g., complex receiver on an Inline-only spec that needs a simple
    /// receiver), letting the caller emit the verbatim form.
    fn try_emit_method_lowering(
        &mut self,
        receiver: &Expr,
        attr: &str,
        args: &[Expr],
        kwargs: &[Keyword],
        lowering: MethodLowering,
        optional: bool,
    ) -> bool {
        match lowering {
            MethodLowering::Rename(js_name) => {
                self.emit_expr(receiver);
                if optional {
                    self.write(&format!("?.{}(", js_name));
                } else {
                    self.write(&format!(".{}(", js_name));
                }
                self.emit_call_args(args, kwargs);
                self.write(")");
                true
            }
            MethodLowering::Inline(spec) => {
                // Specs that reference the receiver more than once require
                // a simple Name receiver. If complex, fall through.
                if spec.needs_simple_receiver() && !is_simple_receiver(receiver) {
                    return false;
                }
                self.emit_inline_spec(receiver, args, spec)
            }
            MethodLowering::Hybrid { inline, runtime } => {
                if is_simple_receiver(receiver)
                    && self.hybrid_inline_applies(inline, receiver)
                    && self.emit_inline_spec(receiver, args, inline)
                {
                    return true;
                }
                // Complex receiver, type-inapplicable inline, OR inline form
                // rejected the args (e.g., wrong arity) — delegate to the
                // runtime helper.
                self.emit_runtime_method(runtime, receiver, args, kwargs)
            }
            MethodLowering::Runtime { helper, .. } => {
                self.emit_runtime_method(helper, receiver, args, kwargs)
            }
            MethodLowering::Unsupported(reason) => {
                // Record a codegen-time diagnostic *and* emit a JS expression
                // that throws if reached. This keeps the JS module parseable
                // (so other tests/CI still surface the error site cleanly)
                // while making the build fail loudly. The CLI surfaces the
                // accumulated `codegen_errors` after `emit_module` returns.
                let _ = receiver;
                let diag = format!("Python method `.{}()` not yet supported. {}", attr, reason);
                eprintln!("error: {}", diag);
                self.codegen_errors.push(diag.clone());
                self.write(&format!(
                    "(() => {{ throw new Error({:?}); }})()",
                    format!("PythScribe: {}", diag)
                ));
                true
            }
        }
    }

    /// Found while shape-testing #83: a Hybrid lowering's inline form can
    /// be type-specific — `clear`/`copy` share one Multi entry across
    /// list/dict/set, but ClearList/CopyList emit `.length = 0` /
    /// `.slice()`, which are only correct for arrays (on a dict they
    /// produced `{'length': 0}` / crashed on `.slice`). Gate those inlines
    /// on a receiver PROVABLY typed List/Tuple; dict/set/unknown receivers
    /// take the shape-dispatching runtime helper instead.
    fn hybrid_inline_applies(&mut self, spec: InlineSpec, receiver: &Expr) -> bool {
        match spec {
            InlineSpec::ClearList | InlineSpec::CopyList => matches!(
                self.infer_type(receiver),
                JsInferredType::List | JsInferredType::Tuple
            ),
            // #301: list mutators only inline (`.push` / `.push(...)` /
            // `.splice`) when the receiver is PROVABLY a list — an unknown
            // receiver could be a DOM node / JS object whose same-named
            // native method (`ParentNode.append`, a library's `insert`)
            // must win. Unknowns route through pyAppend/pyExtend/pyInsert,
            // which dispatch on the receiver shape at runtime.
            InlineSpec::AppendList | InlineSpec::ExtendList | InlineSpec::InsertList => {
                matches!(self.infer_type(receiver), JsInferredType::List)
            }
            _ => true,
        }
    }

    /// Emit a runtime-helper method call: `pyHelper(receiver, ...args)`.
    /// Records the helper as needed so it appears in the runtime imports.
    fn emit_runtime_method(
        &mut self,
        helper: &str,
        receiver: &Expr,
        args: &[Expr],
        kwargs: &[Keyword],
    ) -> bool {
        self.need_runtime(helper);
        self.write(helper);
        self.write("(");
        self.emit_expr(receiver);
        if !args.is_empty() || !kwargs.is_empty() {
            self.write(", ");
        }
        // Pythonic-checks sweep: kwargs used to be silently DROPPED on the
        // Runtime lowering path (`d.popitem(last=False)` compiled to
        // `pyDictPopitem(d)`). Forward them with the standard trailing
        // options-object convention.
        self.emit_call_args(args, kwargs);
        self.write(")");
        true
    }

    /// Emit an inline JS form for a method call. Returns `false` if the
    /// arity doesn't match the spec, so the caller can fall back.
    fn emit_inline_spec(&mut self, receiver: &Expr, args: &[Expr], spec: InlineSpec) -> bool {
        match spec {
            // -------------------- list inline --------------------
            InlineSpec::AppendList => {
                if args.len() != 1 {
                    return false;
                }
                self.emit_expr(receiver);
                self.write(".push(");
                self.emit_expr(&args[0]);
                self.write(")");
            }
            InlineSpec::ExtendList => {
                if args.len() != 1 {
                    return false;
                }
                self.emit_expr(receiver);
                self.write(".push(...");
                self.emit_expr(&args[0]);
                self.write(")");
            }
            InlineSpec::InsertList => {
                if args.len() != 2 {
                    return false;
                }
                self.emit_expr(receiver);
                self.write(".splice(");
                self.emit_expr(&args[0]);
                self.write(", 0, ");
                self.emit_expr(&args[1]);
                self.write(")");
            }
            InlineSpec::CopyList => {
                if !args.is_empty() {
                    return false;
                }
                self.emit_expr(receiver);
                self.write(".slice()");
            }
            InlineSpec::ClearList => {
                // Simple receiver only (guarded by needs_simple_receiver).
                if !args.is_empty() {
                    return false;
                }
                self.write("(");
                self.emit_expr(receiver);
                self.write(".length = 0)");
            }
            InlineSpec::CountList => {
                if args.len() != 1 {
                    return false;
                }
                self.emit_expr(receiver);
                self.write(".filter(__x => __x === ");
                self.emit_expr(&args[0]);
                self.write(").length");
            }
            InlineSpec::PopList => {
                if !args.is_empty() {
                    return false;
                }
                self.emit_expr(receiver);
                self.write(".pop()");
            }
            // -------------------- string inline --------------------
            InlineSpec::Strip => {
                if !args.is_empty() {
                    return false;
                }
                self.emit_expr(receiver);
                self.write(".replace(/^\\s+|\\s+$/g, \"\")");
            }
            InlineSpec::Lstrip => {
                if !args.is_empty() {
                    return false;
                }
                self.emit_expr(receiver);
                self.write(".replace(/^\\s+/, \"\")");
            }
            InlineSpec::Rstrip => {
                if !args.is_empty() {
                    return false;
                }
                self.emit_expr(receiver);
                self.write(".replace(/\\s+$/, \"\")");
            }
            InlineSpec::Zfill => {
                if args.len() != 1 {
                    return false;
                }
                self.emit_expr(receiver);
                self.write(".padStart(");
                self.emit_expr(&args[0]);
                self.write(", \"0\")");
            }
            InlineSpec::Capitalize => {
                if !args.is_empty() {
                    return false;
                }
                // Receiver appears 3x — caller must have ensured simple.
                self.write("(");
                self.emit_expr(receiver);
                self.write(" ? ");
                self.emit_expr(receiver);
                self.write("[0].toUpperCase() + ");
                self.emit_expr(receiver);
                self.write(".slice(1).toLowerCase() : ");
                self.emit_expr(receiver);
                self.write(")");
            }
            InlineSpec::Isdigit => {
                if !args.is_empty() {
                    return false;
                }
                self.write("/^[0-9]+$/.test(");
                self.emit_expr(receiver);
                self.write(")");
            }
            InlineSpec::IsInteger => {
                if !args.is_empty() {
                    return false;
                }
                self.write("Number.isInteger(Number(");
                self.emit_expr(receiver);
                self.write("))");
            }
            InlineSpec::Isalpha => {
                if !args.is_empty() {
                    return false;
                }
                self.write("/^[A-Za-z]+$/.test(");
                self.emit_expr(receiver);
                self.write(")");
            }
            InlineSpec::Isalnum => {
                if !args.is_empty() {
                    return false;
                }
                self.write("/^[A-Za-z0-9]+$/.test(");
                self.emit_expr(receiver);
                self.write(")");
            }
            InlineSpec::Isspace => {
                if !args.is_empty() {
                    return false;
                }
                self.write("/^\\s+$/.test(");
                self.emit_expr(receiver);
                self.write(")");
            }
            InlineSpec::Casefold => {
                // ASCII-equivalent to `.lower()`. Python's casefold does
                // additional Unicode case mapping (e.g., ß → ss); we
                // approximate with toLowerCase. Sufficient for ASCII;
                // documented as a limitation in summary.md.
                if !args.is_empty() {
                    return false;
                }
                self.emit_expr(receiver);
                self.write(".toLowerCase()");
            }
            InlineSpec::Isascii => {
                if !args.is_empty() {
                    return false;
                }
                self.write("[...");
                self.emit_expr(receiver);
                self.write("].every(__c => __c.charCodeAt(0) < 128)");
            }
            InlineSpec::Removeprefix => {
                if args.len() != 1 {
                    return false;
                }
                self.write("(");
                self.emit_expr(receiver);
                self.write(".startsWith(");
                self.emit_expr(&args[0]);
                self.write(") ? ");
                self.emit_expr(receiver);
                self.write(".slice(");
                self.emit_expr(&args[0]);
                self.write(".length) : ");
                self.emit_expr(receiver);
                self.write(")");
            }
            InlineSpec::Removesuffix => {
                if args.len() != 1 {
                    return false;
                }
                self.write("(");
                self.emit_expr(receiver);
                self.write(".endsWith(");
                self.emit_expr(&args[0]);
                self.write(") ? ");
                self.emit_expr(receiver);
                self.write(".slice(0, ");
                self.emit_expr(receiver);
                self.write(".length - ");
                self.emit_expr(&args[0]);
                self.write(".length) : ");
                self.emit_expr(receiver);
                self.write(")");
            }
            InlineSpec::Islower => {
                if !args.is_empty() {
                    return false;
                }
                self.write("(");
                self.emit_expr(receiver);
                self.write(" === ");
                self.emit_expr(receiver);
                self.write(".toLowerCase() && ");
                self.emit_expr(receiver);
                self.write(" !== ");
                self.emit_expr(receiver);
                self.write(".toUpperCase())");
            }
            InlineSpec::Isupper => {
                if !args.is_empty() {
                    return false;
                }
                self.write("(");
                self.emit_expr(receiver);
                self.write(" === ");
                self.emit_expr(receiver);
                self.write(".toUpperCase() && ");
                self.emit_expr(receiver);
                self.write(" !== ");
                self.emit_expr(receiver);
                self.write(".toLowerCase())");
            }
            // -------------------- dict inline --------------------
            // DictKeys/DictValues/DictItems were Object.keys/values/entries
            // inlines; removed in #83 — dict keys/values/items now dispatch
            // on receiver shape at runtime (pyDictKeys/pyDictValues/
            // pyDictItems in method_table.rs).
            InlineSpec::DictUpdate => {
                if args.len() != 1 {
                    return false;
                }
                self.write("Object.assign(");
                self.emit_expr(receiver);
                self.write(", ");
                self.emit_expr(&args[0]);
                self.write(")");
            }
        }
        true
    }

    /// Round-3: constructor call with Python keyword binding —
    /// `new Cls(...__pyKwArgs(Cls, [pos], kwobj))`. `Cls.__pyparams__`
    /// (set at class emission from __init__ params / dataclass fields)
    /// maps names to positional slots; metadata-less classes get the
    /// legacy trailing options object inside the array.
    fn emit_ctor_kw_call(&mut self, ctor_js: &str, args: &[Expr], kwargs: &[Keyword]) {
        self.need_runtime("__pyKwArgs");
        self.write(&format!("new {c}(...__pyKwArgs({c}, [", c = ctor_js));
        for (i, a) in args.iter().enumerate() {
            if i > 0 {
                self.write(", ");
            }
            self.emit_expr(a);
        }
        self.write("], ");
        self.emit_kwargs_value(kwargs);
        self.write("))");
    }

    /// The keyword-arguments value passed to __pyCallKw: a plain object
    /// literal when only named kwargs are present; a Map-aware
    /// pyDictMerge of in-order parts when any `**spread` participates
    /// (object-literal spread of a Map-backed dict would drop entries).
    fn emit_kwargs_value(&mut self, kwargs: &[Keyword]) {
        let has_spread = kwargs.iter().any(|k| k.name.is_none());
        if !has_spread {
            self.write("{");
            for (i, kw) in kwargs.iter().enumerate() {
                if i > 0 {
                    self.write(", ");
                }
                if let Some(name) = &kw.name {
                    self.write(&format!("{}: ", name));
                }
                self.emit_expr(&kw.value);
            }
            self.write("}");
            return;
        }
        self.need_runtime("pyDictMerge");
        self.write("pyDictMerge(");
        let mut parts_written = 0;
        let mut open_obj = false;
        for kw in kwargs {
            match &kw.name {
                Some(name) => {
                    if !open_obj {
                        if parts_written > 0 {
                            self.write(", ");
                        }
                        self.write("{");
                        open_obj = true;
                        parts_written += 1;
                    } else {
                        self.write(", ");
                    }
                    self.write(&format!("{}: ", name));
                    self.emit_expr(&kw.value);
                }
                None => {
                    if open_obj {
                        self.write("}");
                        open_obj = false;
                    }
                    if parts_written > 0 {
                        self.write(", ");
                    }
                    self.emit_expr(&kw.value);
                    parts_written += 1;
                }
            }
        }
        if open_obj {
            self.write("}");
        }
        self.write(")");
    }

    fn emit_call_args(&mut self, args: &[Expr], kwargs: &[Keyword]) {
        let mut first = true;
        for arg in args {
            if !first {
                self.write(", ");
            }
            first = false;
            self.emit_expr(arg);
        }
        // Emit kwargs as an options object if present
        if !kwargs.is_empty() {
            if !first {
                self.write(", ");
            }
            self.write("{");
            for (i, kw) in kwargs.iter().enumerate() {
                if i > 0 {
                    self.write(", ");
                }
                if let Some(name) = &kw.name {
                    self.write(&format!("{}: ", name));
                } else {
                    // `**spread` — previously emitted the bare value
                    // (object-shorthand `{kw}`); spread its entries.
                    self.write("...");
                }
                self.emit_expr(&kw.value);
            }
            self.write("}");
        }
    }

    /// Emit a PSX element as a createElement() call (no JSX angle brackets).
    /// `div(class_name="app", h1("Hello"))` → `createElement("div", {className: "app"}, createElement("h1", null, "Hello"))`
    /// Variant of `emit_psx_element` for the curried form `tag(props)(children)`.
    /// Same output as `createElement(tag, props, ...children)` but with the
    /// kwargs and children sourced from different syntactic positions.
    fn emit_psx_element_with_children(&mut self, tag: &str, kwargs: &[Keyword], children: &[Expr]) {
        self.emit_psx_element(tag, children, kwargs);
    }

    fn emit_psx_element(&mut self, tag: &str, args: &[Expr], kwargs: &[Keyword]) {
        self.needs_create_element = true;

        // Tag: HTML elements as strings, Components (uppercase) as identifiers.
        // For HTML tags we apply React's snake→camel prop convention (className,
        // onClick, htmlFor, …). For user @component calls we leave names alone
        // since the user defined them in their own snake_case vocabulary.
        //
        // Track-B: LIBRARY components — tags whose root binding was imported
        // from a React-ecosystem npm module (directly, via a module alias, or
        // a motion-style member base) — get the SAME snake→camel conversion
        // HTML tags get: the receiving library's prop vocabulary is camelCase
        // (`onOpenChange`, `asChild`, `strokeWidth`), so passing snake_case
        // through silently drops the prop. camelCase written verbatim still
        // works (conversion no-ops on names without underscores).
        let tag_root = tag.split('.').next().unwrap_or(tag);
        let is_component_tag = tag.chars().next().is_some_and(|c| c.is_uppercase())
            || self.react_member_component_bases.contains(tag_root);
        let is_library_component = is_component_tag
            && (self.react_lib_bindings.contains(tag_root)
                || self.react_lib_module_aliases.contains(tag_root));
        let convert_props = !is_component_tag || is_library_component;
        if is_component_tag {
            self.write(&format!("createElement({}", tag));
        } else {
            self.write(&format!("createElement(\"{}\"", tag));
        }

        // Props object (or null if no props)
        if kwargs.is_empty() {
            self.write(", null");
        } else {
            self.write(", {");
            for (i, kw) in kwargs.iter().enumerate() {
                if i > 0 {
                    self.write(", ");
                }
                if let Some(name) = &kw.name {
                    let js_prop = if convert_props {
                        react::snake_to_camel(name)
                    } else {
                        name.clone()
                    };
                    // Some props convert to kebab-case (`aria-label`,
                    // `data-id`) — those aren't valid JS identifiers
                    // unquoted in object literals, so wrap them in
                    // quotes. camelCase / single-word props stay bare.
                    if is_valid_js_identifier(&js_prop) {
                        self.write(&format!("{}: ", js_prop));
                    } else {
                        self.write(&format!("\"{}\": ", js_prop));
                    }
                    // Special case: a `style={...}` prop on an HTML
                    // element needs every CSS key snake→camel'd. React
                    // drops unknown camelCase-only properties, and
                    // writing `border_radius` instead of `borderRadius`
                    // silently fails to render. Two paths:
                    //
                    //   - Dict literal at the call site → snake→camel
                    //     each key at compile time (free, readable JS).
                    //   - Anything else (variable, function call, etc.)
                    //     → wrap in pyNormalizeStyle() so the runtime
                    //     can do the conversion. We don't know what
                    //     keys the value has at codegen time.
                    if convert_props && name == "style" {
                        match &kw.value.kind {
                            ExprKind::Dict { items } => {
                                self.emit_style_dict(items);
                                continue;
                            }
                            // Skip the wrap if the receiver is itself a
                            // call to pyNormalizeStyle (idempotent).
                            ExprKind::Call { func, .. } if matches!(&func.kind, ExprKind::Name(n) if n == "pyNormalizeStyle") =>
                            {
                                self.emit_expr(&kw.value);
                                continue;
                            }
                            _ => {
                                self.need_runtime("pyNormalizeStyle");
                                self.write("pyNormalizeStyle(");
                                self.emit_expr(&kw.value);
                                self.write(")");
                                continue;
                            }
                        }
                    }
                    // #122: the Python loop-capture idiom in an event-
                    // handler prop. `lambda i=i: f(i)` compiled faithfully
                    // to `(i = i) => f(i)` — but React invokes handlers
                    // WITH the SyntheticEvent, overriding the "captured"
                    // value (and the JS self-default is a TDZ
                    // ReferenceError when called argless). A param
                    // defaulted to ITS OWN NAME is unambiguously
                    // creation-time capture (which is when CPython
                    // evaluates defaults too): lower it to a real captured
                    // binding via an IIFE; other params (e.g. `e=None`)
                    // stay real params and receive the event.
                    if name.starts_with("on") && self.try_emit_capture_lambda(&kw.value) {
                        continue;
                    }
                    self.emit_expr(&kw.value);
                } else {
                    // **kwargs spread
                    self.write("...");
                    self.emit_expr(&kw.value);
                }
            }
            self.write("}");
        }

        // Children from positional args
        for arg in args {
            self.write(", ");
            match &arg.kind {
                ExprKind::Call {
                    func,
                    args: child_args,
                    kwargs: child_kwargs,
                    ..
                } => {
                    if let ExprKind::Name(name) = &func.kind {
                        if self.is_psx_tag_call(name) {
                            self.emit_psx_element(name, child_args, child_kwargs);
                            continue;
                        }
                    }
                    self.emit_expr(arg);
                }
                _ => {
                    self.emit_expr(arg);
                }
            }
        }

        self.write(")");
    }

    /// Emit a Dict literal as a React `style={{...}}` object — every
    /// snake_case key is converted to camelCase. Spread items pass through
    /// unchanged, and non-string-literal keys are left alone (we can't
    /// transform a key whose name we don't know at compile time).
    fn emit_style_dict(&mut self, items: &[DictItem]) {
        self.write("({");
        for (i, item) in items.iter().enumerate() {
            if i > 0 {
                self.write(", ");
            }
            match item {
                DictItem::Spread(expr) => {
                    self.write("...");
                    self.emit_expr(expr);
                }
                DictItem::KeyValue { key, value } => {
                    match &key.kind {
                        ExprKind::StringLiteral(s) => {
                            // SECURITY (A6): the key is source-derived; escape it
                            // before emitting the quoted literal, mirroring the
                            // general-dict path (emit_plain_dict_chunk).
                            let js_key = react::snake_to_camel(s);
                            self.write(&format!("{}: ", js_string_literal(&js_key)));
                        }
                        _ => {
                            self.write("[");
                            self.emit_expr(key);
                            self.write("]: ");
                        }
                    }
                    self.emit_expr(value);
                }
            }
        }
        self.write("})");
    }

    /// Emit a fragment as createElement(Fragment, null, ...children)
    fn emit_psx_fragment(&mut self, children: &[Expr]) {
        self.needs_create_element = true;
        self.needs_fragment = true;
        self.write("createElement(Fragment, null");
        for child in children {
            self.write(", ");
            match &child.kind {
                ExprKind::Call {
                    func, args, kwargs, ..
                } => {
                    if let ExprKind::Name(name) = &func.kind {
                        if self.is_psx_tag_call(name) {
                            self.emit_psx_element(name, args, kwargs);
                            continue;
                        }
                    }
                    self.emit_expr(child);
                }
                _ => {
                    self.emit_expr(child);
                }
            }
        }
        self.write(")");
    }

    /// Round-2 pythonic sweep: true iff the expression tree contains a
    /// walrus (`NamedExpr`). The comprehension fast path
    /// (`.filter().map()`) runs TWO passes over the iterable, so a walrus
    /// bound in the filter and read in the map would observe the LAST
    /// filter pass's value ([18, 18, 18] instead of [14, 16, 18]); any
    /// comprehension containing a walrus must take the single-pass loop
    /// path instead.
    fn expr_contains_walrus(expr: &Expr) -> bool {
        use ExprKind as E;
        let any = |exprs: &[Expr]| exprs.iter().any(Self::expr_contains_walrus);
        match &expr.kind {
            E::NamedExpr { .. } => true,
            E::BinOp { left, right, .. } => {
                Self::expr_contains_walrus(left) || Self::expr_contains_walrus(right)
            }
            E::UnaryOp { operand, .. } => Self::expr_contains_walrus(operand),
            E::Compare { left, comparisons } => {
                Self::expr_contains_walrus(left)
                    || comparisons
                        .iter()
                        .any(|(_, e)| Self::expr_contains_walrus(e))
            }
            E::Call {
                func, args, kwargs, ..
            } => {
                Self::expr_contains_walrus(func)
                    || any(args)
                    || kwargs.iter().any(|k| Self::expr_contains_walrus(&k.value))
            }
            E::Attribute { value, .. } => Self::expr_contains_walrus(value),
            E::Subscript { value, index, .. } => {
                Self::expr_contains_walrus(value) || Self::expr_contains_walrus(index)
            }
            E::Slice { lower, upper, step } => [lower, upper, step]
                .into_iter()
                .flatten()
                .any(|e| Self::expr_contains_walrus(e)),
            E::List(elts) | E::Tuple(elts) | E::Set(elts) => any(elts),
            E::Dict { items } => items.iter().any(|i| match i {
                DictItem::KeyValue { key, value } => {
                    Self::expr_contains_walrus(key) || Self::expr_contains_walrus(value)
                }
                DictItem::Spread(e) => Self::expr_contains_walrus(e),
            }),
            E::FString { parts } => parts.iter().any(|p| match p {
                FStringPart::Expr(e) => Self::expr_contains_walrus(e),
                FStringPart::Literal(_) => false,
            }),
            E::ListComp { elt, generators }
            | E::SetComp { elt, generators }
            | E::GeneratorExp { elt, generators } => {
                Self::expr_contains_walrus(elt)
                    || generators
                        .iter()
                        .any(|g| Self::expr_contains_walrus(&g.iter) || any(&g.ifs))
            }
            E::DictComp {
                key,
                value,
                generators,
            } => {
                Self::expr_contains_walrus(key)
                    || Self::expr_contains_walrus(value)
                    || generators
                        .iter()
                        .any(|g| Self::expr_contains_walrus(&g.iter) || any(&g.ifs))
            }
            E::Lambda { body, .. } => Self::expr_contains_walrus(body),
            E::IfExpr {
                test,
                body,
                else_body,
            } => {
                Self::expr_contains_walrus(test)
                    || Self::expr_contains_walrus(body)
                    || Self::expr_contains_walrus(else_body)
            }
            E::Starred(e) | E::Await(e) | E::YieldFrom(e) => Self::expr_contains_walrus(e),
            E::Yield(v) => v.as_deref().is_some_and(Self::expr_contains_walrus),
            _ => false,
        }
    }

    fn emit_list_comprehension(&mut self, elt: &Expr, generators: &[Comprehension]) {
        // [expr for x in xs if cond] → xs.filter(x => cond).map(x => expr)
        // For multiple generators, async-for, or complex cases, use an
        // IIFE with loops. `.filter().map()` doesn't support async
        // iteration cleanly, so async-for must take the loop path.
        // A walrus anywhere in the element/conditions also forces the
        // loop path (see expr_contains_walrus).
        let any_async = generators.iter().any(|g| g.is_async);
        let has_walrus = Self::expr_contains_walrus(elt)
            || generators
                .iter()
                .any(|g| g.ifs.iter().any(Self::expr_contains_walrus));
        if !any_async && !has_walrus && generators.len() == 1 && generators[0].ifs.len() <= 1 {
            let gen = &generators[0];
            self.emit_iterable_as_array(&gen.iter);
            if !gen.ifs.is_empty() {
                self.write(".filter((");
                self.emit_for_target(&gen.target);
                self.write(") => ");
                self.emit_expr(&gen.ifs[0]);
                self.write(")");
            }
            self.write(".map((");
            self.emit_for_target(&gen.target);
            self.write(") => ");
            self.emit_expr(elt);
            self.write(")");
        } else {
            // Complex comprehension — use IIFE with loops. Async-for
            // requires `async` on the IIFE so `await` inside it works.
            // Round-4 sweep: the async IIFE returns a Promise — await it
            // in place when awaiting is legal here (async def body /
            // module top level), otherwise the caller gets the Promise
            // (pre-existing documented limit).
            let wrap_await = any_async && self.await_ok;
            if wrap_await {
                self.write("(await ");
            }
            if any_async {
                self.write("(async () => { const __result = []; ");
            } else {
                self.write("(() => { const __result = []; ");
            }
            self.emit_comprehension_loops(elt, generators, 0);
            self.write(" return __result; })()");
            if wrap_await {
                self.write(")");
            }
        }
    }

    /// #155: generator expressions compile to a lazy JS generator IIFE:
    ///
    ///   (function* (__gen_it) { for (const x of __gen_it) { if (c) { yield e; } } })
    ///       .call(this, XS)
    ///
    /// Design notes:
    /// - The OUTERMOST iterable is evaluated eagerly at creation time and
    ///   passed as the IIFE argument — CPython calls iter(outermost) when
    ///   the genexp object is built; inner iterables/conditions/element
    ///   stay lazy (evaluated during consumption).
    /// - `.call(this, ...)` instead of a plain call: `self` inside method
    ///   bodies is rewritten to `this`, and a bare `function*` would
    ///   shadow it. At module top level `this` is undefined in ESM, which
    ///   is harmless.
    /// - Async genexps become `async function*` (an async-generator
    ///   object, consumable with `async for` / for-await), with the
    ///   Python-protocol bridge applied to each async source.
    /// - Walrus targets keep working: they're hoisted as `let` in the
    ///   enclosing function scope (PEP 572) and simply assigned from
    ///   inside the generator body on consumption — which is also
    ///   CPython's (lazy) binding timing.
    fn emit_generator_exp(&mut self, elt: &Expr, generators: &[Comprehension]) {
        let any_async = generators.iter().any(|g| g.is_async);
        if any_async {
            self.write("(async function* (__gen_it) { ");
        } else {
            self.write("(function* (__gen_it) { ");
        }
        self.emit_genexp_loops(elt, generators, 0);
        self.write("}).call(this, ");
        let first = &generators[0];
        if first.is_async {
            // #239: async iterable — raw expr through __pyAsyncIter, not the
            // sync pyForIter/pyDictKeys wrap.
            self.need_runtime("__pyAsyncIter");
            self.write("__pyAsyncIter(");
            self.emit_expr(&first.iter);
            self.write(")");
        } else {
            self.emit_iterable(&first.iter);
        }
        self.write(")");
    }

    /// Loop-nest body for emit_generator_exp. Identical shape to
    /// emit_comprehension_loops except the innermost statement is `yield`
    /// (not `__result.push`) and the outermost source is the pre-evaluated
    /// `__gen_it` parameter.
    fn emit_genexp_loops(&mut self, elt: &Expr, generators: &[Comprehension], idx: usize) {
        if idx >= generators.len() {
            self.write("yield ");
            self.emit_expr(elt);
            self.write("; ");
            return;
        }

        let gen = &generators[idx];
        if gen.is_async {
            self.write("for await (const ");
        } else {
            self.write("for (const ");
        }
        self.emit_for_target(&gen.target);
        self.write(" of ");
        if idx == 0 {
            // Outermost iterable was evaluated at creation time and passed
            // as the IIFE argument (already async-bridged if needed).
            self.write("__gen_it");
        } else if gen.is_async {
            self.need_runtime("__pyAsyncIter");
            self.write("__pyAsyncIter(");
            self.emit_iterable(&gen.iter);
            self.write(")");
        } else {
            self.emit_iterable(&gen.iter);
        }
        self.write(") { ");

        for cond in &gen.ifs {
            self.write("if (");
            self.emit_expr(cond);
            self.write(") { ");
        }

        self.emit_genexp_loops(elt, generators, idx + 1);

        for _ in &gen.ifs {
            self.write("} ");
        }
        self.write("} ");
    }

    fn emit_comprehension_loops(&mut self, elt: &Expr, generators: &[Comprehension], idx: usize) {
        if idx >= generators.len() {
            self.write("__result.push(");
            self.emit_expr(elt);
            self.write("); ");
            return;
        }

        let gen = &generators[idx];
        // `async for x in xs` (PEP 530) lowers to `for await (const x
        // of xs)`. The surrounding context must itself be async — the
        // user is responsible for putting the comprehension inside an
        // `async def`.
        if gen.is_async {
            self.write("for await (const ");
        } else {
            self.write("for (const ");
        }
        self.emit_for_target(&gen.target);
        self.write(" of ");
        // Round-4 sweep: bridge Python-protocol async iterables (see
        // emit_for's async arm).
        if gen.is_async {
            self.need_runtime("__pyAsyncIter");
            self.write("__pyAsyncIter(");
            self.emit_iterable(&gen.iter);
            self.write(")");
        } else {
            self.emit_iterable(&gen.iter);
        }
        self.write(") { ");

        for cond in &gen.ifs {
            self.write("if (");
            self.emit_expr(cond);
            self.write(") { ");
        }

        self.emit_comprehension_loops(elt, generators, idx + 1);

        for _ in &gen.ifs {
            self.write("} ");
        }
        self.write("} ");
    }

    fn emit_for_target(&mut self, target: &Expr) {
        match &target.kind {
            // Pythonic-checks sweep: recurse so a NESTED tuple/list target
            // (`for i, (x, y) in ...`) emits a destructuring pattern
            // (`[i, [x, y]]`), not a pyTuple(...) VALUE inside the pattern
            // (a JS syntax error).
            ExprKind::Tuple(elts) | ExprKind::List(elts) => {
                self.write("[");
                for (i, e) in elts.iter().enumerate() {
                    if i > 0 {
                        self.write(", ");
                    }
                    self.emit_for_target(e);
                }
                self.write("]");
            }
            // PBT-2: a comprehension/genexp target is a BINDING position
            // (arrow parameter or `for (const …)`) — a sentinel-guarded name
            // must emit bare, never as a __pyChkLocal read (caught by the
            // LiveCodeBench net: `(__pyChkLocal(i, "i")) => …` is a JS
            // SyntaxError).
            _ => {
                let was_lhs = self.in_lhs_target;
                self.in_lhs_target = true;
                self.emit_expr(target);
                self.in_lhs_target = was_lhs;
            }
        }
    }

    /// #83: true iff a dict-literal / dict-comprehension key expression is
    /// provably a string at compile time (string literal, f-string, or a
    /// `str(...)` call). Only such keys may live in a plain-object dict —
    /// JS object keys stringify everything else, so any other key shape
    /// gets the Map-backed `PyDict` representation instead.
    fn key_provably_string(key: &Expr) -> bool {
        match &key.kind {
            ExprKind::StringLiteral(_) | ExprKind::FString { .. } => true,
            ExprKind::Call { func, .. } => {
                matches!(&func.kind, ExprKind::Name(n) if n == "str")
            }
            _ => false,
        }
    }

    /// #83 hybrid dict-literal emission.
    ///
    /// - No spread + all keys provably strings → plain object literal
    ///   (today's shape: full JS interop — React props, JSON, spread).
    /// - No spread + any non-string key → `new PyDict([[k, v], ...])`.
    /// - Any spread → `pyDictMerge(part, ...)`: a spread argument can be a
    ///   Map-backed dict at runtime, which `{...m}` would silently drop
    ///   (a Map has no own enumerable props). pyDictMerge shape-dispatches
    ///   and returns a plain object unless some part is Map-backed.
    fn emit_dict_literal(&mut self, items: &[DictItem]) {
        // #106: one-shot force flag — the literal is assigned to a name
        // later subscript-written with a non-string literal key, so it
        // must be Map-backed from construction.
        let forced = std::mem::take(&mut self.force_pydict_literal);
        let has_spread = items.iter().any(|i| matches!(i, DictItem::Spread(_)));
        if !has_spread {
            let all_string = !forced
                && items.iter().all(|i| match i {
                    DictItem::KeyValue { key, .. } => Self::key_provably_string(key),
                    DictItem::Spread(_) => true,
                });
            let refs: Vec<&DictItem> = items.iter().collect();
            if all_string {
                self.emit_plain_dict_chunk(&refs);
            } else {
                self.emit_pydict_chunk(&refs);
            }
            return;
        }
        self.need_runtime("pyDictMerge");
        self.write("pyDictMerge(");
        let mut first = true;
        let mut chunk: Vec<&DictItem> = Vec::new();
        for item in items {
            match item {
                DictItem::Spread(expr) => {
                    if !chunk.is_empty() {
                        if !first {
                            self.write(", ");
                        }
                        first = false;
                        self.emit_dict_chunk(&chunk);
                        chunk.clear();
                    }
                    if !first {
                        self.write(", ");
                    }
                    first = false;
                    self.emit_expr(expr);
                }
                DictItem::KeyValue { .. } => chunk.push(item),
            }
        }
        if !chunk.is_empty() {
            if !first {
                self.write(", ");
            }
            self.emit_dict_chunk(&chunk);
        }
        self.write(")");
    }

    /// Emit a run of KeyValue dict items in whichever shape its keys allow.
    fn emit_dict_chunk(&mut self, chunk: &[&DictItem]) {
        let all_string = chunk.iter().all(|i| match i {
            DictItem::KeyValue { key, .. } => Self::key_provably_string(key),
            DictItem::Spread(_) => true,
        });
        if all_string {
            self.emit_plain_dict_chunk(chunk);
        } else {
            self.emit_pydict_chunk(chunk);
        }
    }

    /// Plain-object dict emission (pre-#83 shape) for all-string-key runs.
    fn emit_plain_dict_chunk(&mut self, items: &[&DictItem]) {
        self.write("({");
        for (i, item) in items.iter().enumerate() {
            if i > 0 {
                self.write(", ");
            }
            match item {
                DictItem::Spread(expr) => {
                    self.write("...");
                    self.emit_expr(expr);
                }
                DictItem::KeyValue { key, value } => {
                    match &key.kind {
                        ExprKind::StringLiteral(s) if s == "__proto__" => {
                            // F3: a bare `"__proto__": v` object-literal
                            // entry mutates the prototype instead of
                            // creating an own key (crashes/pollutes).
                            // The computed-key form `["__proto__"]: v`
                            // defines a real own property, matching
                            // Python dict semantics.
                            self.write("[\"__proto__\"]: ");
                        }
                        ExprKind::StringLiteral(s) => {
                            // Transform known React prop names (on_click → onClick, etc.)
                            let js_key = react::react_prop_mapping(s)
                                .map(|m| m.to_string())
                                .unwrap_or_else(|| escape_js_string(s));
                            self.write(&format!("\"{}\": ", js_key));
                        }
                        _ => {
                            self.write("[");
                            self.emit_expr(key);
                            self.write("]: ");
                        }
                    }
                    self.emit_expr(value);
                }
            }
        }
        self.write("})");
    }

    /// Map-backed dict emission for key runs with any non-string key
    /// (#83): `new PyDict([[k, v], ...])`. PyDict canonicalizes keys the
    /// CPython way (True↔1, 1.0↔1, tuples by structure) and is inherently
    /// proto-safe (Map keys never touch the prototype).
    fn emit_pydict_chunk(&mut self, items: &[&DictItem]) {
        self.need_runtime("PyDict");
        self.write("new PyDict([");
        for (i, item) in items.iter().enumerate() {
            if i > 0 {
                self.write(", ");
            }
            if let DictItem::KeyValue { key, value } = item {
                self.write("[");
                self.emit_expr(key);
                self.write(", ");
                self.emit_expr(value);
                self.write("]");
            }
        }
        self.write("])");
    }

    /// Emit an expression in iterable position (for-loops, comprehension
    /// sources). Python iterates dicts by KEY; plain-object dicts aren't
    /// even iterable in JS, so a Dict-typed iterable gets wrapped in the
    /// shape-dispatching pyDictKeys (returns an array for both shapes).
    fn emit_iterable(&mut self, iter: &Expr) {
        match self.infer_type(iter) {
            JsInferredType::Dict => {
                self.need_runtime("pyDictKeys");
                self.write("pyDictKeys(");
                self.emit_expr(iter);
                self.write(")");
            }
            // #239: an UNKNOWN operand (e.g. an untyped dict parameter) may be a
            // plain-object dict (not JS-iterable) or a Map/Counter (whose for..of
            // yields entries, not keys). Route through pyForIter, which iterates
            // dict keys and passes lists/tuples/sets/strings/generators through.
            JsInferredType::Unknown => {
                self.need_runtime("pyForIter");
                self.write("pyForIter(");
                self.emit_expr(iter);
                self.write(")");
            }
            _ => self.emit_expr(iter),
        }
    }

    /// Like `emit_iterable`, but the result is guaranteed to be a real JS
    /// Array — required by the comprehension fast path, which chains
    /// `.filter(...).map(...)` on it. Strings have no `.map`; generators
    /// DO have one in Node 22+ (Iterator Helpers) but it returns a lazy
    /// iterator, not an Array — both were producing wrong output for
    /// comprehensions over non-array iterables (Pythonic-checks sweep).
    /// Provably-array iterables (lists, tuples) and dicts (pyDictKeys
    /// already returns an array) skip the pySeq wrap.
    fn emit_iterable_as_array(&mut self, iter: &Expr) {
        match self.infer_type(iter) {
            JsInferredType::List | JsInferredType::Tuple => self.emit_expr(iter),
            JsInferredType::Dict => {
                self.need_runtime("pyDictKeys");
                self.write("pyDictKeys(");
                self.emit_expr(iter);
                self.write(")");
            }
            _ => {
                self.need_runtime("pySeq");
                self.write("pySeq(");
                self.emit_expr(iter);
                self.write(")");
            }
        }
    }

    fn emit_dict_comprehension(&mut self, key: &Expr, value: &Expr, generators: &[Comprehension]) {
        // #83: string-keyed comprehensions keep the plain-object shape;
        // any other key expression builds a Map-backed PyDict from the
        // same [key, value] pair stream.
        if Self::key_provably_string(key) {
            self.write("Object.fromEntries(");
        } else {
            self.need_runtime("PyDict");
            self.write("new PyDict(");
        }
        // Reuse list comprehension to generate [key, value] pairs. A walrus
        // in the key/value/conditions forces the single-pass loop path
        // (see expr_contains_walrus).
        let has_walrus = Self::expr_contains_walrus(key)
            || Self::expr_contains_walrus(value)
            || generators
                .iter()
                .any(|g| g.ifs.iter().any(Self::expr_contains_walrus));
        if !has_walrus && generators.len() == 1 && generators[0].ifs.len() <= 1 {
            let gen = &generators[0];
            self.emit_iterable_as_array(&gen.iter);
            if !gen.ifs.is_empty() {
                self.write(".filter((");
                self.emit_for_target(&gen.target);
                self.write(") => ");
                self.emit_expr(&gen.ifs[0]);
                self.write(")");
            }
            self.write(".map((");
            self.emit_for_target(&gen.target);
            self.write(") => [");
            self.emit_expr(key);
            self.write(", ");
            self.emit_expr(value);
            self.write("])");
        } else {
            self.write("(() => { const __result = []; ");
            // Use dict-specific loop emission
            self.emit_dict_comprehension_loops(key, value, generators, 0);
            self.write(" return __result; })()");
        }
        self.write(")");
    }

    fn emit_dict_comprehension_loops(
        &mut self,
        key: &Expr,
        value: &Expr,
        generators: &[Comprehension],
        idx: usize,
    ) {
        if idx >= generators.len() {
            self.write("__result.push([");
            self.emit_expr(key);
            self.write(", ");
            self.emit_expr(value);
            self.write("]); ");
            return;
        }

        let gen = &generators[idx];
        self.write("for (const ");
        self.emit_for_target(&gen.target);
        self.write(" of ");
        self.emit_iterable(&gen.iter);
        self.write(") { ");

        for cond in &gen.ifs {
            self.write("if (");
            self.emit_expr(cond);
            self.write(") { ");
        }

        self.emit_dict_comprehension_loops(key, value, generators, idx + 1);

        for _ in &gen.ifs {
            self.write("} ");
        }
        self.write("} ");
    }
}

fn aug_assign_op_str(op: &AugAssignOp) -> &'static str {
    match op {
        AugAssignOp::Add => "+=",
        AugAssignOp::Sub => "-=",
        AugAssignOp::Mul => "*=",
        AugAssignOp::Div => "/=",
        AugAssignOp::FloorDiv => "/* //= */",
        AugAssignOp::Mod => "%=",
        AugAssignOp::Pow => "**=",
        AugAssignOp::BitAnd => "&=",
        AugAssignOp::BitOr => "|=",
        AugAssignOp::BitXor => "^=",
        AugAssignOp::ShiftLeft => "<<=",
        AugAssignOp::ShiftRight => ">>=",
    }
}

/// #306: Python builtin callables spelled in all-lowercase (i.e. names the
/// unbound-PSX-tag fallback could otherwise claim). The lowering tables in
/// `builtins.rs` cover the mapped subset; this adds the remaining CPython
/// builtins (supported or not) so `getattr(...)` / `callable(...)` inside a
/// @component never turn into `createElement("getattr", ...)`. Builtins that
/// double as HTML tags (`map`, `input`, `object`, `time`) are claimed by the
/// element allowlist FIRST inside components — longstanding, documented
/// behavior this list does not change.
fn is_python_builtin_name(name: &str) -> bool {
    crate::builtins::builtin_func_mapping(name).is_some()
        || crate::builtins::builtin_value_mapping(name).is_some()
        || matches!(
            name,
            "getattr"
                | "setattr"
                | "hasattr"
                | "delattr"
                | "callable"
                | "vars"
                | "globals"
                | "locals"
                | "id"
                | "hash"
                | "format"
                | "frozenset"
                | "bytes"
                | "bytearray"
                | "memoryview"
                | "complex"
                | "object"
                | "open"
                | "eval"
                | "exec"
                | "compile"
                | "dir"
                | "issubclass"
                | "isinstance"
                | "aiter"
                | "anext"
                | "super"
                | "breakpoint"
                | "exit"
                | "quit"
                | "pow"
                | "slice"
                | "staticmethod"
                | "classmethod"
                | "property"
                | "help"
                | "hex"
                | "oct"
                | "bin"
                | "ascii"
        )
}

/// #306 follow-up (FlameReact f015/f062 regression): all-lowercase JS/browser
/// GLOBALS that are legitimately CALLED bare inside a @component —
/// `fetch(url)` in an async handler, `alert(...)`, `atob/btoa`, ... These are
/// tag-shaped and unbound (they're host globals, not module bindings), so the
/// unbound-PSX-tag fallback would otherwise claim them as elements
/// (`createElement("fetch", ...)` — an element object whose `.json()` then
/// explodes). None of these names is an HTML element, so guarding them costs
/// nothing. camelCase globals (setTimeout, parseInt, requestAnimationFrame,
/// structuredClone, ...) never match the tag shape and need no entry.
fn is_js_global_callable(name: &str) -> bool {
    matches!(
        name,
        "fetch" | "alert" | "confirm" | "prompt" | "atob" | "btoa"
            | "escape" | "unescape" | "require"
            // window methods occasionally called bare
            | "open" | "close" | "stop" | "focus" | "blur" | "scroll"
    )
}

/// True iff `s` is a valid JS identifier (can appear unquoted as an
/// object literal key). Conservative subset: ASCII letters, digits,
/// `_`, `$`; first char can't be a digit. Hyphens (kebab-case CSS/ARIA
/// keys) and other punctuation force quoting.
fn is_valid_js_identifier(s: &str) -> bool {
    let mut chars = s.chars();
    let first = match chars.next() {
        Some(c) => c,
        None => return false,
    };
    if !(first.is_ascii_alphabetic() || first == '_' || first == '$') {
        return false;
    }
    chars.all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '$')
}

/// F1: JS reserved / contextual / strict-mode-reserved words that are ALSO
/// legal Python identifiers, so a user's Python binding can collide with one.
/// Python keywords (`class`, `import`, `while`, `with`, `yield`, ...) can never
/// reach codegen as identifiers, so they're harmless to list but mostly
/// omitted.
///
/// Sweep-A S1 finding: `super` IS sanitized (unlike an earlier revision of
/// this comment claimed). `super` is not a Python keyword, so `super = 5`
/// is legal Python that binds a plain variable — but bare `super` is a full
/// JS reserved word in every position (`let super = 5;` is a SyntaxError
/// under Node ESM's implicit strict mode), not just inside a class method.
/// This does not conflict with cooperative-`super()` lowering: `super()`
/// *calls* are intercepted by a dedicated `ExprKind::Name(n) if n == "super"`
/// match in the `Call` handling (see the two `"super"` matches elsewhere in
/// this file) before a bare `Name("super")` ever reaches general identifier
/// emission, so that path is unaffected by sanitizing the identifier here.
fn is_js_reserved_word(s: &str) -> bool {
    matches!(
        s,
        "let"
            | "const"
            | "var"
            | "new"
            | "function"
            | "this"
            | "typeof"
            | "delete"
            | "void"
            | "switch"
            | "case"
            | "default"
            | "catch"
            | "do"
            | "enum"
            | "export"
            | "extends"
            | "instanceof"
            | "throw"
            | "static"
            | "debugger"
            | "null"
            | "true"
            | "false"
            | "undefined"
            | "NaN"
            | "Infinity"
            | "arguments"
            | "eval"
            | "await"
            | "yield"
            | "super"
            | "implements"
            | "interface"
            | "package"
            | "private"
            | "protected"
            | "public"
            | "finally"
            | "in"
            | "of"
            | "import"
    )
}

/// SECURITY INVARIANT: any JS string literal built from source- or
/// config-derived text (module specifiers, dict keys, dataclass `choices`,
/// PSX props, string values, error messages that embed any of these, ...)
/// MUST be produced with `escape_js_string` / `js_string_literal`. NEVER
/// `format!("\"{}\"", x)` on such `x` — a raw `"`, `\`, newline, or
/// U+2028/U+2029 in `x` closes the literal early and injects arbitrary
/// top-level JS into the emitted module. Only values known to be parser
/// identifiers (which cannot contain a quote) are exempt. See the
/// arbitrary-JS-injection cluster fix (sites A1/A2/A5/A6).
fn escape_js_string(s: &str) -> String {
    // #95: string values are now DECODED at the lexer boundary, so any
    // control character can appear here and must be re-escaped for the
    // emitted JS double-quoted literal.
    let mut out = String::with_capacity(s.len());
    for ch in s.chars() {
        match ch {
            '\\' => out.push_str("\\\\"),
            '"' => out.push_str("\\\""),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            '\u{2028}' => out.push_str("\\u2028"),
            '\u{2029}' => out.push_str("\\u2029"),
            c if (c as u32) < 0x20 || c as u32 == 0x7f => {
                out.push_str(&format!("\\x{:02x}", c as u32));
            }
            c => out.push(c),
        }
    }
    out
}

/// Wrap a string in JS double-quote literal form with escapes applied.
fn js_string_literal(s: &str) -> String {
    format!("\"{}\"", escape_js_string(s))
}

/// Compute the React Refresh hook signature for a component body.
///
/// Walks the body in source order and collects the **names** of all
/// hook calls — call expressions where the callee is a `Name` or
/// `Attribute` ending in an identifier that starts with `use` (or
/// the Python-source `use_` convention). The signature is the list
/// joined by `\n`, matching the format `react-refresh/babel-plugin`
/// emits.
///
/// What matters is **stability** across edits: if the user changes
/// pure JSX without touching hook calls, the signature is identical
/// and React preserves state. If the user adds or reorders a hook,
/// the signature changes and React performs a safe remount.
fn refresh_hook_signature(body: &[Stmt]) -> String {
    let mut hooks = Vec::new();
    for stmt in body {
        collect_hooks_in_stmt(stmt, &mut hooks);
    }
    hooks.join("\n")
}

fn is_hook_name(name: &str) -> bool {
    // Python-source: `use_state`, `use_effect`, custom `use_my_hook`.
    // Either snake_case (`use_*`) or already-camel (`useState` — appears
    // when names have been emitted through stub-inferred typings; rare
    // in source but possible after expansion).
    if let Some(rest) = name.strip_prefix("use_") {
        return !rest.is_empty();
    }
    if let Some(rest) = name.strip_prefix("use") {
        return rest.chars().next().is_some_and(|c| c.is_ascii_uppercase());
    }
    false
}

fn collect_hooks_in_stmt(stmt: &Stmt, out: &mut Vec<String>) {
    match &stmt.kind {
        StmtKind::Expr(e) => collect_hooks_in_expr(e, out),
        StmtKind::Assign { value, .. } => collect_hooks_in_expr(value, out),
        StmtKind::AugAssign { value, .. } => collect_hooks_in_expr(value, out),
        StmtKind::AnnAssign { value: Some(v), .. } => collect_hooks_in_expr(v, out),
        StmtKind::Return(Some(e)) => collect_hooks_in_expr(e, out),
        StmtKind::If {
            test,
            body,
            elif_clauses,
            else_body,
        } => {
            collect_hooks_in_expr(test, out);
            for s in body {
                collect_hooks_in_stmt(s, out);
            }
            for (cond, blk) in elif_clauses {
                collect_hooks_in_expr(cond, out);
                for s in blk {
                    collect_hooks_in_stmt(s, out);
                }
            }
            if let Some(eb) = else_body {
                for s in eb {
                    collect_hooks_in_stmt(s, out);
                }
            }
        }
        StmtKind::While { test, body, .. } => {
            collect_hooks_in_expr(test, out);
            for s in body {
                collect_hooks_in_stmt(s, out);
            }
        }
        StmtKind::For { iter, body, .. } => {
            collect_hooks_in_expr(iter, out);
            for s in body {
                collect_hooks_in_stmt(s, out);
            }
        }
        StmtKind::Try {
            body,
            handlers,
            else_body,
            finally_body,
        } => {
            for s in body {
                collect_hooks_in_stmt(s, out);
            }
            for h in handlers {
                for s in &h.body {
                    collect_hooks_in_stmt(s, out);
                }
            }
            if let Some(eb) = else_body {
                for s in eb {
                    collect_hooks_in_stmt(s, out);
                }
            }
            if let Some(fb) = finally_body {
                for s in fb {
                    collect_hooks_in_stmt(s, out);
                }
            }
        }
        StmtKind::With { items, body, .. } => {
            for item in items {
                collect_hooks_in_expr(&item.context_expr, out);
            }
            for s in body {
                collect_hooks_in_stmt(s, out);
            }
        }
        _ => {}
    }
}

fn collect_hooks_in_expr(expr: &Expr, out: &mut Vec<String>) {
    match &expr.kind {
        ExprKind::Call {
            func, args, kwargs, ..
        } => {
            let hook_name: Option<String> = match &func.kind {
                ExprKind::Name(n) if is_hook_name(n) => Some(n.clone()),
                ExprKind::Attribute { attr, .. } if is_hook_name(attr) => Some(attr.clone()),
                _ => None,
            };
            if let Some(n) = hook_name {
                out.push(n);
            }
            collect_hooks_in_expr(func, out);
            for a in args {
                collect_hooks_in_expr(a, out);
            }
            for k in kwargs {
                collect_hooks_in_expr(&k.value, out);
            }
        }
        ExprKind::BinOp { left, right, .. } => {
            collect_hooks_in_expr(left, out);
            collect_hooks_in_expr(right, out);
        }
        ExprKind::UnaryOp { operand, .. } => collect_hooks_in_expr(operand, out),
        ExprKind::Compare { left, comparisons } => {
            collect_hooks_in_expr(left, out);
            for (_, c) in comparisons {
                collect_hooks_in_expr(c, out);
            }
        }
        ExprKind::IfExpr {
            test,
            body,
            else_body,
        } => {
            collect_hooks_in_expr(test, out);
            collect_hooks_in_expr(body, out);
            collect_hooks_in_expr(else_body, out);
        }
        ExprKind::Attribute { value, .. } => collect_hooks_in_expr(value, out),
        ExprKind::Subscript { value, index, .. } => {
            collect_hooks_in_expr(value, out);
            collect_hooks_in_expr(index, out);
        }
        ExprKind::Lambda { body, .. } => collect_hooks_in_expr(body, out),
        ExprKind::List(items) | ExprKind::Tuple(items) | ExprKind::Set(items) => {
            for item in items {
                collect_hooks_in_expr(item, out);
            }
        }
        ExprKind::Dict { items } => {
            for item in items {
                match item {
                    pyths_syntax::ast::DictItem::KeyValue { key, value } => {
                        collect_hooks_in_expr(key, out);
                        collect_hooks_in_expr(value, out);
                    }
                    pyths_syntax::ast::DictItem::Spread(e) => collect_hooks_in_expr(e, out),
                }
            }
        }
        ExprKind::ListComp { elt, generators }
        | ExprKind::SetComp { elt, generators }
        | ExprKind::GeneratorExp { elt, generators } => {
            collect_hooks_in_expr(elt, out);
            for g in generators {
                collect_hooks_in_expr(&g.iter, out);
                for if_clause in &g.ifs {
                    collect_hooks_in_expr(if_clause, out);
                }
            }
        }
        ExprKind::Starred(e) | ExprKind::Await(e) | ExprKind::YieldFrom(e) => {
            collect_hooks_in_expr(e, out);
        }
        ExprKind::Yield(Some(e)) => collect_hooks_in_expr(e, out),
        ExprKind::FString { parts } => {
            for part in parts {
                if let pyths_syntax::ast::FStringPart::Expr(e) = part {
                    collect_hooks_in_expr(e, out);
                }
            }
        }
        _ => {}
    }
}

fn escape_template_literal(s: &str) -> String {
    // #95: values are decoded — escape control chars for the emitted
    // template literal. A raw `\r` would be normalized to `\n` by the JS
    // parser (line-terminator normalization), so it MUST be escaped;
    // `\n`/`\t` are escaped for readability.
    let mut out = String::with_capacity(s.len());
    let chars: Vec<char> = s.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        let ch = chars[i];
        match ch {
            '\\' => out.push_str("\\\\"),
            '`' => out.push_str("\\`"),
            '$' if chars.get(i + 1) == Some(&'{') => {
                out.push_str("\\${");
                i += 1;
            }
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            '\u{2028}' => out.push_str("\\u2028"),
            '\u{2029}' => out.push_str("\\u2029"),
            c if (c as u32) < 0x20 || c as u32 == 0x7f => {
                out.push_str(&format!("\\x{:02x}", c as u32));
            }
            c => out.push(c),
        }
        i += 1;
    }
    out
}

/// Format an f64 as a clean number string (no trailing .0 for integers).
fn format_f64(v: f64) -> String {
    if v == v.trunc() && v.abs() < 1e15 {
        format!("{}", v as i64)
    } else {
        format!("{}", v)
    }
}

/// Check if a function body contains yield or yield from expressions.
fn body_contains_yield(body: &[Stmt]) -> bool {
    for stmt in body {
        if stmt_contains_yield(stmt) {
            return true;
        }
    }
    false
}

fn stmt_contains_yield(stmt: &Stmt) -> bool {
    match &stmt.kind {
        StmtKind::Expr(e) => expr_contains_yield(e),
        StmtKind::Assign { value, .. } => expr_contains_yield(value),
        StmtKind::AugAssign { value, .. } => expr_contains_yield(value),
        StmtKind::Return(Some(e)) => expr_contains_yield(e),
        StmtKind::If {
            test,
            body,
            elif_clauses,
            else_body,
        } => {
            expr_contains_yield(test)
                || body_contains_yield(body)
                || elif_clauses
                    .iter()
                    .any(|(c, b)| expr_contains_yield(c) || body_contains_yield(b))
                || else_body.as_ref().is_some_and(|b| body_contains_yield(b))
        }
        StmtKind::While { body, .. } => body_contains_yield(body),
        StmtKind::For { body, .. } => body_contains_yield(body),
        StmtKind::Try {
            body,
            handlers,
            else_body,
            finally_body,
        } => {
            body_contains_yield(body)
                || handlers.iter().any(|h| body_contains_yield(&h.body))
                || else_body.as_ref().is_some_and(|b| body_contains_yield(b))
                || finally_body
                    .as_ref()
                    .is_some_and(|b| body_contains_yield(b))
        }
        StmtKind::With { body, .. } => body_contains_yield(body),
        // Don't descend into nested function defs — they have their own generator status
        StmtKind::FuncDef { .. } | StmtKind::ClassDef { .. } => false,
        _ => false,
    }
}

fn expr_contains_yield(expr: &Expr) -> bool {
    matches!(&expr.kind, ExprKind::Yield(_) | ExprKind::YieldFrom(_))
}

/// Check if a module path is a React or Next.js module where
/// snake_case→camelCase transforms should be applied to imports.
/// Known Python standard library modules that have PythScribe runtime equivalents.
const STDLIB_MODULES: &[&str] = &[
    "math",
    "json",
    "itertools",
    "functools",
    "collections",
    "random",
    "datetime",
    "re",
    "decimal",
    "fractions",
    "operator",
    "copy",
    "string",
    "heapq",
    "bisect",
    "sys",
];

/// Known PythScribe web modules (pyths.fetch, pyths.storage, pyths.router)
/// Note: pyths.dom maps to pyths-runtime/dom (top-level, not web/)
const WEB_MODULES: &[&str] = &["fetch", "storage", "router"];

/// Resolve a Python module name to its JS import path.
/// Standard library modules are mapped to `pyths-runtime/stdlib/<name>`.
/// Web modules (pyths.dom etc.) are mapped to `pyths-runtime/web/<name>`.
/// Other modules are passed through with `.` replaced by `/`.
/// Known npm package mappings for Pythonic module names.
/// These translate underscore/dot syntax to actual npm package names.
///
/// Most kebab-case packages don't need an entry here — the default
/// kebab-fallback handles `foo_bar` → `foo-bar` automatically. This
/// table is for **irregular** mappings: scoped packages (`@scope/x`),
/// dot-paths into sub-modules, or names that don't kebab cleanly.
const NPM_MODULE_MAPPINGS: &[(&str, &str)] = &[
    // React core + Redux
    ("react_redux", "react-redux"),
    ("react_dom", "react-dom"),
    ("react_dom.client", "react-dom/client"),
    ("react_dom.server", "react-dom/server"),
    ("reduxjs.toolkit", "@reduxjs/toolkit"),
    // Routing
    ("react_router", "react-router"),
    ("react_router_dom", "react-router-dom"),
    // Forms / motion / state / data
    ("react_hook_form", "react-hook-form"),
    ("framer_motion", "framer-motion"),
    ("mobx_react", "mobx-react"),
    ("mobx_react_lite", "mobx-react-lite"),
    // Icon / asset libraries with React bindings
    ("react_icons", "react-icons"),
    ("lucide_react", "lucide-react"),
    // i18n
    ("react_intl", "react-intl"),
    ("react_i18next", "react-i18next"),
    ("i18next", "i18next"),
    // Markdown / content
    ("react_markdown", "react-markdown"),
    ("react_helmet", "react-helmet"),
    ("react_helmet_async", "react-helmet-async"),
    // Drag-and-drop
    ("react_dnd", "react-dnd"),
    ("react_dnd_html5_backend", "react-dnd-html5-backend"),
    ("react_beautiful_dnd", "react-beautiful-dnd"),
    // Utility
    ("date_fns", "date-fns"),
    ("date_fns.locale", "date-fns/locale"),
    ("classnames", "classnames"),
    ("clsx", "clsx"),
    ("tailwind_merge", "tailwind-merge"),
    // Component libraries with non-scoped names (most others are
    // `@scope/...` and routed through the `at_<org>.<pkg>` convention).
    ("react_table", "react-table"),
    ("react_select", "react-select"),
    ("react_window", "react-window"),
    ("react_virtual", "react-virtual"),
    ("react_aria", "react-aria"),
    ("react_use", "react-use"),
];

/// Whether `name` is a Python builtin exception class that the runtime
/// (`pyths-runtime`) exports and that should be auto-imported when used in a
/// `raise X(...)` statement. Mirrors how `pyLen` / `pyBool` / etc. auto-import
/// when their corresponding builtins are used.
/// If `stmt` is a `super().__init__(args)` expression statement, return its
/// args so the constructor can hoist it to a leading `super(args)` call.
fn super_init_args(stmt: &Stmt) -> Option<&[Expr]> {
    let StmtKind::Expr(expr) = &stmt.kind else {
        return None;
    };
    let ExprKind::Call { func, args, .. } = &expr.kind else {
        return None;
    };
    let ExprKind::Attribute { value, attr, .. } = &func.kind else {
        return None;
    };
    if attr != "__init__" {
        return None;
    }
    let ExprKind::Call { func: inner, .. } = &value.kind else {
        return None;
    };
    match &inner.kind {
        ExprKind::Name(n) if n == "super" => Some(args),
        _ => None,
    }
}

/// Largest exact integer in a JS `Number` (2⁵³ − 1).
const SAFE_INT_MAX: i64 = 9_007_199_254_740_991;

/// Interval arithmetic for the bounded-int fast path. Given the intervals
/// of two operands, return the result interval for `+ - *` (saturating to
/// `None` on i64 overflow, which conservatively forces the helper path).
fn combine_int_bound(l: (i64, i64), op: BinOp, r: (i64, i64)) -> Option<(i64, i64)> {
    let (la, lb) = l;
    let (ra, rb) = r;
    match op {
        BinOp::Add => Some((la.checked_add(ra)?, lb.checked_add(rb)?)),
        BinOp::Sub => Some((la.checked_sub(rb)?, lb.checked_sub(ra)?)),
        BinOp::Mul => {
            let p1 = la.checked_mul(ra)?;
            let p2 = la.checked_mul(rb)?;
            let p3 = lb.checked_mul(ra)?;
            let p4 = lb.checked_mul(rb)?;
            Some((p1.min(p2).min(p3).min(p4), p1.max(p2).max(p3).max(p4)))
        }
        _ => None,
    }
}

/// The set of builtin-exception NAMES that an `except <name>:` clause must
/// catch — the class itself plus every subclass, mirroring the CPython
/// hierarchy the package runtime implements (KeyError/IndexError → LookupError;
/// ZeroDivisionError/OverflowError → ArithmeticError; NotImplementedError →
/// RuntimeError; UnboundLocalError → NameError).
///
/// This is the *drift fix* for exception-hierarchy matching under `pyths run`:
/// the inline runtime helpers throw name-tagged plain `Error`s (`e.name =
/// "KeyError"`), NOT real class instances, so `__exc instanceof LookupError`
/// never matched a runtime-raised KeyError inline (it did under `compile`,
/// where the package throws a real `KeyError extends LookupError`). Matching by
/// the descendant-name set makes `except LookupError:` catch a KeyError under
/// BOTH paths. The `instanceof` leg is kept (additive) so real class instances
/// — user `raise`d builtins, `compile`-path throws — still match as before.
fn builtin_exception_descendants(name: &str) -> &'static [&'static str] {
    match name {
        "LookupError" => &["LookupError", "IndexError", "KeyError"],
        "ArithmeticError" => &["ArithmeticError", "ZeroDivisionError", "OverflowError"],
        "RuntimeError" => &["RuntimeError", "NotImplementedError"],
        "NameError" => &["NameError", "UnboundLocalError"],
        // Leaf classes (and the leaf-like bases we treat as leaves): only the
        // name itself. `Exception`/`BaseException` never reach here — they are
        // handled as unconditional catch-alls by the caller.
        "ValueError" => &["ValueError"],
        "TypeError" => &["TypeError"],
        "IndexError" => &["IndexError"],
        "KeyError" => &["KeyError"],
        "AttributeError" => &["AttributeError"],
        "StopIteration" => &["StopIteration"],
        "StopAsyncIteration" => &["StopAsyncIteration"],
        "ZeroDivisionError" => &["ZeroDivisionError"],
        "OverflowError" => &["OverflowError"],
        "NotImplementedError" => &["NotImplementedError"],
        "UnboundLocalError" => &["UnboundLocalError"],
        _ => &[],
    }
}

fn is_builtin_exception(name: &str) -> bool {
    matches!(
        name,
        "Exception"
            | "ValueError"
            | "IndexError"
            | "KeyError"
            | "AttributeError"
            | "StopIteration"
            | "ZeroDivisionError"
            // Batch G: TypeError (thrown by pyOrd et al. as a name-tagged
            // Error) and OverflowError (pyInt on float('inf')) need
            // name-based except matching too.
            | "TypeError"
            | "OverflowError"
            // Round-4 sweep: the runtime grew CPython's hierarchy classes
            // (LookupError/ArithmeticError bases, RuntimeError,
            // NotImplementedError, StopAsyncIteration) — raise/except sites
            // need their auto-imports too.
            | "RuntimeError"
            | "NotImplementedError"
            | "LookupError"
            | "ArithmeticError"
            | "StopAsyncIteration"
            // PBT-2: zero-iteration for-loop target reads raise these; the
            // runtime grew the classes, so raise/except sites auto-import.
            | "NameError"
            | "UnboundLocalError"
    )
}

fn resolve_module_path(module: &str) -> String {
    // Round-4 sweep: `import asyncio` / `from asyncio import ...` map to
    // the runtime's Promise-backed asyncio shim — the same place
    // `pyths.asyncio` resolves. Previously fell through to the bare
    // "asyncio" specifier, which nothing can resolve at runtime.
    if module == "asyncio" {
        return "pyths-runtime/asyncio".to_string();
    }
    // Check for exact stdlib match (e.g., "math", "json")
    if STDLIB_MODULES.contains(&module) {
        return format!("pyths-runtime/stdlib/{}", module);
    }
    // Check for pyths.* web module (e.g., "pyths.dom" → "pyths-runtime/dom")
    if let Some(submod) = module.strip_prefix("pyths.") {
        if WEB_MODULES.contains(&submod) {
            return format!("pyths-runtime/web/{}", submod);
        }
        // `pyths.<stdlib>` (e.g. pyths.datetime) previously fell through to
        // `pyths-runtime/datetime` — a path with no export-map entry, so the
        // import failed at bundle/run time (found by the Twitter clone).
        // Alias it to the same place the bare stdlib name resolves.
        if STDLIB_MODULES.contains(&submod) {
            return format!("pyths-runtime/stdlib/{}", submod);
        }
        // pyths.X.Y → pyths-runtime/X/Y for any sub-module
        return format!("pyths-runtime/{}", submod.replace('.', "/"));
    }
    // Check for known npm package name mappings (e.g., react_redux → react-redux)
    for &(py_name, npm_name) in NPM_MODULE_MAPPINGS {
        if module == py_name {
            return npm_name.to_string();
        }
    }
    // Step 8: scoped npm packages via `at_<org>.<pkg>` convention.
    // Pythonic identifiers can't carry `@` or `-`, so we use the `at_` prefix
    // for scoped npm packages:
    //   at_my_org.pkg        → @my-org/pkg
    //   at_org.scope.deep    → @org/scope/deep
    if let Some(rest) = module.strip_prefix("at_") {
        let parts: Vec<&str> = rest.split('.').collect();
        if parts.len() >= 2 {
            let org = parts[0].replace('_', "-");
            let pkg_path = parts[1..].join("/").replace('_', "-");
            return format!("@{}/{}", org, pkg_path);
        }
    }
    // Default fallback: treat as an npm bare specifier, kebab-casing
    // the package-name segment so `from foo_bar import x` becomes
    // `import { x } from "foo-bar"` — covers the long tail of npm
    // packages without per-package mapping entries.
    //
    // Convention:
    //   foo_bar              → foo-bar               (top-level npm package)
    //   foo_bar.sub_path     → foo-bar/sub-path      (sub-path import)
    //   foo_bar.sub.deep     → foo-bar/sub/deep
    //
    // Sub-path segments are kebab'd too because the npm convention is
    // overwhelmingly kebab-case. Users who actually have a local
    // snake_case module file should either use a Python relative
    // import (`from .my_local import x`) or rename it; the JS ecosystem
    // expects kebab anyway.
    //
    // Override: add the explicit mapping to NPM_MODULE_MAPPINGS for
    // any package that doesn't follow the kebab convention (rare).
    let parts: Vec<&str> = module.split('.').collect();
    parts
        .iter()
        .map(|p| p.replace('_', "-"))
        .collect::<Vec<_>>()
        .join("/")
}

/// Whether a Python-source module identifier should be treated as a
/// React-ecosystem package — meaning import names get snake→camel'd
/// and JSX prop conventions apply. Covers React core, Next.js, Redux,
/// and the top tier of community libraries (state, routing, forms,
/// motion). Scoped packages (`@tanstack/...`, `@xstate/...`, `@react-spring/...`,
/// `@hookform/...`) are recognized via the `at_<org>.<pkg>` Pythonic form.
///
/// The list is intentionally curated, not generic — for arbitrary npm
/// packages where the conventions don't apply, users should write
/// imports with explicit JS-style names (`from at_some.lib import x`)
/// and the resolver will pass them through unchanged.
fn is_react_or_next_module(module: &str) -> bool {
    matches!(
        module,
        // React core + Redux. NOTE: this check runs on the RAW Python module
        // name (react_dom), before the react_dom -> "react-dom" path mapping,
        // so the snake forms must be listed; the kebab forms are kept
        // defensively for callers that pass the mapped name. (Launch-survey A1:
        // only the kebab forms were listed, so `from react_dom import
        // create_portal` emitted an unconverted import binding while the call
        // site emitted createPortal(...) — a guaranteed ReferenceError.)
        "react" | "react_dom" | "react_dom.client" | "react_dom.server"
            | "react-dom" | "react-dom/client" | "react-dom/server"
            | "react_redux" | "reduxjs.toolkit"
            | "pyths.react"
        // State management
            | "zustand" | "jotai" | "valtio" | "recoil" | "xstate"
        // Data fetching
            | "swr"
        // Routing
            | "react_router" | "react_router_dom"
        // Forms
            | "react_hook_form"
        // Motion / animation
            | "framer_motion" | "motion.react"
        // MobX
            | "mobx_react" | "mobx_react_lite"
        // Icon / asset libraries with React bindings
            | "react_icons" | "lucide_react"
        // i18n
            | "react_intl" | "react_i18next"
        // Drag-and-drop
            | "react_dnd" | "react_beautiful_dnd"
        // Component libraries (non-scoped)
            | "react_select" | "react_table" | "react_window"
            | "react_aria" | "react_use" | "react_helmet" | "react_helmet_async"
            | "react_markdown"
    ) || module.starts_with("next/")
        || module.starts_with("next.")
        // Scoped packages with multiple sub-paths.
        || module.starts_with("at_tanstack.")        // @tanstack/{react-query,react-table,...}
        || module.starts_with("at_xstate.")          // @xstate/react
        || module.starts_with("at_react_spring.")    // @react-spring/web, @react-spring/three
        || module.starts_with("at_hookform.")        // @hookform/resolvers
        // Component libraries (scoped)
        || module.starts_with("at_mantine.")         // @mantine/core, @mantine/hooks, ...
        || module.starts_with("at_chakra_ui.")       // @chakra-ui/react, @chakra-ui/icons, ...
        || module.starts_with("at_headlessui.")      // @headlessui/react
        || module.starts_with("at_radix_ui.")        // @radix-ui/react-*
        || module.starts_with("at_heroicons.")       // @heroicons/react/24/outline, ...
        || module.starts_with("at_emotion.")         // @emotion/react, @emotion/styled
        || module.starts_with("at_floating_ui.")     // @floating-ui/react, ...
        || module.starts_with("at_dnd_kit.")         // @dnd-kit/core, @dnd-kit/sortable
        || module.starts_with("at_storybook.")       // @storybook/react, ...
        || module.starts_with("at_testing_library.") // @testing-library/react, ...
}

#[cfg(test)]
mod tests {
    use super::*;

    fn compile(source: &str) -> String {
        let module = pyths_parser::parse(source).expect("Parse failed");
        let mut gen = JsCodegen::new();
        gen.emit_module(&module);
        gen.finish()
    }

    fn compile_inline(source: &str) -> String {
        let module = pyths_parser::parse(source).expect("Parse failed");
        let mut gen = JsCodegen::new_inline();
        gen.emit_module(&module);
        gen.finish()
    }

    // #241: a bool literal in `==`/`!=` must route through pyEq, not the `===`
    // fast path (`True === 1` is false in JS but Python `True == 1` is true).
    #[test]
    fn test_bool_literal_equality_uses_pyeq() {
        let js = compile("x = (True == 1)");
        assert!(js.contains("pyEq("), "bool==int should use pyEq:\n{js}");
        // a plain int==int keeps the === fast path
        let ji = compile("x = (2 == 3)");
        assert!(
            ji.contains("2 === 3") && !ji.contains("pyEq"),
            "int==int should stay ===:\n{ji}"
        );
    }

    // #239: `for k in d` where `d` is an untyped (Unknown) iterable — e.g. a
    // dict PARAMETER — must iterate keys (Python) and not crash on a plain
    // object. Routes through pyForIter; typed lists/ranges keep the raw path.
    #[test]
    fn test_for_over_unknown_iterable_uses_pyforiter() {
        let jd = compile("def f(d):\n    for k in d:\n        print(k)");
        assert!(
            jd.contains("of pyForIter(d)"),
            "dict-param loop not wrapped:\n{jd}"
        );
        // a provably-list iterable keeps the raw fast path
        let jl = compile("xs = [1, 2, 3]\nfor x in xs:\n    print(x)");
        assert!(
            jl.contains("of xs)") && !jl.contains("pyForIter(xs)"),
            "list loop should stay raw:\n{jl}"
        );
        // range keeps its own path
        let jr = compile("for i in range(3):\n    print(i)");
        assert!(
            !jr.contains("pyForIter"),
            "range loop should not be wrapped:\n{jr}"
        );
    }

    // #237: `.isupper()`/`.islower()` on a COMPLEX receiver (`s[i].isupper()`)
    // must use the runtime helper — the inline form references the receiver 3×
    // (needs a simple receiver) and otherwise fell through to a verbatim
    // `.isupper()` that JS strings do not have.
    #[test]
    fn test_isupper_islower_on_complex_receiver_use_runtime() {
        let jc = compile("s = \"Ab\"\nx = s[0].isupper()");
        assert!(
            jc.contains("pyStrIsupper("),
            "complex isupper not lowered to helper:\n{jc}"
        );
        let jl = compile("s = \"Ab\"\nx = s[1:].islower()");
        assert!(
            jl.contains("pyStrIslower("),
            "complex islower not lowered to helper:\n{jl}"
        );
        // simple receiver keeps the inline fast path (no helper)
        let js = compile("s = \"Ab\"\nx = s.isupper()");
        assert!(
            !js.contains("pyStrIsupper"),
            "simple isupper should stay inline:\n{js}"
        );
    }

    // #251: `type(x) == int` compares the runtime type's __name__ (the bare
    // builtin type name otherwise lowered to its constructor → always False).
    #[test]
    fn test_type_identity_comparison() {
        let js = compile("x = (type(5) == int)");
        assert!(
            js.contains("pyType(5).__name__ === \"int\""),
            "type== not lowered:\n{js}"
        );
        let jr = compile("x = (str == type(v))");
        assert!(
            jr.contains("pyType(v).__name__ === \"str\""),
            "reversed type== not lowered:\n{jr}"
        );
        let jn = compile("x = (type(5) != list)");
        assert!(
            jn.contains("pyType(5).__name__ !== \"list\""),
            "type!= not lowered:\n{jn}"
        );
    }

    // #244: `d = dict()` (constructor) written with a dynamic key is Map-backed
    // too, not just the `{}` literal (#230).
    #[test]
    fn test_dict_ctor_dynamic_key_forces_pydict() {
        let jd = compile("d = dict()\nfor k in ks:\n    d[k] = 1");
        assert!(
            jd.contains("new PyDict"),
            "dict() with dynamic key not Map-backed:\n{jd}"
        );
        let jr = compile("d = dict()\nd[\"a\"] = 1");
        assert!(
            !jr.contains("new PyDict"),
            "string-key dict() should stay plain:\n{jr}"
        );
    }

    // #230: `d = {}` written with a DYNAMIC key must be a Map-backed PyDict —
    // a plain object stringifies non-string keys (`d[num]=1` → `{"1":...}`).
    // A string-literal write key keeps the plain-object record fast path.
    #[test]
    fn test_dynamic_key_dict_forces_pydict() {
        let jd = compile("d = {}\nfor num in [1,2,3]:\n    d[num] = 1");
        assert!(
            jd.contains("new PyDict"),
            "dynamic-key dict not Map-backed:\n{jd}"
        );
        // string-literal keys stay a plain object (records)
        let jr = compile("d = {}\nd[\"a\"] = 1\nd[\"b\"] = 2");
        assert!(
            !jr.contains("new PyDict"),
            "string-record dict should stay plain:\n{jr}"
        );
    }

    // #227: a Float-typed variable keeps its `.0` when printed — `x = 2.0` then
    // `print(x)` must format as a float (a whole float and an int are the same
    // JS number at runtime, so the type must be tracked statically).
    #[test]
    fn test_float_typed_variable_formats_as_float_on_print() {
        let js = compile("x = 2.0\nprint(x)");
        assert!(
            js.contains("pyFormatFloat(x)"),
            "float var not float-formatted:\n{js}"
        );
        // an int variable is NOT wrapped
        let ji = compile("y = 5\nprint(y)");
        assert!(
            !ji.contains("pyFormatFloat"),
            "int var must not be float-formatted:\n{ji}"
        );
        // tuple unpack propagates per-element float types
        let jt = compile("a, b = -1.0, 1.0\nprint(a, b)");
        assert!(
            jt.contains("pyFormatFloat(a)") && jt.contains("pyFormatFloat(b)"),
            "tuple-unpack floats not tracked:\n{jt}"
        );
    }

    // #223: copy/string are recognized stdlib modules — `import copy` resolves
    // to the runtime package, not an npm lookup.
    #[test]
    fn test_copy_string_are_stdlib_modules() {
        let jc = compile("import copy\nx = copy.deepcopy(a)");
        assert!(
            jc.contains("pyths-runtime/stdlib/copy"),
            "copy not stdlib:\n{jc}"
        );
        let js = compile("import string\nx = string.ascii_lowercase");
        assert!(
            js.contains("pyths-runtime/stdlib/string"),
            "string not stdlib:\n{js}"
        );
    }

    // #224: a Capitalized member of a stdlib module namespace is a constructor.
    #[test]
    fn test_module_qualified_class_gets_new() {
        let js = compile("import collections\nc = collections.Counter(xs)");
        assert!(
            js.contains("new collections.Counter("),
            "should emit new:\n{js}"
        );
    }

    // #225: eval/exec/compile are unsupported with a clear diagnostic (not a
    // cryptic `eval$ is not defined`).
    #[test]
    fn test_eval_is_unsupported_with_diagnostic() {
        let js = compile("x = eval(\"1+2\")");
        assert!(
            js.contains("not supported"),
            "eval should emit a diagnostic:\n{js}"
        );
        assert!(
            !js.contains("eval$"),
            "must not emit the reserved-word rename:\n{js}"
        );
    }

    // #217: `list()` with no args must be `[]`, not `Array.from()` (throws).
    #[test]
    fn test_empty_list_constructor() {
        let js = compile("xs = list()\nxs.append(1)");
        assert!(js.contains("let xs = []"), "list() should be []:\n{js}");
        assert!(
            !js.contains("Array.from()"),
            "must not emit Array.from():\n{js}"
        );
    }

    // #219: slice assignment routes to pySetSlice, never a bare invalid LHS.
    #[test]
    fn test_slice_assignment_uses_setslice() {
        let js = compile("l = [1,2,3]\nl[::2] = [9, 8]");
        assert!(
            js.contains("pySetSlice("),
            "slice assign should call pySetSlice:\n{js}"
        );
        assert!(
            !js.contains("pySlice(l") || !js.contains(") = "),
            "no invalid slice LHS:\n{js}"
        );
    }

    // #221: a call on a stdlib module namespace (`re.split`) is a module
    // function, NOT the string `.split` method — must be emitted verbatim.
    #[test]
    fn test_stdlib_module_call_not_string_method() {
        let js = compile("import re\nparts = re.split(r\"[.]\", s)");
        assert!(
            js.contains("re.split("),
            "re.split should be a module call:\n{js}"
        );
        assert!(
            !js.contains("pyStrSplit"),
            "must not lower to pyStrSplit:\n{js}"
        );
    }

    // #211: Python truthiness of empty collections. `not x`, `if x:`, `while x:`
    // on a list/dict/set/tuple or an Unknown (unannotated param) must go through
    // pyBool — an empty list is falsy in Python but `![]`/`[]` is truthy in JS.
    // Scalars (int/bool/str/None/float) keep the bare fast path.
    #[test]
    fn test_truthiness_wraps_collection_and_unknown_not_scalar() {
        // `not` on an unannotated param (Unknown) → pyBool-wrapped.
        let js = compile("def f(xs):\n    return not xs");
        assert!(
            js.contains("!pyBool("),
            "`not xs` (Unknown) not wrapped:\n{}",
            js
        );

        // `while` on an unannotated param (Unknown) → pyBool-wrapped (this is the
        // /70 over-run: `while lst:` must stop on the empty tail).
        let jw = compile("def f(lst):\n    while lst:\n        lst.pop()");
        assert!(
            jw.contains("while (pyBool("),
            "`while lst` (Unknown) not wrapped:\n{}",
            jw
        );

        // `if` on a list literal → wrapped.
        let jl = compile("xs = []\nif not xs:\n    print(1)");
        assert!(jl.contains("!pyBool("), "`not []` not wrapped:\n{}", jl);

        // Scalars keep the bare fast path — no pyBool for an int param.
        let js2 = compile("def f(n: int):\n    return not n");
        assert!(
            js2.contains("(!n)") && !js2.contains("pyBool"),
            "`not n` (int) should stay bare:\n{}",
            js2,
        );
    }

    // #206: bin/hex/oct had no builtin mapping and crashed at runtime with
    // `ReferenceError: bin is not defined`. They must lower to runtime helpers.
    #[test]
    fn test_bin_hex_oct_map_to_runtime_helpers() {
        for (src, helper) in [
            ("print(bin(5))", "pyBin"),
            ("print(hex(255))", "pyHex"),
            ("print(oct(8))", "pyOct"),
        ] {
            let js = compile(src);
            assert!(
                js.contains(&format!("{}(", helper)),
                "{} not lowered to {}:\n{}",
                src,
                helper,
                js,
            );
        }
    }

    // #199: a name declared `global` must NOT be re-declared with `let` inside
    // the function — that shadows the module binding (mutation lost) and reads
    // it before init (TDZ → NaN). Assignments must rebind the module variable.
    #[test]
    fn test_global_assignment_rebinds_module_no_shadowing_let() {
        let js = compile("n = 1\ndef inc():\n    global n\n    n = n + 1\n    return n\ninc()");
        // The module binding is declared once at top level...
        assert!(
            js.contains("export let n = 1;"),
            "module `n` missing:\n{}",
            js
        );
        // ...and `let n` appears exactly once (that module decl) — the function
        // assigns `n` bare, with no shadowing `let n`.
        assert_eq!(
            js.matches("let n").count(),
            1,
            "global `n` was shadowed by a local `let`:\n{}",
            js,
        );
    }

    // #199: the same must hold when the global is assigned only inside a nested
    // block — the up-front global scan must beat the block-hoist pass.
    #[test]
    fn test_global_assigned_in_nested_block_not_shadowed() {
        let js = compile(
            "n = 0\ndef f(c):\n    global n\n    if c:\n        n = n + 5\n    return n\nf(True)",
        );
        // Exactly one `let n` — the module decl; no shadowing local even though
        // the only assignment is inside a nested `if` block.
        assert_eq!(
            js.matches("let n").count(),
            1,
            "global `n` (nested-block assign) was shadowed by a local `let`:\n{}",
            js,
        );
    }

    // #199: `nonlocal` has the analogous closure-scope bug — the inner function
    // must rebind the enclosing `c`, not declare a fresh local.
    #[test]
    fn test_nonlocal_assignment_rebinds_enclosing_no_shadowing_let() {
        let js = compile(
            "def outer():\n    c = 0\n    def inc():\n        nonlocal c\n        c = c + 1\n        return c\n    return inc()",
        );
        // `let c` appears exactly once — the enclosing declaration, not a second
        // shadowing one inside `inc`.
        assert_eq!(
            js.matches("let c").count(),
            1,
            "nonlocal `c` was shadowed by a second `let`:\n{}",
            js,
        );
    }

    // #201: `import` inside a function body must be hoisted to module scope —
    // ES `import` is legal only at the top level, so emitting it inline in the
    // function produced a Node `SyntaxError`. The local name binding is
    // preserved (module scope is a superset of the function scope).
    #[test]
    fn test_function_local_import_hoisted_to_module_scope() {
        let js =
            compile("def f():\n    import random\n    return random.randint(0, 5)\nprint(f())");
        // The import is emitted, but at the module top — not inside `f`.
        assert!(
            js.contains("import * as random from \"pyths-runtime/stdlib/random\";"),
            "import missing:\n{}",
            js,
        );
        let import_at = js.find("import * as random").expect("no random import");
        let fn_at = js.find("function f(").expect("no function f");
        assert!(
            import_at < fn_at,
            "function-local import was not hoisted above `function f`:\n{}",
            js,
        );
    }

    // #201: a top-level import of the same module plus a function-local import
    // of it must not emit two conflicting `import * as x` declarations.
    #[test]
    fn test_function_local_import_dedups_against_top_level() {
        let js = compile(
            "import random\ndef f():\n    import random\n    return random.randint(0, 5)\nprint(f())",
        );
        assert_eq!(
            js.matches("import * as random").count(),
            1,
            "duplicate `import * as random`:\n{}",
            js,
        );
    }

    // #170: `pyths run` inline runtime must cover every Runtime-lowered
    // helper, not just the hand-written table — missing ones are extracted
    // from the embedded package runtime with their dependencies.
    #[test]
    fn test_inline_runtime_extracts_str_helpers() {
        let js = compile_inline("ys = [\"a\", \"b\"]\nprint(\",\".join(ys))");
        assert!(
            js.contains("function pyStrJoin("),
            "pyStrJoin missing:\n{}",
            js
        );
        assert!(
            !js.contains("import {"),
            "inline output must be self-contained"
        );
    }

    #[test]
    fn test_inline_runtime_extracts_async_iter() {
        let js = compile_inline("async def f(gen):\n    async for x in gen:\n        print(x)\n");
        assert!(
            js.contains("function __pyAsyncIter("),
            "__pyAsyncIter missing:\n{}",
            js
        );
    }

    #[test]
    fn test_inline_runtime_extraction_pulls_dependencies() {
        // pyStrCenter etc. may lean on private top-level helpers of the
        // package runtime; the fallback must pull those too, and mark the
        // extracted region.
        let js = compile_inline("print(\"x\".center(5, \"-\"))");
        assert!(js.contains("function pyStrCenter("), "pyStrCenter missing");
        assert!(js.contains("Extracted from package runtime"));
    }

    // PBT-1: pySlice's hand-written inline copy drifted from the package
    // runtime (no out-of-range clamping). It was removed; `pyths run` must
    // now pull the canonical definition via the #170 extraction fallback.
    #[test]
    fn test_inline_runtime_extracts_py_slice() {
        let js = compile_inline("print([1, 2, 3][10:100])");
        assert!(js.contains("function pySlice("), "pySlice missing:\n{}", js);
        assert!(
            js.contains("Extracted from package runtime"),
            "pySlice should come from the extraction fallback"
        );
        // The extracted body is the clamped one (CPython slice.indices).
        assert!(
            js.contains("const upper = step < 0 ? len - 1 : len;"),
            "extracted pySlice lacks CPython clamping:\n{}",
            js
        );
    }

    // #329: repr() escapes non-printable Cf/Zs/Cc/... code points (NBSP,
    // U+3000, soft hyphen, ...) as \xNN/\uNNNN/\UNNNNNNNN — the inline pyRepr
    // twin carries the same __cpNonPrintable range table.
    #[test]
    fn test_inline_repr_escapes_nonprintable() {
        let js = compile_inline("print(repr(chr(0xA0)))");
        assert!(
            js.contains("function __cpNonPrintable("),
            "printability table missing:\n{}",
            js
        );
        assert!(
            js.contains("__cpNonPrintable(cp)"),
            "repr not using printability check:\n{}",
            js
        );
    }

    // #328: `str.center` puts the odd extra pad on the LEFT when both margin
    // and width are odd (`marg & width & 1`). Extracted via #170 fallback.
    #[test]
    fn test_inline_str_center_odd_margin() {
        let js = compile_inline("print('ab'.center(5))");
        assert!(
            js.contains("function pyStrCenter("),
            "pyStrCenter missing:\n{}",
            js
        );
        assert!(
            js.contains("(need & width & 1)"),
            "odd-margin left-bias missing:\n{}",
            js
        );
    }

    // #327: `str.count('')` (empty needle) returns len(s)+1 (the empty
    // substring matches at every gap), not 0. pyCount has no inline twin —
    // it flows through the #170 extraction fallback.
    #[test]
    fn test_inline_str_count_empty() {
        let js = compile_inline("print('abc'.count(''))");
        assert!(js.contains("function pyCount("), "pyCount missing:\n{}", js);
        assert!(
            js.contains("v.length === 0"),
            "empty-needle branch missing:\n{}",
            js
        );
    }

    // #320: a bitwise operator with a non-integer operand must raise TypeError,
    // not leak the JS RangeError from BigInt(0.2). The inline pyBitOr/And/Xor
    // twins carry the __reqBitInt guard; shifts flow through #170 extraction.
    #[test]
    fn test_inline_bitwise_int_guard() {
        let js = compile_inline(
            "ok = 0\ntry:\n    _r = 1 | 0.2\nexcept Exception as e:\n    ok = 1\nprint(ok)",
        );
        assert!(
            js.contains("function __reqBitInt("),
            "bitwise int guard missing:\n{}",
            js
        );
        assert!(
            js.contains("unsupported operand type(s) for"),
            "TypeError message missing:\n{}",
            js
        );
    }

    // #319: float ** overflow raises OverflowError (int ** stays exact BigInt).
    // The codegen passes a float-context flag to pyPow/pyMul/pyAdd when a
    // statically-known float operand is present; the inline twins carry it.
    #[test]
    fn test_inline_float_overflow_flag() {
        let js = compile_inline(
            "ok = 0\ntry:\n    _r = 10.0 ** 400\nexcept Exception as e:\n    ok = 1\nprint(ok)",
        );
        assert!(
            js.contains("pyPow(10, 400, true)"),
            "float-ctx flag missing on pyPow:\n{}",
            js
        );
        assert!(
            js.contains("__reqNum = (x)"),
            "inline __reqNum missing:\n{}",
            js
        );
        assert!(
            js.contains("!Number.isSafeInteger(x)"),
            "inline __isFloat not widened:\n{}",
            js
        );
    }

    // #318: round(float, ndigits) keeps the float type — `round(1234.5678, -2)`
    // must pre-format as `1200.0`, not the int `1200`. Single-arg round stays
    // int. The inline pyRound twin carries the BigInt + extreme-ndigits fix.
    #[test]
    fn test_round_float_ndigits_preformats() {
        let js = compile("print(repr(round(1234.5678, -2)))");
        assert!(
            js.contains("pyFormatFloat("),
            "round(float, n) not float-formatted:\n{}",
            js
        );
        // Single-arg round is an int — no float pre-format.
        let js1 = compile("print(repr(round(2.5)))");
        assert!(
            !js1.contains("pyFormatFloat("),
            "round(x) single-arg should be int:\n{}",
            js1
        );
    }

    #[test]
    fn test_inline_pyround_bigint_and_extreme() {
        let js = compile_inline("print(repr(round(10**30, 0)))\nprint(repr(round(1.5, 400)))");
        assert!(
            js.contains("function pyRound("),
            "inline pyRound missing:\n{}",
            js
        );
        assert!(
            js.contains("function __roundBigNeg("),
            "inline __roundBigNeg missing:\n{}",
            js
        );
        assert!(
            js.contains("typeof x === \"bigint\""),
            "inline pyRound lacks bigint path:\n{}",
            js
        );
    }

    // #322: `None + x` (e.g. a defaulted dict.get feeding arithmetic) must
    // raise TypeError, not coerce to NaN. The inline arithmetic block carries
    // the same None guard the package operators.js does (both runtimes fixed).
    #[test]
    fn test_inline_arith_none_guard_present() {
        let js = compile_inline("v = {}\nprint(repr(v.get(5) + 1))");
        assert!(
            js.contains("function __arithNoneGuard("),
            "None guard missing:\n{}",
            js
        );
        assert!(
            js.contains("unsupported operand type(s) for"),
            "CPython TypeError message missing:\n{}",
            js
        );
        assert!(
            js.contains("__arithNoneGuard(\"+\", a, b)"),
            "pyAdd guard call missing"
        );
    }

    // #321: `del xs[a:b]` slice-delete lowers to pyDelSlice, which must be
    // pulled into the inline runtime via the same #170 extraction fallback
    // (no hand-written twin), clamping OOB bounds like pySlice.
    #[test]
    fn test_inline_runtime_extracts_py_del_slice() {
        let js = compile_inline("v = [1, 2, 3]\ndel v[5:9]\nprint(repr(v))");
        assert!(
            js.contains("pyDelSlice(v, 5, 9, null)"),
            "del-slice call missing:\n{}",
            js
        );
        assert!(
            js.contains("function pyDelSlice("),
            "pyDelSlice def missing:\n{}",
            js
        );
        assert!(
            js.contains("Extracted from package runtime"),
            "pyDelSlice should come from the extraction fallback"
        );
    }

    #[test]
    fn test_package_runtime_slice_index_sanity() {
        let (_slices, by_name) = JsCodegen::package_runtime_slices();
        for name in ["pyStrJoin", "pyStrCenter", "pyStrFormat", "__pyAsyncIter"] {
            assert!(by_name.contains_key(name), "slice index missing {}", name);
        }
    }

    #[test]
    fn test_hello_world() {
        let js = compile("print(\"hello world\")");
        // print lowers to pyPrint (strips BigInt's `n` suffix on output).
        assert!(js.contains("pyPrint(\"hello world\")"));
    }

    #[test]
    fn test_assignment() {
        let js = compile("x = 42");
        assert!(js.contains("let x = 42;"));
    }

    #[test]
    fn test_function() {
        let js = compile("def greet(name):\n    print(name)");
        assert!(js.contains("function greet(name)"));
        assert!(js.contains("pyPrint(name)"));
    }

    #[test]
    fn test_if_else() {
        let js = compile("if x > 0:\n    y = 1\nelse:\n    y = 2");
        assert!(js.contains("if ("));
        assert!(js.contains("} else {"));
    }

    #[test]
    fn test_is_none_uses_loose_null() {
        // B-021: `x is None` / `is not None` must treat a JS `undefined`
        // value (e.g. from `dict.get(missing)`) as None, so emit `== null` /
        // `!= null` (loose), not strict `Object.is(x, null)`.
        let js = compile("a = d.get(\"k\")\nif a is not None:\n    print(a)");
        assert!(
            js.contains("!= null"),
            "is-not-None should emit `!= null`: {js}"
        );
        assert!(
            !js.contains("Object.is"),
            "is-None must not use Object.is: {js}"
        );

        let js2 = compile("if a is None:\n    print(1)");
        assert!(
            js2.contains("== null"),
            "is-None should emit `== null`: {js2}"
        );
    }

    #[test]
    fn test_conditional_first_assignment_is_hoisted() {
        // B-023: a variable first-assigned inside an if/else must be
        // function-scoped (`let` hoisted to the top), not block-scoped.
        let js =
            compile("def f(c):\n    if c:\n        x = 1\n    else:\n        x = 2\n    return x");
        assert!(js.contains("let x;"), "should hoist `let x;`: {js}");
        let after = js.split("let x;").nth(1).unwrap();
        assert!(
            !after.contains("let x"),
            "branches must not re-declare x: {js}"
        );

        // A var first-assigned at the top level is left as inline `let`.
        let js2 = compile("def g():\n    y = 1\n    if y:\n        y = 2\n    return y");
        assert!(
            js2.contains("let y = 1"),
            "top-level first-assign stays inline: {js2}"
        );
    }

    #[test]
    fn test_list_plus_unknown_concats() {
        // B-019 / crit-13: `<unknown> + [x]` must go through pyAdd — it concats
        // (Python list `+`) without JS `+` string-coercing arrays, and unlike a
        // blind spread it preserves tuple-ness and raises TypeError on
        // list+tuple. Only provable list+list uses the native spread fast path.
        let js = compile("def f(xs):\n    return xs + [9]");
        assert!(
            js.contains("pyAdd(xs, [9])"),
            "unknown+list routes via pyAdd: {js}"
        );
        let js_ll = compile("def h():\n    a = [1]\n    b = [2]\n    return a + b");
        assert!(
            js_ll.contains("[...a, ...b]"),
            "list+list keeps spread fast path: {js_ll}"
        );

        // Arithmetic on unknown operands routes through pyAdd (keeps
        // arbitrary-precision ints exact); must NOT spread-concat.
        let js2 = compile("def g(a, b):\n    return a + b");
        assert!(
            js2.contains("pyAdd(a, b)"),
            "numeric + routes via pyAdd: {js2}"
        );
        assert!(!js2.contains("[..."), "numeric + must not spread: {js2}");
    }

    #[test]
    fn test_for_loop() {
        // A loop variable that is never reassigned keeps its per-iteration
        // block-scoped `const` (no output churn from #220). #239: an Unknown
        // iterable (`items` is untyped here) is wrapped in pyForIter so a dict
        // param iterates keys and a plain object doesn't crash.
        let js = compile("for i in items:\n    print(i)");
        assert!(js.contains("for (const i of pyForIter(items))"), "{js}");
    }

    // #220: a for-loop variable REUSED after the loop (as a plain assignment)
    // must not ReferenceError — Python function-scopes it, so it is hoisted to
    // a single `let` that both the loop and the reassignment bind.
    #[test]
    fn test_for_loop_var_reused_after_loop() {
        let js = compile("def f(xs):\n    for i in xs:\n        pass\n    i = 0\n    return i");
        // The reused loop var is hoisted to a single function-scope `let`
        // (PBT-2: sentinel-initialized, since only loop iterations or the
        // later `i = 0` give it a value); the loop binds that hoisted `i`
        // directly (bare `for (i of ...)`, #269) and the `i = 0` reassignment
        // binds — and un-sentinels — the same one (no ReferenceError).
        assert!(
            js.contains("let i = __UNBOUND;"),
            "reused loop var not hoisted:\n{js}"
        );
        assert!(
            js.contains("i = 0"),
            "reassignment should bind the hoisted let:\n{js}"
        );
    }

    #[test]
    fn test_class() {
        let js = compile("class Dog:\n    def __init__(self, name):\n        self.name = name");
        // Regular classes use the cooperative PyObject model: extend
        // PyObject and emit __init__ as a prototype method (dispatched via
        // the MRO), not the JS constructor.
        assert!(js.contains("class Dog extends PyObject"), "JS: {js}");
        assert!(js.contains("__init__(name)"), "JS: {js}");
        assert!(js.contains("this.name = name"), "JS: {js}");
        assert!(js.contains("__pyClass(Dog, [])"), "JS: {js}");
    }

    #[test]
    fn test_list_comprehension() {
        let js = compile("result = [x * 2 for x in items]");
        assert!(js.contains(".map("));
    }

    #[test]
    fn test_comprehension_over_string_wraps_pyseq() {
        // Pythonic-checks sweep: `[c for c in "ab"]` used to emit
        // `"ab".map(...)` — strings have no .map. Any iterable that is not
        // provably an array must be materialized via pySeq first (also
        // covers generators, whose .map is a lazy Iterator Helper in
        // Node 22+, not an Array).
        let js = compile("result = [c for c in \"ab\"]");
        assert!(js.contains("pySeq(\"ab\").map("), "JS: {js}");
    }

    #[test]
    fn test_comprehension_over_known_list_no_wrap() {
        // Provably-array iterables skip the pySeq wrap (no runtime cost).
        let js = compile("xs = [1, 2]\nresult = [x * 2 for x in xs]");
        assert!(js.contains("xs.map("), "JS: {js}");
        assert!(!js.contains("pySeq"), "JS: {js}");
    }

    #[test]
    fn test_and_or_python_truthiness_on_containers() {
        // #273: `a and b` / `a or b` short-circuit on PYTHON truthiness. A scalar
        // left keeps the raw JS `&&`/`||` (no churn); a container/Unknown left
        // routes through pyAnd/pyOr, so an empty list (JS-truthy but Python-falsy)
        // short-circuits and returns itself instead of evaluating the right side.
        let scalar = compile("x = (a < b) and c");
        assert!(
            scalar.contains(" && "),
            "scalar-left keeps raw &&:\n{scalar}"
        );
        assert!(
            !scalar.contains("pyAnd"),
            "scalar-left must not wrap:\n{scalar}"
        );
        let and_c = compile("xs = []\ny = xs and xs[0]");
        assert!(
            and_c.contains("pyAnd("),
            "container-left routes pyAnd:\n{and_c}"
        );
        let or_c = compile("xs = []\ny = xs or 5");
        assert!(
            or_c.contains("pyOr("),
            "container-left routes pyOr:\n{or_c}"
        );
    }

    #[test]
    fn test_comprehension_over_reassigned_self_wraps_pyseq() {
        // #272: `s = [expr for c in s]` rebinds `s` from a str to a list, but the
        // RHS iterates the OLD `s` (still a str) → must pySeq-wrap. The assignment
        // type is recorded AFTER the value is emitted, so `s` stays Unknown while
        // the comprehension is generated (was: recorded first → `s` looked like a
        // List → bare `s.map(...)` on a string → runtime crash).
        let js = compile("def f(s):\n    s = [1 for c in s]\n    return s");
        assert!(
            js.contains("pySeq(s).map("),
            "self-reassign must pySeq-wrap:\n{js}"
        );
    }

    #[test]
    fn test_comprehension_nested_tuple_target() {
        // Pythonic-checks sweep: `for i, (x, y) in ...` used to emit the
        // inner tuple target as a pyTuple(...) VALUE inside the
        // destructuring pattern — a JS syntax error. Nested targets must
        // recurse through emit_for_target.
        let js = compile("result = [i + x for i, (x, y) in pairs]");
        assert!(js.contains("([i, [x, y]])"), "JS: {js}");
        assert!(!js.contains("pyTuple(x, y)])"), "JS: {js}");
    }

    #[test]
    fn test_fstring() {
        // A4: no-format-spec interpolation routes through pyStr so
        // bool/None/floats/containers print CPython-style instead of via
        // JS's implicit template-literal ToString (e.g. `${true}` -> "true"
        // instead of "True"). pyStr(str) is an identity passthrough, so
        // this doesn't change output for already-correct string values.
        let js = compile("msg = f\"hello {name}\"");
        assert!(js.contains("`hello ${pyStr(name)}`"));
        assert!(js.contains("import { pyStr }") || js.contains("pyStr,") || js.contains(", pyStr"));
    }

    #[test]
    fn test_fstring_bool_none() {
        let js = compile("msg = f\"{True} {None}\"");
        assert!(js.contains("${pyStr(true)}"));
        assert!(js.contains("${pyStr(null)}"));
    }

    #[test]
    fn test_fstring_format_spec_unaffected() {
        // Format-spec interpolation (`{x:.2f}`) must keep working — its
        // lowering already produces a JS expression that evaluates to a
        // string (`pyFixed(x, 2)` direct-emission fast path since #86 —
        // CPython round-half-even, replacing `.toFixed(2)` — or a
        // `pyFormatSpec(...)` runtime-helper call for complex specs).
        // Wrapping that in pyStr is a safe no-op (pyStr(string) is
        // identity) — this test locks in that the fast-path output
        // is still present, i.e. the spec lowering emits the fast path.
        let js = compile("msg = f\"{x:.2f}\"");
        assert!(
            js.contains("pyFixed(x, 2)"),
            "format-spec fast path should emit pyFixed: {js}"
        );
    }

    #[test]
    fn test_print_whole_float_literal_uses_pyformatfloat() {
        // A4: print(1.0) must not lose the `.0` — small ints and whole
        // floats compile to the identical JS number, so the ambiguity is
        // resolved at compile time via static type inference (see the A4
        // note above emit_call's print/str/repr handling) rather than
        // left to pyRepr's runtime number branch to guess.
        let js = compile("print(1.0)");
        assert!(js.contains("pyPrint(pyFormatFloat(1))"), "got: {js}");
    }

    #[test]
    fn test_str_and_repr_whole_float_literal_use_pyformatfloat() {
        let js = compile("x = str(1.0)\ny = repr(2.0)");
        assert!(js.contains("pyFormatFloat(1)"), "got: {js}");
        assert!(js.contains("pyFormatFloat(2)"), "got: {js}");
    }

    #[test]
    fn test_print_mixed_float_and_other_args() {
        // Only the statically-float arg gets pre-formatted; others still
        // flow through pyPrint's normal per-arg pyStr handling.
        let js = compile("print(1.0, \"x\", True)");
        assert!(
            js.contains("pyPrint(pyFormatFloat(1), \"x\", true)"),
            "got: {js}"
        );
    }

    #[test]
    fn test_fstring_whole_float_literal_uses_pyformatfloat() {
        let js = compile("msg = f\"{1.0}\"");
        assert!(js.contains("${pyFormatFloat(1)}"), "got: {js}");
    }

    #[test]
    fn test_negative_whole_float_literal_uses_pyformatfloat() {
        // Regression: `-1.0` parses as UnaryOp(Neg, FloatLiteral(1.0)),
        // not a single FloatLiteral node — infer_type must propagate
        // Float through unary minus/plus or this falls through to the
        // Unknown default and misses the compile-time float fast path
        // (caught by the differential corpus's a4_float_whole_neg_repr).
        let js = compile("print(-1.0)");
        assert!(js.contains("pyFormatFloat("), "got: {js}");
    }

    #[test]
    fn test_repr_of_division_not_treated_as_definitely_float() {
        // Regression (found via the differential corpus during A4 dev):
        // infer_type's `BinOp::Div => Float` rule is unconditional and
        // doesn't know Decimal/Fraction override `__truediv__` to return
        // their own type. `is_definitely_float` must NOT trust that rule
        // — repr(a / b) must still go through the normal pyRepr runtime
        // dispatch (which correctly calls a custom __repr__ when present)
        // rather than being pre-formatted with pyFormatFloat, which would
        // silently coerce a Decimal/Fraction result to a plain float.
        let js = compile("x = repr(Decimal(1) / Decimal(3))");
        assert!(js.contains("pyRepr("), "got: {js}");
        assert!(!js.contains("pyFormatFloat"), "got: {js}");
    }

    #[test]
    fn test_print_plain_int_not_affected_by_float_fix() {
        // Sanity/non-regression: an actual int literal must NOT be routed
        // through pyFormatFloat (would wrongly add `.0`).
        let js = compile("print(1)");
        assert!(js.contains("pyPrint(1)"), "got: {js}");
        assert!(!js.contains("pyFormatFloat"), "got: {js}");
    }

    #[test]
    fn test_tuple_literal_is_marked() {
        // A4 tuple investigation: tuple literals used to compile to a
        // bare JS array indistinguishable from a list. They now route
        // through the pyTuple(...) marker helper so pyRepr/pyStr/pyPrint
        // can tell `(1, 2)` apart from `[1, 2]`.
        // Small int literals (abs <= 2**53-1) stay plain JS numbers, not
        // BigInt (see ExprKind::IntLiteral above) — 1, 2 not 1n, 2n.
        let js = compile("t = (1, 2)");
        assert!(js.contains("pyTuple(1, 2)"), "got: {js}");
    }

    #[test]
    fn test_tuple_unpack_target_not_marked() {
        // Tuple-unpacking assignment targets (`a, b = ...`) are a
        // completely separate codegen path (emit_assign's destructuring-
        // pattern special case; since #84 a `let`-predeclared ASSIGNMENT
        // `([a, b] = ...)` so already-declared names can be re-unpacked,
        // e.g. the swap idiom) — the TARGET must NOT be routed through
        // pyTuple (the RHS value legitimately is).
        let js = compile("a, b = 1, 2");
        assert!(js.contains("([a, b] ="), "got: {js}");
        assert!(!js.contains("pyTuple(a, b)"), "got: {js}");
    }

    #[test]
    fn test_for_target_tuple_not_marked() {
        // `for a, b in items` unpacking target — also a separate path
        // (emit_for_target), must not be routed through pyTuple either.
        let js = compile("for a, b in items:\n    pass");
        assert!(js.contains("for (const [a, b] of"), "got: {js}");
    }

    #[test]
    fn test_floor_division() {
        let js = compile("x = a // b");
        // Routed through pyFloorDiv so b===0 raises ZeroDivisionError.
        assert!(js.contains("pyFloorDiv(a, b)"));
    }

    #[test]
    fn test_none_true_false() {
        let js = compile("x = None\ny = True\nz = False");
        assert!(js.contains("null"));
        assert!(js.contains("true"));
        assert!(js.contains("false"));
    }

    #[test]
    fn test_lambda() {
        let js = compile("f = lambda x: x + 1");
        // `+` routes through pyAdd for arbitrary-precision faithfulness.
        assert!(js.contains("(x) => pyAdd(x, 1)"));
    }

    #[test]
    fn test_super_as_plain_identifier_is_sanitized() {
        // Sweep-A S1 finding: `super` is a full JS reserved word (not just
        // in class-method position) — `let super = 5;` is a SyntaxError
        // under Node ESM (always strict mode). PythScribe special-cases
        // `super()` *calls* via a dedicated AST match before general Name
        // emission (see the two `"super"` call-site matches above), so
        // sanitizing bare `super` as an identifier here cannot break that
        // path. Regression test for the fix (previously `is_js_reserved_word`
        // deliberately excluded "super").
        let js = compile("super = 5\nprint(super + 1)");
        assert!(
            js.contains("super$"),
            "bare `super` identifier must be sanitized: {js}"
        );
        assert!(
            !js.contains("let super "),
            "unsanitized `super` binding is illegal JS: {js}"
        );
    }
}
