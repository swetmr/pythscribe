use std::cell::Cell;
use std::collections::{HashMap, HashSet};

use pyths_syntax::ast::*;
use pyths_syntax::operators::*;

use crate::builtins::{builtin_func_mapping, BuiltinMapping};
use crate::method_lowering::{is_simple_receiver, method_lowering, InlineSpec, MethodLowering};
use crate::method_table::{container_method_arity, ReceiverKind};
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
    /// `w: int = property(getW, setW)` — a descriptor field: installed as a
    /// class attribute (not an __init__ param), but CPython's generated
    /// __repr__/__eq__ still read it (through the getter).
    property_default: bool,
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
    /// @dataclass(order=True): emit __lt__/__le__/__gt__/__ge__ comparing
    /// the field tuple (CPython's generated ordering).
    order: bool,
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

/// autotester data_classes: typing-module constructs that can appear as
/// (bare) annotations but are NOT runtime classes — they must never become
/// an `instanceof <Name>` check (`x: ClassVar = 10` emitted `instanceof
/// ClassVar` → ReferenceError). `List`/`Dict` are handled structurally below.
fn is_typing_only_name(name: &str) -> bool {
    matches!(
        name,
        "Any" | "ClassVar" | "Optional" | "Union" | "Callable" | "Tuple"
            | "Set" | "FrozenSet" | "Iterable" | "Iterator" | "Sequence"
            | "Mapping" | "MutableMapping" | "MutableSequence" | "Final"
            | "Literal" | "Type" | "NoReturn" | "Never" | "Hashable"
            | "Generator" | "Coroutine" | "Awaitable" | "AnyStr" | "Text"
            | "TypeVar" | "Protocol" | "TypedDict" | "NamedTuple"
    )
}

/// autotester data_classes: is this annotation `ClassVar` / `ClassVar[...]`?
/// CPython dataclasses EXCLUDE such pseudo-fields from __init__ — they are
/// plain class attributes.
fn is_classvar_annotation(annotation: &Expr) -> bool {
    match &annotation.kind {
        ExprKind::Name(n) => n == "ClassVar",
        ExprKind::Subscript { value, .. } => {
            matches!(&value.kind, ExprKind::Name(n) if n == "ClassVar")
        }
        _ => false,
    }
}

/// Resolve a Python type annotation AST node to a TypeCheck.
fn resolve_type_check(annotation: &Expr) -> TypeCheck {
    match &annotation.kind {
        ExprKind::Name(n) => match n.as_str() {
            "int" => TypeCheck::Int,
            "float" => TypeCheck::Float,
            "str" => TypeCheck::Str,
            "bool" => TypeCheck::Bool,
            "list" | "List" => TypeCheck::List(None),
            "dict" | "Dict" => TypeCheck::Dict(None, None),
            other => {
                if !is_typing_only_name(other)
                    && other.chars().next().is_some_and(|c| c.is_uppercase())
                {
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
                                "order" => {
                                    if let ExprKind::BoolLiteral(v) = &kw.value.kind {
                                        opts.order = *v;
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
/// `property(...)` call detection: a dataclass field whose DEFAULT is a
/// property stays a class-attribute descriptor (reads hit the getter), not
/// an __init__ field — matching CPython's observable behavior.
fn is_property_call(e: &Expr) -> bool {
    matches!(&e.kind, ExprKind::Call { func, .. }
        if matches!(&func.kind, ExprKind::Name(n) if n == "property"))
}

/// WB-14: is `e` an immutable constant literal — one whose value is identical
/// whether evaluated at def-time or call-time (so a lambda/function default of
/// this shape needs NO def-time hoist; a bare JS default param is equivalent)?
/// Excludes mutable literals (`[]`, `{}`, sets, tuples of exprs) and any
/// expression that can observe surrounding state (Names, calls, f-strings).
fn is_const_literal(e: &Expr) -> bool {
    matches!(
        &e.kind,
        ExprKind::NoneLiteral
            | ExprKind::BoolLiteral(_)
            | ExprKind::IntLiteral(_)
            | ExprKind::FloatLiteral(_)
            | ExprKind::ImagLiteral(_)
            | ExprKind::StringLiteral(_)
            | ExprKind::BytesLiteral(_)
    )
}

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
            // autotester data_classes: ClassVar pseudo-fields are NOT
            // dataclass fields (CPython excludes them from __init__); they
            // are installed as class attributes by emit_class_def instead.
            if is_classvar_annotation(annotation) {
                continue;
            }
            let property_default =
                value.as_ref().is_some_and(|v| is_property_call(v));
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
                                    property_default: false,
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
                    property_default,
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
/// Round-3 unification: the decision `plan_import_binding` hands back to an
/// import emitter. Every import form (plain/aliased `import`, generic
/// `from ... import`, the recognized-lib hybrid from-import, relative
/// from-imports) maps its names through the SAME planner; only the ESM
/// SYNTAX it then emits (named specifier vs `import * as`) is per-form.
#[derive(Debug)]
enum ImportBindingPlan {
    /// This binding already IS this exact import (same module + export) —
    /// idempotent Python re-import; emit nothing.
    Dedup,
    /// First binder of this name — hoist under the sanitized binding name.
    Fresh,
    /// Hoist under `unique` and emit a body-local rebind: `binding = unique`
    /// when the name is already a param/earlier local (`reassign`), else
    /// `const binding = unique` (a fresh shadow of an outer-scope alias).
    Rebind {
        js_binding: String,
        unique: String,
        reassign: bool,
    },
    /// DX-B2 module-scope cross-module collision — diagnostic already
    /// emitted; the caller aborts the statement.
    Error,
    /// DX-B2 root fix (alias-and-rewrite): a module-scope cross-module
    /// collision where the PYTHON source names DIFFER — the JS-name
    /// convergence was manufactured by our own snake→camel import
    /// conversion (`from zustand import create_store` → `createStore`
    /// beside `from redux import createStore`). Both bindings are valid,
    /// distinct Python names, so a hard error would reject a correct
    /// program. Instead this import is hoisted under `unique` and every
    /// reference to its Python name is rewritten via `import_ref_renames`;
    /// the earlier claimant keeps the plain JS binding. A same-Python-name
    /// collision between the JS binding's two *original* claimants
    /// (`import a as z` + `import b as z`) is a genuine double-bind and
    /// still hard-errors; but once a different-Python-name import already
    /// holds the binding via alias, a later same-Python-name import resolves
    /// by Python last-wins (another alias), never a wrong runtime either way.
    Alias { unique: String },
}

/// FULL_SURFACE #1: per-scope bookkeeping for dotted NO-ALIAS imports
/// (`import pkg.sub`). One entry per Python scope, in lockstep with
/// `declared_scopes` (see `push_scope`/`pop_scope`).
#[derive(Debug, Default)]
struct DottedImportScope {
    /// Head names (`pkg` of `import pkg.sub`) bound in this scope to a
    /// MUTABLE package object (`let pkg = {};`) that dotted paths are
    /// grafted onto.
    heads: HashSet<String>,
    /// Dotted paths currently holding a plain mutable intermediate object
    /// (`pkg.sub = {};` — created so a deeper level could be grafted).
    obj_paths: HashSet<String>,
    /// Dotted paths currently holding a grafted frozen ESM namespace
    /// (the leaf of a previous dotted import). If a later import needs a
    /// CHILD under such a path, the namespace is copied into a mutable
    /// object first (`pkg.sub = Object.assign({}, pkg.sub);`).
    ns_paths: HashSet<String>,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum JsInferredType {
    /// `int`, `bool`, `str`, `None` — JS truthiness and `===` match
    /// Python semantics, so these skip the helper wrap.
    Primitive,
    /// Provably `float` (float literal, `float`-annotated, true-division
    /// result, or arithmetic among floats). Option B value model: a float is
    /// a native JS `Number` when non-integer-valued and a boxed `PyFloat`
    /// when integer-valued (8.0) — never a BigInt — so arithmetic on two
    /// Floats can skip the arbitrary-precision helper and emit the bare-op
    /// unwrap/re-box fast path (`__pyF(__reqNum(a) op __reqNum(b))`,
    /// unwrapping through the value-boundary authority). Because the value
    /// MAY be a box (a JS object), Float does NOT share Primitive's bare
    /// truthiness or `===` equality: `if x:` routes through pyBool and
    /// `==` through pyEq (both box-aware).
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
// THE numeric value-boundary authority (mirror of runtime/src/operators.js —
// see the block comment there; #460/#461/#464): float ⇔ __pyfloat__ brand or
// non-integer Number; an integer-valued Number of ANY magnitude is an int
// (exactness past 2**53 restored by __intBin's BigInt promotion).
const __isFloat = (x) => (typeof x === "number" && !Number.isInteger(x)) || (x != null && x.__pyfloat__ === true);
const __toBig = (x) => (typeof x === "bigint" ? x : BigInt(x));
const __norm = (big) => (big >= -__MAX_SAFE && big <= __MAX_SAFE ? Number(big) : big);
function __intBin(a, b, numOp, bigOp) {
    if (typeof a === "number" && typeof b === "number"
        && Number.isSafeInteger(a) && Number.isSafeInteger(b)) {
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
    if (x != null && x.__pyfloat__ === true) return x.valueOf();
    return Number(x);
};
const __numeric = (x) => typeof x === "number" || typeof x === "bigint" || (x != null && x.__pyfloat__ === true);
// #469: operand-type names come from __pyTypeName — THE ONE type-name source,
// extracted ON USE from the canonical runtime.js (same emit-on-reference
// discipline as __pyBytesKind; see emit_inline_runtime). The old inline
// __opTypeName copy lacked the function/class/plain-object arms and leaked
// JS class names ('Function'/'Object') where CPython says
// 'function'/'type'/'dict'.
function __arithTypeErr(msg) { const e = new Error(msg); e.name = "TypeError"; return e; }
// E2 (#466): THE binary-op operand-type authority — mirror of
// runtime/src/operators.js __binOpTypeError (parity battery covers it).
// Always throws the CPython TypeError for an operand pair no valid
// combination matched; subsumes the old __arithNoneGuard (#322).
function __binOpTypeError(op, a, b) {
    if (op === "+") {
        const ak = __pyBytesKind(a);
        if (ak !== null) throw __arithTypeErr(`can't concat ${__pyTypeName(b)} to ${ak}`);
        if (typeof a === "string") throw __arithTypeErr(`can only concatenate str (not "${__pyTypeName(b)}") to str`);
        if (Array.isArray(a)) {
            const an = a.__pytuple__ ? "tuple" : "list";
            throw __arithTypeErr(`can only concatenate ${an} (not "${__pyTypeName(b)}") to ${an}`);
        }
    } else if (op === "*") {
        const seq = (v) => typeof v === "string" || Array.isArray(v) || __pyBytesKind(v) !== null;
        if (seq(a)) throw __arithTypeErr(`can't multiply sequence by non-int of type '${__pyTypeName(b)}'`);
        if (seq(b)) throw __arithTypeErr(`can't multiply sequence by non-int of type '${__pyTypeName(a)}'`);
    }
    throw __arithTypeErr(`unsupported operand type(s) for ${op}: '${__pyTypeName(a)}' and '${__pyTypeName(b)}'`);
}
// E2 (#466): sequence-replication count rule — int/bool only (bool ⊂ int),
// null = invalid. Mirror of runtime/src/operators.js __mulRepCount.
const __mulRepCount = (v) => {
    if (typeof v === "boolean") return v ? 1 : 0;
    // #471: CPython bounds the count to an index-sized (Py_ssize_t) integer —
    // `[1] * (10**30)` raises OverflowError, never a JS RangeError.
    if (typeof v === "bigint") {
        if (v > 9223372036854775807n || v < -9223372036854775808n) throw __ofe("cannot fit 'int' into an index-sized integer");
        return Number(v);
    }
    if (typeof v === "number" && Number.isInteger(v)) {
        if (v >= 9223372036854775808 || v < -9223372036854775808) throw __ofe("cannot fit 'int' into an index-sized integer");
        return v;
    }
    return null;
};
const __arithNumOk = (x) => typeof x === "number" || typeof x === "bigint" || typeof x === "boolean" || (x != null && x.__pyfloat__ === true);
function __reqArithNum(op, a, b) {
    if (!__arithNumOk(a) || !__arithNumOk(b)) __binOpTypeError(op, a, b);
}
// Wave-15 F4: bool ⊂ int — coerce bool operands (when the other side is
// numeric/bool) before arithmetic so bool+BigInt stays exact.
const __boolNum = (x) => (x ? 1 : 0);
function __pyBytesRep(src, n) {
    // item 5: bytearray * n -> bytearray (type follows the bytes-like operand),
    // duck-typed via the mutator surface. Mirrors runtime/src/operators.js
    // __bytesRepeat (distinct name so the parity freeze stays accurate).
    const count = n < 0 ? 0 : n;
    if (typeof src.copy === "function" && typeof src.clear === "function" && typeof src.extend === "function") {
        const out = src.copy();
        out.clear();
        for (let i = 0; i < count; i++) out.extend(src);
        return out;
    }
    const out = new PyBytes(src.length * count);
    for (let i = 0; i < count; i++) out.set(src, i * src.length);
    return out;
}
function pyAdd(a, b, fctx) {
    if (typeof a === "boolean" && __arithNumOk(b)) a = __boolNum(a);
    if (typeof b === "boolean" && __arithNumOk(a)) b = __boolNum(b);
    if (__numeric(a) && __numeric(b)) {
        if (fctx || __isFloat(a) || __isFloat(b)) return __pyF(__reqNum(a) + __reqNum(b));
        return __intBin(a, b, (x, y) => x + y, (x, y) => x + y);
    }
    if (a != null && typeof a.__add__ === "function") return a.__add__(b);
    if (b != null && typeof b.__radd__ === "function") return b.__radd__(a);
    if (a instanceof Uint8Array && b instanceof Uint8Array) {
        // item 5: result type follows the LEFT operand (bytearray + bytes-like
        // -> bytearray), duck-typed via the mutator surface. Mirrors
        // runtime/src/operators.js pyAdd (parity battery covers it).
        if (typeof a.copy === "function" && typeof a.extend === "function") { const out = a.copy(); out.extend(b); return out; }
        const out = new PyBytes(a.length + b.length); out.set(a, 0); out.set(b, a.length); return out;
    }
    if (Array.isArray(a) && Array.isArray(b)) {
        const at = !!a.__pytuple__, bt = !!b.__pytuple__;
        if (at !== bt) throw new TypeError(`can only concatenate ${at ? "tuple" : "list"} (not "${bt ? "tuple" : "list"}") to ${at ? "tuple" : "list"}`);
        const r = [...a, ...b];
        if (at) Object.defineProperty(r, "__pytuple__", { value: true, enumerable: false });
        return r;
    }
    if (typeof a === "string" && typeof b === "string") return a + b;
    // E2 (#466): no valid combination matched — the operand-type authority
    // decides the CPython TypeError (was: raw `a + b` string coercion).
    __binOpTypeError("+", a, b);
}
function pySub(a, b) {
    if (typeof a === "boolean" && __arithNumOk(b)) a = __boolNum(a);
    if (typeof b === "boolean" && __arithNumOk(a)) b = __boolNum(b);
    if (__numeric(a) && __numeric(b)) {
        if (__isFloat(a) || __isFloat(b)) return __pyF(__reqNum(a) - __reqNum(b));
        return __intBin(a, b, (x, y) => x - y, (x, y) => x - y);
    }
    if (a instanceof Set && b instanceof Set) { const out = new (a.constructor)(a); for (const v of b) out.delete(v); return out; }
    if (a != null && typeof a.__sub__ === "function") return a.__sub__(b);
    if (b != null && typeof b.__rsub__ === "function") return b.__rsub__(a);
    __reqArithNum("-", a, b);
    return Number(a) - Number(b);
}
function pyMul(a, b, fctx) {
    if (typeof a === "boolean" && __arithNumOk(b)) a = __boolNum(a);
    if (typeof b === "boolean" && __arithNumOk(a)) b = __boolNum(b);
    if (__numeric(a) && __numeric(b)) {
        if (fctx || __isFloat(a) || __isFloat(b)) return __pyF(__reqNum(a) * __reqNum(b));
        return __intBin(a, b, (x, y) => x * y, (x, y) => x * y);
    }
    if (a != null && typeof a.__mul__ === "function") return a.__mul__(b);
    if (b != null && typeof b.__rmul__ === "function") return b.__rmul__(a);
    // E2 (#466): sequence replication — INT/bool counts only, validated by
    // __mulRepCount; anything else falls to the operand-type authority.
    // Mirrors runtime/src/operators.js pyMul (parity battery covers it).
    {
        const aSeq = typeof a === "string" || Array.isArray(a) || a instanceof Uint8Array;
        const bSeq = typeof b === "string" || Array.isArray(b) || b instanceof Uint8Array;
        if (aSeq !== bSeq) {
            const n = __mulRepCount(aSeq ? b : a);
            if (n !== null) {
                const s = aSeq ? a : b;
                if (s instanceof Uint8Array) return __pyBytesRep(s, n);
                if (typeof s === "string") return s.repeat(Math.max(0, n));
                const result = [];
                for (let i = 0; i < n; i++) result.push(...s);
                if (s.__pytuple__) Object.defineProperty(result, "__pytuple__", { value: true, enumerable: false });
                return result;
            }
        }
    }
    __binOpTypeError("*", a, b);
}
function pyDiv(a, b, floatDiv) {
    if (typeof a === "boolean" && __arithNumOk(b)) a = __boolNum(a);
    if (typeof b === "boolean" && __arithNumOk(a)) b = __boolNum(b);
    if ((!__numeric(a) || !__numeric(b)) && a != null && typeof a.__truediv__ === "function") return a.__truediv__(b);
    if ((!__numeric(a) || !__numeric(b)) && b != null && typeof b.__rtruediv__ === "function") return b.__rtruediv__(a);
    __reqArithNum("/", a, b);
    const bn = __reqNum(b);
    if (bn === 0) throw __zde((floatDiv || __isFloat(a) || __isFloat(b)) ? "float division by zero" : "division by zero");
    return __pyF(__reqNum(a) / bn);
}
function pyFloorDiv(a, b) {
    if (typeof a === "boolean" && __arithNumOk(b)) a = __boolNum(a);
    if (typeof b === "boolean" && __arithNumOk(a)) b = __boolNum(b);
    if ((!__numeric(a) || !__numeric(b)) && a != null && typeof a.__floordiv__ === "function") return a.__floordiv__(b);
    if ((!__numeric(a) || !__numeric(b)) && b != null && typeof b.__rfloordiv__ === "function") return b.__rfloordiv__(a);
    __reqArithNum("//", a, b);
    if (__isFloat(a) || __isFloat(b)) {
        const x = Number(a), y = Number(b);
        if (y === 0) throw __zde("float floor division by zero");
        const mod = x % y;
        let div = (x - mod) / y;
        if (mod !== 0 && (y < 0) !== (mod < 0)) div -= 1;
        let fd = Math.floor(div);
        if (div - fd > 0.5) fd += 1;
        return __pyF(fd);
    }
    if (Number(b) === 0) throw __zde("integer division or modulo by zero");
    return __intBin(a, b, (x, y) => Math.floor(x / y), (x, y) => { let q = x / y; if (x % y !== 0n && (x < 0n) !== (y < 0n)) q -= 1n; return q; });
}
function pyMod(a, b) {
    if (typeof a === "boolean" && __arithNumOk(b)) a = __boolNum(a);
    if (typeof b === "boolean" && __arithNumOk(a)) b = __boolNum(b);
    if ((!__numeric(a) || !__numeric(b)) && a != null && typeof a.__mod__ === "function") return a.__mod__(b);
    if ((!__numeric(a) || !__numeric(b)) && b != null && typeof b.__rmod__ === "function") return b.__rmod__(a);
    // Honest unsupported-feature error: printf-style %-formatting is a
    // surface PythScribe does not implement (known limitation; use f-strings).
    if (typeof a === "string") {
        const e = new Error("printf-style %-formatting is not supported by PythScribe; use an f-string");
        e.name = "NotImplementedError"; throw e;
    }
    __reqArithNum("%", a, b);
    if (__isFloat(a) || __isFloat(b)) {
        const bf = __reqNum(b);
        if (bf === 0) throw __zde("float modulo by zero");
        // Sign-of-divisor correction without the `(+y)%y` re-mod (rounds at
        // huge divisors) — mirrors runtime/src/operators.js pyMod.
        let m = __reqNum(a) % bf;
        if (m !== 0 && (m < 0) !== (bf < 0)) m += bf;
        return __pyF(m);
    }
    if (Number(b) === 0) throw __zde("integer modulo by zero"); // CPython 3.12: `%` says "modulo", `//` keeps "division or modulo"
    return __intBin(a, b, (x, y) => { let m = x % y; if (m !== 0 && (m < 0) !== (y < 0)) m += y; return m; }, (x, y) => { let m = x % y; if (m !== 0n && (m < 0n) !== (y < 0n)) m += y; return m; });
}
function pyPow(a, b, fctx) {
    if (typeof a === "boolean" && __arithNumOk(b)) a = __boolNum(a);
    if (typeof b === "boolean" && __arithNumOk(a)) b = __boolNum(b);
    if (__numeric(a) && __numeric(b)) {
        if (fctx || __isFloat(a) || __isFloat(b) || (typeof b === "bigint" ? b < 0n : b < 0)) {
            const an = __reqNum(a), bn = __reqNum(b);
            // Mirrors runtime/src/operators.js pyPow: zero to a negative
            // power is CPython's ZeroDivisionError, not OverflowError.
            if (an === 0 && bn < 0) throw __zde("0.0 cannot be raised to a negative power");
            const r = an ** bn;
            if (!isFinite(r) && isFinite(an) && isFinite(bn)) throw __ofe("(34, 'Result too large')");
            return __pyF(r);
        }
        return __intBin(a, b, (x, y) => x ** y, (x, y) => x ** y);
    }
    if (a != null && typeof a.__pow__ === "function") return a.__pow__(b);
    if (b != null && typeof b.__rpow__ === "function") return b.__rpow__(a);
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
    static __name__ = "object";
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
            case "float": return (typeof obj === "number" && !Number.isInteger(obj)) || (obj != null && obj.__pyfloat__ === true);
            case "dict": return obj !== null && typeof obj === "object" && (Object.getPrototypeOf(obj) === Object.prototype || obj instanceof Map);
            case "set": case "frozenset": return obj instanceof Set;
            // Bytes authority: bytearray is NOT a bytes subclass in CPython,
            // so each name matches exactly its own kind.
            case "bytes": return __pyBytesKind(obj) === "bytes";
            case "bytearray": return __pyBytesKind(obj) === "bytearray";
            case "NoneType": return obj === null || obj === undefined;
            case "object": return true;
        }
        return false;
    }
    // Interned CALLABLE type objects (int/list/dict/object/… as VALUES —
    // runtime.js __pyType* singletons) dispatch through the string path.
    if (typeof cls === "function" && cls.__pytype__ === true) {
        return __pyIsInstance(obj, cls.__name__);
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
function pyProperty(fget, fset, fdel, doc) {
    return { __pyproperty__: true, fget, fset, fdel, doc };
}
function __pyClassAttr(cls, name, value) {
    cls[name] = value;
    if (value !== null && typeof value === "object" && value.__pyproperty__) { // autotester properties (mirrors runtime/src/classes.js)
        Object.defineProperty(cls.prototype, name, {
            get() { return value.fget ? value.fget.call(this) : undefined; },
            set(v) { if (!value.fset) { const e = new Error("can't set attribute"); e.name = "AttributeError"; throw e; } value.fset.call(this, v); },
            configurable: true,
        });
        return;
    }
    Object.defineProperty(cls.prototype, name, {
        get() { return cls[name]; },
        set(v) { Object.defineProperty(this, name, { value: v, writable: true, enumerable: true, configurable: true }); },
        configurable: true,
    });
}
function __pyClassCall(cls, name, args) {
    const s = cls[name];
    if (typeof s === "function" && /^class[\s{(]/.test(Function.prototype.toString.call(s))) return new s(...args); // autotester classes: nested class attr constructs with new (mirrors runtime/src/classes.js)
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

/// WB-15 (naming soundness, NB-1 family): how a bare identifier `self` lowers
/// at the CURRENT emission point. This is the SINGLE, order-independent notion
/// that replaced the former quartet of interacting flags (`self_receiver_depth`
/// / `in_nested_fn_of_method` / `self_param_fn_depth` / `method_self_alias`),
/// whose per-context interaction leaked (a free `self` in a nested fn of a
/// `@staticmethod` still aliased the receiver; a `@classmethod def m(self)`
/// threw). It is computed once from the real binding structure — the enclosing
/// method's receiver — and saved/restored at every scope that can rebind `self`:
/// method bodies, nested `function`/`class` defs, `@static`/`@classmethod`,
/// lambda params, comprehension for-targets, and a method-local rebind of
/// `self` (assignment/for/with target).
///
/// The rule: `self` lowers to the receiver IFF, in the current scope, it names
/// an actual instance-method receiver — the first param `self` of an enclosing
/// non-static, non-classmethod method (incl. `__init__`/constructors) — AND is
/// NOT shadowed by a closer binding of `self`. A closer binding is honored
/// uniformly: a lambda param `self` (S4), a comprehension target `self` (S5),
/// and a method's own local rebind `self = …` / `for self in …` (S6) all make
/// `self` an ordinary identifier in their scope — both in reference AND binder
/// position (a `self` binder must NEVER emit `this`, which is un-assignable and
/// illegal as a JS param). A method that locally rebinds its receiver `self`
/// captures it once as `let self = this;` (JS `this` is not assignable) so a
/// pre-rebind `self.attr` read still sees the instance. Everywhere else — module
/// scope, a plain function, a `@staticmethod`/`@classmethod` body, a shadowing
/// scope — `self` is the ordinary identifier.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum SelfLowering {
    /// `self` is an ordinary identifier → emit `self`. Module scope, a plain
    /// function, a `@staticmethod`/`@classmethod` body whose receiver param is
    /// not literally `self`, a `self` PARAM of a nearer non-method scope, or a
    /// classmethod's `const self = this` local (which the identifier reads).
    /// Emitting `this` here produced `export let this = …` (a hard syntax error
    /// that silently aborted the module — pull-loading's `var self = {}` bag).
    Ordinary,
    /// `self` is the enclosing instance-method receiver, referenced directly in
    /// the method's own `this` frame → emit `this`.
    Receiver,
    /// `self` is the enclosing instance-method receiver, but we are inside a
    /// nested `function`/`class`/static-method whose own `this` is rebound →
    /// emit the `__self` alias captured at the top of that instance method.
    ReceiverAlias,
}

/// #452 (review blocker 2): which scope OWNS a sentinel-hoisted name at a
/// read site — decides the unbound-read guard (see `sentinel_read`).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum SentinelRead {
    /// The innermost function scope owns it → UnboundLocalError
    /// (`__pyChkLocal`).
    Local,
    /// An enclosing function scope owns it → free-variable NameError
    /// (`__pyChkFree`).
    Free,
    /// Module scope owns it → CPython's dynamic globals → builtins chain
    /// (`__pyChkGlobal`, or the builtin value fallback for builtin names).
    Global,
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
    /// Issue #438: the PRE-COMPUTED complete local-binding set per Python scope
    /// (module / function / method / comprehension), pushed/popped in lockstep
    /// with the scope stack. Unlike `declared_scopes` (built incrementally in
    /// source order for `let`-emission), this holds ALL names a scope binds
    /// up front, so shadow resolution (`is_declared_in_any_scope`) is
    /// ORDER-INDEPENDENT — a builtin shadowed by a local anywhere in the scope
    /// resolves to the local. Populated by `collect_local_bindings`.
    scope_bindings: Vec<HashSet<String>>,
    /// B8(b): every name bound by MODULE-level user code (assignments,
    /// def/class, for/with/except targets — order-independent pre-scan via
    /// `collect_bound_names`). Consulted by `plan_import_binding_impl` so a
    /// snake→camel import whose manufactured JS name collides with a USER
    /// binding hoists under a unique name (alias-and-rewrite) instead of
    /// clashing ("Identifier already declared" / a silently killed import).
    module_bound_names: HashSet<String>,
    /// #452/#453 (naming soundness): EVERY identifier the module uses anywhere
    /// — bare-name references AND binding names, at any nesting depth
    /// (order-independent whole-module pre-pass via `collect_all_idents`).
    /// Seed set for `fresh_temp`, which guarantees internal codegen
    /// temporaries (`__result`, `__comp_it`, `__gen_it`) can never collide
    /// with a user-visible name.
    module_idents: HashSet<String>,
    /// Review finding 2: per function-like scope, the names declared `global`
    /// in it. A `global X` reference must resolve ONLY at module/builtin scope,
    /// skipping intervening enclosing-function frames (an enclosing local `X`
    /// must not capture it). Pushed/popped in lockstep with `scope_bindings`.
    scope_globals: Vec<HashSet<String>>,
    /// #274: JS binding names already emitted by a module-scope `import` (the
    /// name after `as`, or the plain imported name). Python tolerates importing
    /// the same name twice (idempotent rebind); ES modules do not — a second
    /// `import { defaultdict }` is a "already declared" SyntaxError. Dedupe by
    /// binding so re-imports (common when a file re-imports a name its preamble
    /// / another line already brought in) are dropped.
    imported_bindings: HashSet<String>,
    /// DX-B2: JS binding name → (import identity `module\0export`, PYTHON
    /// source name that claimed it). Re-importing the SAME binding from the
    /// SAME module is idempotent (dedup); importing it from a DIFFERENT
    /// module is a cross-module collision — the old flat `imported_bindings`
    /// set silently dropped the second import (or, on the other order,
    /// emitted an unconverted call → ReferenceError). When the two PYTHON
    /// names also match, we hard-error (genuine double-bind). When they
    /// DIFFER, the collision is an artifact of our snake→camel conversion
    /// (`create_store` vs `createStore`) — both are valid distinct Python
    /// bindings, so the later import is ALIASED under a unique JS name and
    /// its references rewritten (see `import_ref_renames`).
    imported_binding_modules: HashMap<String, (String, String)>,
    /// DX-B2 alias-and-rewrite: module-scope Python name → the unique JS
    /// name its colliding import was hoisted under. Consulted at every
    /// bare-Name reference (before the react_imports snake→camel match) so
    /// call/value sites bind the right import. A function-scope binding of
    /// the same name shadows the import and skips the rewrite.
    import_ref_renames: HashMap<String, String>,
    /// DX-B2 alias-and-rewrite: Python name → import identity for the
    /// aliased side, so an idempotent re-import of the SAME aliased import
    /// dedups instead of minting a second unique hoist.
    aliased_import_identities: HashMap<String, String>,
    /// Fix J (scope-aware hoisted-import dedup): the JS namespace binding name
    /// → the resolved module it was FIRST hoisted for. ES modules put every
    /// import at the top of the file, so a function-local `import pandas as m`
    /// cannot share the top-level `m` that a `import numpy as m` in another
    /// function already claimed. When a function-local alias collides with a
    /// DIFFERENT module here, the import is hoisted under a UNIQUE name and an
    /// in-body `const <alias> = <unique>;` shadows the outer binding within that
    /// function (JS block scoping resolves references — no rewrite needed).
    /// Same alias + same module dedups; module-scope collisions hard-error (DX-B2).
    hoisted_alias_module: HashMap<String, String>,
    /// Monotonic suffix for the unique names minted by the fix-J rename above.
    import_rename_counter: u32,
    /// Round-3 unification: per-scope map of binding name → (import IDENTITY
    /// `module\0export`, assignable) for the import that last bound it IN
    /// THAT SCOPE, pushed/popped in lockstep with `declared_scopes`.
    /// `assignable` records whether the JS binding the name resolves to in
    /// that scope is a mutable body-local (`let` shadow / reassigned param)
    /// vs an immutable module-top import hoist. This is what lets
    /// `plan_import_binding` tell "this binding is already THIS import"
    /// (idempotent re-import → dedup) apart from "this binding is a param /
    /// earlier local that happens to share the name" (→ reassign rebind):
    /// `is_declared` alone cannot distinguish the two once the first import
    /// has declared the name.
    scope_import_decls: Vec<HashMap<String, (String, bool)>>,
    /// FULL_SURFACE #1 (`import pkg.sub` without `as`): per-scope state for
    /// dotted no-alias imports, pushed/popped in lockstep with
    /// `declared_scopes`. See [`Self::emit_dotted_no_alias_import`].
    dotted_import_scopes: Vec<DottedImportScope>,
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
    /// #443: class names whose `class` statement has already been EMITTED
    /// (source order). `known_classes` is pre-scanned and order-independent,
    /// so when an import later REBINDS a name whose class definition already
    /// executed (`class sqrt: …` then `from math import sqrt`), the name is
    /// dropped from `known_classes` — Python is last-wins, and calls after
    /// the rebind must not `new`-construct the (now shadowed) class.
    emitted_class_names: HashSet<String>,
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
    /// 0.2.2 member-call class fix: local namespace aliases bound to a CORE
    /// React module (`import react [as R]`, `import react_dom [as D]`,
    /// `import react_dom.client as C`) → which module. EVERY member access on
    /// one of these — call position, value position, any member — routes
    /// through `react::route_namespace_member`: camel-cased + module-checked
    /// against the audited table, or a compile diagnostic (removed /
    /// wrong-module). No member may fall through to a silent-dead snake
    /// identifier or a `pyBoundMethod` wrap.
    react_namespace_alias_modules: HashMap<String, react::ReactHelperSource>,
    /// Track-B: bindings whose LOWERCASE members are React components
    /// (framer-motion's `motion.div` / `motion.span`). Member calls rooted
    /// here dispatch to createElement even though the attr is lowercase.
    react_member_component_bases: HashSet<String>,
    /// TB-1: LOCAL bindings (alias-aware) that refer to React's
    /// `createElement` factory — `from react import createElement`,
    /// `create_element`, or `create_element as h`. When such a name is CALLED
    /// directly, its props argument (the 2nd positional, if a dict literal) is
    /// in PSX-prop position, so its keys get the snake→camel/kebab prop-name
    /// transform. This is the ONLY dict-literal position that transform reaches
    /// — general dict literals emit keys verbatim (TB-1 soundness fix).
    react_create_element_fns: HashSet<String>,
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
    /// autotester docstrings: the module docstring (first statement string
    /// literal), backing `__doc__` reads; None when absent, like CPython.
    module_doc: Option<String>,
    /// autotester data_classes: per-dataclass FLATTENED field statements
    /// (base fields first, CPython inheritance order) so a derived
    /// @dataclass constructor/__repr__/__eq__ covers inherited fields.
    dataclass_field_stmts: HashMap<String, Vec<Stmt>>,
    /// WB-15 (naming soundness, NB-1 family): the SINGLE predicate that governs
    /// how a bare identifier `self` lowers here — see `SelfLowering`. Replaces
    /// the former interacting quartet (`self_receiver_depth`,
    /// `in_nested_fn_of_method`, `self_param_fn_depth`, `method_self_alias`);
    /// computed from real binding structure and saved/restored at every scope
    /// boundary that can rebind `self`, so no call site can reintroduce the leak.
    self_lowering: SelfLowering,
    /// autotester callable_test: true when any class in this module defines
    /// a `__call__` method — gates the __pyCall local-variable call wrap.
    module_has_dunder_call: bool,
    /// autotester module_math/module_itertools: `from <stdlib> import *`
    /// really binds names. Every export of the stdlib shim (parsed at build
    /// time from the embedded source) maps to its namespace-import var +
    /// whether it is a class (constructed with `new`). An undeclared Name
    /// resolves through this map (declared locals/params shadow, matching
    /// CPython's rebinding), and it suppresses the builtin lowerings —
    /// `from math import *` makes `pow` mean `math.pow`.
    star_import_bindings: HashMap<String, (String, bool)>,
    /// autotester simple_and_augmented_assignment: true while emitting the
    /// CALLEE of a direct call — `recv.m(...)` keeps `this` in plain JS, so
    /// the value-position pyBoundMethod wrap is unnecessary noise there.
    in_call_callee: bool,
    /// autotester properties: while emitting a post-class attribute VALUE
    /// (`x = property(getX, setX)` in a class body), sibling method names
    /// resolve to `<Cls>.prototype.<name>` (Python class-body scoping).
    class_attr_subst: Option<(String, HashSet<String>)>,
    /// autotester module_datetime: local names bound to the datetime module
    /// namespace (`import datetime [as dt]`). Its class names are lowercase,
    /// so the #224 Capitalized-attr `new` heuristic never fires — qualified
    /// constructor calls (`datetime.date(...)`) need their own `new` rule.
    datetime_namespaces: HashSet<String>,
    /// Local names bound to asyncio's `run` via `from asyncio import run`.
    asyncio_run_fns: HashSet<String>,
    /// #448: local names bound to `importlib.import_module` via
    /// `from importlib import import_module [as X]`. A call on such a name
    /// lowers to native ES dynamic `import(<spec>)` (see the call site and
    /// builtins::import_module). Covers the aliased form; the bare
    /// `import_module(...)` builtin is handled by builtin_func_mapping.
    import_module_fns: HashSet<String>,
    /// #448: local names bound to the `importlib` module via `import importlib
    /// [as X]`. `importlib` is not a real module in the compiled output — the
    /// ONLY supported surface is `import_module`, and only through the
    /// `from importlib import import_module` / bare-builtin forms. A member call
    /// `<ns>.import_module(...)` is diagnosed (it has no valid lowering).
    importlib_namespaces: HashSet<String>,
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
            scope_bindings: vec![HashSet::new()], // module scope (filled in emit_module)
            module_bound_names: HashSet::new(),
            module_idents: HashSet::new(),
            scope_globals: vec![HashSet::new()],
            dotted_import_scopes: vec![DottedImportScope::default()], // module scope
            hoisted_scopes: vec![HashSet::new()],  // module scope
            sentinel_scopes: vec![HashSet::new()], // module scope
            imported_bindings: HashSet::new(),
            imported_binding_modules: HashMap::new(),
            import_ref_renames: HashMap::new(),
            aliased_import_identities: HashMap::new(),
            hoisted_alias_module: HashMap::new(),
            import_rename_counter: 0,
            scope_import_decls: vec![HashMap::new()], // module scope
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
            emitted_class_names: HashSet::new(),
            local_module_imports: HashSet::new(),
            float_returning_functions: HashSet::new(),
            pydict_forced_locals: HashSet::new(),
            force_pydict_literal: false,
            codegen_errors: Vec::new(),
            certificate: crate::cert::Certificate::default(),
            react_imports: HashSet::new(),
            react_lib_bindings: HashSet::new(),
            react_lib_module_aliases: HashSet::new(),
            react_namespace_alias_modules: HashMap::new(),
            react_member_component_bases: HashSet::new(),
            react_create_element_fns: HashSet::new(),
            npm_imports: HashMap::new(),
            react_refresh: false,
            class_stack: Vec::new(),
            runtime_pkg: "pyths-runtime",
            known_functions: HashSet::new(),
            loop_flag_stack: Vec::new(),
            loop_flag_counter: 0,
            asyncio_namespaces: HashSet::new(),
            module_namespaces: HashSet::new(),
            module_doc: None,
            dataclass_field_stmts: HashMap::new(),
            self_lowering: SelfLowering::Ordinary,
            module_has_dunder_call: false,
            star_import_bindings: HashMap::new(),
            in_call_callee: false,
            class_attr_subst: None,
            datetime_namespaces: HashSet::new(),
            asyncio_run_fns: HashSet::new(),
            import_module_fns: HashSet::new(),
            importlib_namespaces: HashSet::new(),
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
            scope_bindings: vec![HashSet::new()], // module scope (filled in emit_module)
            module_bound_names: HashSet::new(),
            module_idents: HashSet::new(),
            scope_globals: vec![HashSet::new()],
            dotted_import_scopes: vec![DottedImportScope::default()], // module scope
            hoisted_scopes: vec![HashSet::new()],  // module scope
            sentinel_scopes: vec![HashSet::new()], // module scope
            imported_bindings: HashSet::new(),
            imported_binding_modules: HashMap::new(),
            import_ref_renames: HashMap::new(),
            aliased_import_identities: HashMap::new(),
            hoisted_alias_module: HashMap::new(),
            import_rename_counter: 0,
            scope_import_decls: vec![HashMap::new()], // module scope
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
            emitted_class_names: HashSet::new(),
            local_module_imports: HashSet::new(),
            float_returning_functions: HashSet::new(),
            pydict_forced_locals: HashSet::new(),
            force_pydict_literal: false,
            codegen_errors: Vec::new(),
            certificate: crate::cert::Certificate::default(),
            react_imports: HashSet::new(),
            react_lib_bindings: HashSet::new(),
            react_lib_module_aliases: HashSet::new(),
            react_namespace_alias_modules: HashMap::new(),
            react_member_component_bases: HashSet::new(),
            react_create_element_fns: HashSet::new(),
            npm_imports: HashMap::new(),
            react_refresh: false,
            class_stack: Vec::new(),
            runtime_pkg: "pyths-runtime",
            known_functions: HashSet::new(),
            loop_flag_stack: Vec::new(),
            loop_flag_counter: 0,
            asyncio_namespaces: HashSet::new(),
            module_namespaces: HashSet::new(),
            module_doc: None,
            dataclass_field_stmts: HashMap::new(),
            self_lowering: SelfLowering::Ordinary,
            module_has_dunder_call: false,
            star_import_bindings: HashMap::new(),
            in_call_callee: false,
            class_attr_subst: None,
            datetime_namespaces: HashSet::new(),
            asyncio_run_fns: HashSet::new(),
            import_module_fns: HashSet::new(),
            importlib_namespaces: HashSet::new(),
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
        // #439: a WASM-routed function whose name is a JS reserved word
        // (`def default(...)`) is exported by the glue under its sanitized
        // identifier (`default$`) — `export function default` is a SyntaxError,
        // and `import { default } from …` an invalid binding. Sanitize the
        // re-export names IDENTICALLY here so the import binds the glue's real
        // export and matches this module's call sites (which also go through
        // `sanitize_ident`).
        let joined = names
            .iter()
            .map(|s| Self::sanitize_ident(s).into_owned())
            .collect::<Vec<_>>()
            .join(", ");
        // SECURITY (#4): `glue_filename` is source/output-stem-derived. A `"`
        // or newline in it would break out of the import specifier string.
        // Encode it. `joined` is the sorted set of WASM-skipped function names
        // (parser identifiers), which cannot contain a quote.
        self.write(&format!(
            "\nimport {{ {} }} from {};\n",
            joined,
            js_string_literal(glue_filename)
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
        // Bug-1 (aliasing soundness): the in-place aug-assign helpers fall
        // back to their binary value helpers for immutable targets — force
        // the binary helper (and thus its hand-inline mirror + dep chain)
        // in whenever the pyI* wrapper is needed. The wrapper bodies
        // themselves come from the #170 extraction (canonical operators.js).
        for (iop, vop) in [
            ("pyIAdd", "pyAdd"),
            ("pyISub", "pySub"),
            ("pyIMul", "pyMul"),
            ("pyIBitOr", "pyBitOr"),
            ("pyIBitAnd", "pyBitAnd"),
            ("pyIBitXor", "pyBitXor"),
        ] {
            if needed.contains(iop) {
                needed.insert(vop.to_string());
            }
        }
        if needed.contains("pyPrint") {
            needed.insert("pyStr".to_string());
        }
        // public #3: pyGetItem dispatches a slice-object key (`xs[slice(1,3)]`)
        // through pySlice — the hand-written inline copy needs the extracted
        // canonical pySlice present to link against.
        if needed.contains("pyGetItem") {
            needed.insert("pySlice".to_string());
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
        // Option-B spike: the inline arith mirror (PY_ARITH_JS) constructs
        // boxed floats via __pyF — pull the canonical __pyF + PyFloat from
        // operators.js whenever any arith helper (or float()) is present.
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
            "pyFloat",
            "pyRound",
            // Option B: the inline PyComplex block boxes __abs__ results via
            // __pyF, and pyBoundMethod re-tags .real/.imag through it.
            "pyComplex",
            "pyBoundMethod",
        ]
        .iter()
        .any(|h| needed.contains(*h))
        {
            needed.insert("__pyF".to_string());
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
        // SEC-7 (CWE-1321): every plain-object dict/kwargs write goes through
        // the proto-safe __pyDictWrite primitive, so pull it in whenever any
        // of its consumers is inlined. Mirrors runtime/src/runtime.js.
        if needed.contains("pySetItem")
            || needed.contains("pyDictSetdefault")
            || needed.contains("pyUpdate")
            || needed.contains("pyDictMerge")
            || needed.contains("__pyCallKw")
            || needed.contains("__pyKwArgs")
        {
            needed.insert("__pyDictWrite".to_string());
        }
        // delta4: EVERY plain-dict subscript/probe op shares the coerce-once
        // key primitive — read/delete/write/probe must all agree on ONE
        // coercion of the key (Symbol.toPrimitive double-coercion invariant).
        if needed.contains("__pyDictWrite")
            || needed.contains("pyGetItem")
            || needed.contains("pyDelItem")
            || needed.contains("pyPop")
            || needed.contains("pyDictGet")
            || needed.contains("pyDictSetdefault")
            || needed.contains("pyContains")
        {
            needed.insert("__pyPropKey".to_string());
        }
        let needed = &needed;

        let mut rt = String::new();
        rt.push_str("// --- PythScribe Runtime (inlined) ---\n");

        if needed.contains("__pyPropKey") {
            // Mirrors runtime/src/runtime.js __pyPropKey — the SINGLE
            // coerce-once dict-key primitive (see that copy's doc comment).
            // delta4: full ToPropertyKey — a Symbol.toPrimitive returning a
            // Symbol passes through (String() would throw); the computed-
            // property position applies the spec coercion EXACTLY ONCE.
            rt.push_str(
                r#"function __pyPropKey(k) {
    if (typeof k === "symbol") return k;
    if ((typeof k === "object" && k !== null) || typeof k === "function") return Reflect.ownKeys({ [k]: 0 })[0];
    return String(k);
}
"#,
            );
        }
        if needed.contains("__pyDictWrite") {
            // Mirrors runtime/src/runtime.js __pyDictWrite (SEC-7). `o[k] = v`
            // with k === "__proto__" invokes the inherited Object.prototype
            // setter and reparents `o` instead of storing a key — prototype
            // pollution, and a Python-semantics break (`"__proto__" in d`).
            rt.push_str(
                r#"function __pyDictWrite(o, k, v) {
    const pk = __pyPropKey(k); // R1: compare the coerced property key
    if (pk === "__proto__") { Object.defineProperty(o, "__proto__", { value: v, writable: true, enumerable: true, configurable: true }); return; }
    o[pk] = v; // R1: write the ALREADY-coerced key so o[k] cannot re-coerce (Symbol.toPrimitive)
}
"#,
            );
        }

        if needed.contains("pyRange") || needed.contains("__pyRangeIter") {
            // ROOT FIX (mirrors runtime/src/runtime.js): ONE guard/normalize
            // source (__pyRangeNorm/__pyRangeLen) shared by the materializing
            // `pyRange` AND the lazy `__pyRangeIter` that the optimized
            // `for i in range(...)` loop iterates — so the fast path can never
            // diverge (bool ok; float args rejected; BigInt/2**53-safe counted
            // stepping; no hang).
            rt.push_str(
                r#"function __pyRangeNorm(startOrStop, stop, step) {
    const __b = (v) => (typeof v === "boolean" ? (v ? 1 : 0) : v);
    startOrStop = __b(startOrStop); stop = __b(stop); step = __b(step);
    let start;
    if (stop === undefined) { start = 0; stop = startOrStop; step = 1; }
    else { start = startOrStop; if (step === undefined || step === null) step = 1; else if (step === 0 || step === 0n) { const e = new Error("range() arg 3 must not be zero"); e.name = "ValueError"; throw e; } }
    const __numOrBig = (v) => typeof v === "number" || typeof v === "bigint";
    for (const v of [start, stop, step]) { if (v != null && v.__pyfloat__ === true) { const e = new Error("'float' object cannot be interpreted as an integer"); e.name = "TypeError"; throw e; } }
    if (!__numOrBig(start) || !__numOrBig(stop) || !__numOrBig(step)) { const bad = !__numOrBig(start) ? start : !__numOrBig(stop) ? stop : step; const tn = bad === null || bad === undefined ? "NoneType" : typeof bad === "string" ? "str" : Array.isArray(bad) ? (bad.__pytuple__ ? "tuple" : "list") : typeof bad === "object" ? "dict" : typeof bad; const e = new Error("'" + tn + "' object cannot be interpreted as an integer"); e.name = "TypeError"; throw e; }
    for (const v of [start, stop, step]) { if (typeof v === "number" && !Number.isInteger(v)) { const e = new Error("'float' object cannot be interpreted as an integer"); e.name = "TypeError"; throw e; } }
    return { start, stop, step };
}
function __pyRangeLen(start, stop, step) {
    const bs = BigInt(start), bt = BigInt(stop), bp = BigInt(step);
    return bp > 0n ? (bt > bs ? (bt - bs + bp - 1n) / bp : 0n) : (bs > bt ? (bs - bt + (-bp) - 1n) / (-bp) : 0n);
}
const __MAX_SAFE_BIG = 9007199254740991n;
function __pyRangeUseBig(start, stop, step, bs, bp, len) {
    if (typeof start === "bigint" || typeof stop === "bigint" || typeof step === "bigint") return true;
    if (len === 0n) return false;
    const last = bs + (len - 1n) * bp; const abs = (x) => (x < 0n ? -x : x);
    return abs(bs) > __MAX_SAFE_BIG || abs(last) > __MAX_SAFE_BIG || abs(last - bs) > __MAX_SAFE_BIG; // delta4: the Number loop's INTERMEDIATE i*step reaches |last-start|
}
function* __pyRangeIter(startOrStop, stop, step) {
    const n = __pyRangeNorm(startOrStop, stop, step);
    const len = __pyRangeLen(n.start, n.stop, n.step);
    const bs = BigInt(n.start), bp = BigInt(n.step);
    if (__pyRangeUseBig(n.start, n.stop, n.step, bs, bp, len)) { let v = bs; for (let c = 0n; c < len; c++, v += bp) yield v; }
    else { const count = Number(len); let v = n.start; for (let i = 0; i < count; i++, v += n.step) yield v; }
}
function pyRange(startOrStop, stop, step) {
    const n = __pyRangeNorm(startOrStop, stop, step);
    const len = __pyRangeLen(n.start, n.stop, n.step);
    if (len > 4294967295n) { const e = new Error("range() result has too many items"); e.name = "OverflowError"; throw e; }
    const result = [];
    const bs = BigInt(n.start), bp = BigInt(n.step);
    if (__pyRangeUseBig(n.start, n.stop, n.step, bs, bp, len)) { for (let i = 0n; i < len; i++) result.push(bs + i * bp); }
    else { const count = Number(len); for (let i = 0; i < count; i++) result.push(n.start + i * n.step); }
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
        return __pyOwnKeys(it)[Symbol.iterator](); // r6: symbol keys iterate too
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
        // WF-1: the hand-written inline __pyEffect mirror was DELETED — the
        // #170 extraction pulls the canonical package __pyEffect (and its
        // spread-form sibling __pyEffectArgs) from runtime/src/runtime.js.
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
    const __wasF = x != null && x.__pyfloat__ === true;
    if (__wasF) x = x.valueOf();
    if (typeof x === "bigint") {
        const nd = ndigits == null ? 0 : Math.trunc(Number(ndigits));
        return nd >= 0 ? x : __roundBigNeg(x, -nd);
    }
    if (typeof x === "number" && !isFinite(x)) {
        if (ndigits == null) {
            if (Number.isNaN(x)) throw new ValueError("cannot convert float NaN to integer");
            throw new OverflowError("cannot convert float infinity to integer");
        }
        return __pyF(x);
    }
    if (x == null || typeof x !== "number") {
        throw new TypeError("type cannot be interpreted as a number");
    }
    const __reF = ndigits != null && (__wasF || !Number.isInteger(x));
    const nd = ndigits == null ? 0 : Math.trunc(ndigits);
    const factor = Math.pow(10, nd);
    if (factor === 0) return __reF ? __pyF(x < 0 ? -0 : 0) : (x < 0 ? -0 : 0);
    if (!isFinite(factor)) return __reF ? __pyF(x) : x;
    const scaled = x * factor;
    if (!isFinite(scaled)) return __reF ? __pyF(x) : x;
    const floor = Math.floor(scaled);
    const diff = scaled - floor;
    let rounded;
    if (diff > 0.5) rounded = floor + 1;
    else if (diff < 0.5) rounded = floor;
    else rounded = floor % 2 === 0 ? floor : floor + 1;
    const result = rounded / factor;
    return __reF ? __pyF(result) : result;
}
"#,
            );
        }
        if needed.contains("pyLen") {
            rt.push_str(
                r#"function pyLen(obj) {
    if (obj == null) throw new TypeError("object of type 'NoneType' has no len()");
    if (typeof obj === "number" || typeof obj === "bigint" || typeof obj === "boolean" || obj.__pyfloat__ === true) {
        throw new TypeError(`object of type '${__pyTypeName(obj)}' has no len()`); // #467: one type-name source
    }
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
    return __pyOwnKeys(obj).length; // r6: symbol-keyed entries count
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
        // pyBool / pyAnd / pyOr / pyAny / pyAll: intentionally NOT hand-written
        // here (bytes-completeness root fix). The hand-inlined pyBool copy
        // drifted from the canonical runtime/src/types.js one — it had no
        // bytes/bytearray branch, so `bool(b"")` stayed truthy under
        // `pyths run` even after the package runtime was fixed (#457). All
        // five now flow through the #170 extraction fallback below, which
        // pulls the canonical types.js definitions with their transitive
        // dependencies (pyAnd/pyOr/pyAny/pyAll pull pyBool automatically) —
        // one source of truth for Python truthiness, like pySlice/pySetSlice.
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
        else if (el != null && el.__pyfloat__ === true) s += "n:" + String(el.valueOf()) + ";";
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
    if (k != null && k.__pyfloat__ === true) return k.valueOf();
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
            else { for (const k of __pyOwnKeys(src)) this.set(k, src[k]); } // r6: symbols survive
        }
    }
    set(k, v) {
        const c = __pyKey(k);
        if ((typeof k === "boolean" || Array.isArray(k) || (k != null && k.__pyfloat__ === true)) && !super.has(c)) {
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
        // autotester dictionaries: the hand-written inline pyDict mirror was
        // DELETED (drift trap — it lacked the CPython update-sequence element
        // validation). The #170 extraction pulls the canonical runtime pyDict
        // with its exception-class deps.
        // 0.2.2 hold blocker 2: the hand-written inline pySetItem / pyDelItem
        // mirrors were DELETED — 1b28bae5 fixed their error KINDS (non-integer
        // list index → TypeError, del on a tuple → TypeError instead of a
        // SILENT SPLICE) in the package runtime only, and the stale inline
        // copies shipped the pre-fix behavior in `pyths run`/`bundle`/`test`
        // (C4 wrong-kind + silent tuple mutation). They now flow through the
        // #170 extraction, which pulls the canonical runtime/src/runtime.js
        // definitions with transitive deps — inline == package by construction
        // (same migration as pyDictGet/pyUpdate/pyContains). The
        // inline_runtime_parity battery covers the error-kind cases the fix
        // changed, so the pre-fix shape can no longer pass the gate.
        if needed.contains("pyDictKeys") {
            rt.push_str(r#"function pyDictKeys(d) {
    if (d == null) { const e = new Error("'NoneType' object is not iterable"); e.name = "TypeError"; throw e; }
    if (!Array.isArray(d) && typeof d.keys === "function") return [...d.keys()];
    return __pyOwnKeys(d); // r6: symbol keys listed too
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
    if (x.__pyfloat__ === true) { const e = new Error("'float' object is not iterable"); e.name = "TypeError"; throw e; }
    if (typeof x === "object") return __pyOwnKeys(x); // r6: symbols too
    const e = new Error("'" + __pyTypeName(x) + "' object is not iterable"); e.name = "TypeError"; throw e; // #467
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
    if (it.__pyfloat__ === true) { const e = new Error("'float' object is not iterable"); e.name = "TypeError"; throw e; }
    if (typeof it === "object") return __pyOwnKeys(it); // r6: symbols too
    const e = new Error("'" + __pyTypeName(it) + "' object is not iterable"); e.name = "TypeError"; throw e; // #467
}
"#);
        }
        if needed.contains("pyDictValues") {
            rt.push_str(r#"function pyDictValues(d) {
    if (d == null) { const e = new Error("'NoneType' object is not iterable"); e.name = "TypeError"; throw e; }
    if (!Array.isArray(d) && typeof d.values === "function") return [...d.values()];
    return __pyOwnKeys(d).map((k) => d[k]); // r6: symbol-keyed values included
}
"#);
        }
        // WB-20/WB-6: the hand-written inline pyDictItems / pyDictGet /
        // pyDictSetdefault mirrors were DELETED — they drifted from the
        // canonical package copies (pyDictGet shipped WITHOUT the WB-20
        // unconditional-read MobX fix in `pyths run`/`bundle`; pyDictItems
        // and pyDictSetdefault lacked the WB-6 user-method dispatch). They
        // now flow through the #170 extraction fallback, which pulls the
        // canonical runtime/src/runtime.js definitions with transitive deps
        // — inline == package by construction (same migration as
        // pyFormatSpec and the exception classes). The
        // inline_runtime_parity gate enforces this class-wide.
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
    const keys = __pyOwnKeys(d); // r6: a symbol-keyed last entry pops correctly
    if (keys.length === 0) { const e = new Error("popitem(): dictionary is empty"); e.name = "KeyError"; throw e; }
    const k = keys[keys.length - 1]; const v = d[k]; delete d[k];
    return pyTuple(k, v);
}
"#);
        }
        // pyPop: migrated to the #170 extraction (0.2.2 hold item 3) — the
        // canonical package copy gained the receiver-class guards (None/set/
        // int/float/bool/str/bytes no longer fall into the dict-pop path with
        // a wrong-kind KeyError; set.pop() is a real method) and keeping a
        // hand mirror in sync would be exactly the drift class blocker 2
        // closed. Inline == package by construction.
        // WB-6: the hand-written inline pyUpdate mirror was DELETED — it
        // drifted (no pairs-iterable form, weaker error surface). The #170
        // extraction now pulls the canonical package pyUpdate with its deps
        // (__pyUpdatePairs/__pyUpdatePairShape/__pyDictWrite).
        // WB-6: the hand-written inline pyClear / pyCopy mirrors were DELETED
        // (same migration as pyUpdate/pyDictGet above) — the #170 extraction
        // pulls the canonical package copies, which propagate a user
        // receiver's method return value and share one __isPlainObj rule.
        if needed.contains("pyDictMerge") {
            // Mirrors runtime/src/runtime.js pyDictMerge (#83).
            rt.push_str(r#"function pyDictMerge(...parts) {
    if (parts.some((p) => p instanceof Map)) {
        const out = new PyDict();
        for (const p of parts) {
            if (p == null) continue;
            if (p instanceof Map) { for (const [k, v] of p.entries()) out.set(k, v); }
            else { for (const k of __pyOwnKeys(p)) out.set(k, p[k]); } // delta4: symbols survive
        }
        return out;
    }
    const out = {};
    for (const p of parts) {
        if (p == null) continue;
        for (const k of __pyOwnKeys(p)) __pyDictWrite(out, k, p[k]); // SEC-7 + delta4
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
    if (n != null && n.__pyfloat__ === true) n = n.valueOf();
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
const __pyAddrMap = new WeakMap();
let __pyAddrNext = 0x7f6c00000000;
function __pyObjAddr(obj) {
    let a = __pyAddrMap.get(obj);
    if (a === undefined) { a = __pyAddrNext; __pyAddrNext += 0x40; __pyAddrMap.set(obj, a); }
    return a.toString(16).padStart(12, "0");
}
function pyRepr(obj) {
    if (obj === null || obj === undefined) return "None";
    if (typeof obj === "boolean") return obj ? "True" : "False";
    if (obj.__pyfloat__ === true) return pyFormatFloat(obj.valueOf());
    if (typeof obj === "object" && typeof obj.__repr__ === "function") return obj.__repr__();
    if (typeof obj === "bigint") return obj.toString();
    if (typeof obj === "number") { if (Number.isInteger(obj) && Math.abs(obj) <= Number.MAX_SAFE_INTEGER) return String(obj); if (Number.isInteger(obj)) return BigInt(obj).toString(); return pyFormatFloat(obj); }
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
    if (typeof obj === "object") {
        const proto = Object.getPrototypeOf(obj);
        if (proto !== Object.prototype && proto !== null) {
            const ctor = obj.constructor;
            const nm = (ctor && (ctor.__name__ || ctor.name)) || "object";
            const mod = (ctor && ctor.__module__) || "__main__";
            return `<${mod}.${nm} object at 0x${__pyObjAddr(obj)}>`;
        }
        const parts = []; for (const k of __pyOwnKeys(obj)) parts.push(`${pyRepr(k)}: ${pyRepr(obj[k])}`); return `{${parts.join(", ")}}`; // r6: symbol entries shown
    }
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
    if (obj != null && obj.__pyfloat__ === true) return pyFormatFloat(obj.valueOf());
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
        // pyEq is no longer hand-inlined here: the canonical
        // runtime/src/operators.js definition is pulled by the #170 extractor
        // with its transitive deps (__isPlainObject, __pyOwnKeys), so the two
        // copies cannot drift. The old inline copy was exactly such a drift —
        // it lacked the S1 bound-method-equality branch (a.m == a.m compares
        // __pyboundfunc__/__pyboundself__ tags), so `pyths run` printed False
        // where `pyths compile` + the package runtime printed True. Same
        // de-inline pattern as pyType / pyFormatSpec / pyDict below.
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
    if (a instanceof Set && b instanceof Set) { if (a.size >= b.size) return false; for (const x of a) { if (!b.has(x)) return false; } return true; }
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
    if (a instanceof Set && b instanceof Set) { for (const x of a) { if (!b.has(x)) return false; } return true; }
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
    if (a instanceof Set && b instanceof Set) { if (b.size >= a.size) return false; for (const x of b) { if (!a.has(x)) return false; } return true; }
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
    if (a instanceof Set && b instanceof Set) { for (const x of b) { if (!a.has(x)) return false; } return true; }
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
    // Option B: a boxed (integer-valued) float coerces by value — this is
    // THE complex-coercion authority behind complex() and every PyComplex
    // dunder, so one unbox here covers complex(8.0), 8.0+2j, abs(3.0+4.0j).
    // (Mirrors the cmath.__c fix; keep both coercion authorities in sync.)
    if (o != null && o.__pyfloat__ === true) return new PyComplex(o.valueOf(), 0);
    if (typeof o === "bigint") return new PyComplex(Number(o), 0);
    if (typeof o === "boolean") return new PyComplex(o ? 1 : 0, 0);
    // Cross-copy interop: a complex-shaped object from the OTHER runtime
    // copy (package stdlib cmath vs the inline program runtime) converts.
    if (o !== null && typeof o === "object" && typeof o.real === "number" && typeof o.imag === "number") return new PyComplex(o.real, o.imag);
    return null;
}
class PyComplex {
    constructor(re, im) {
        // Option B: a boxed (integer-valued) float component unboxes ONCE
        // here — the constructor is the single entry for complex(), the
        // dunders (via __toComplex) and cross-copy coercion, so internals
        // (repr's `re === 0` check, the arithmetic) always see natives.
        this.real = re != null && re.__pyfloat__ === true ? re.valueOf() : re;
        this.imag = im != null && im.__pyfloat__ === true ? im.valueOf() : im;
    }
    // Brand for the attribute-read path: complex .real/.imag are FLOATS in
    // CPython (pyBoundMethod re-tags them via __pyF on the way out).
    get __pycomplex__() { return true; }
    __add__(o) { const c = __toComplex(o); return c ? new PyComplex(this.real + c.real, this.imag + c.imag) : undefined; }
    __radd__(o) { const c = __toComplex(o); return c ? new PyComplex(c.real + this.real, c.imag + this.imag) : undefined; }
    __sub__(o) { const c = __toComplex(o); return c ? new PyComplex(this.real - c.real, this.imag - c.imag) : undefined; }
    __rsub__(o) { const c = __toComplex(o); return c ? new PyComplex(c.real - this.real, c.imag - this.imag) : undefined; }
    __mul__(o) { const c = __toComplex(o); return c ? new PyComplex(this.real * c.real - this.imag * c.imag, this.real * c.imag + this.imag * c.real) : undefined; }
    __rmul__(o) { const c = __toComplex(o); return c ? new PyComplex(c.real * this.real - c.imag * this.imag, c.real * this.imag + c.imag * this.real) : undefined; }
    __neg__() { return new PyComplex(-this.real, -this.imag); }
    __pos__() { return new PyComplex(this.real, this.imag); }
    // CPython: abs(complex) is a FLOAT — abs(3.0+4.0j) is 5.0, not int 5.
    __abs__() { return __pyF(Math.hypot(this.real, this.imag)); }
    __eq__(o) { const c = __toComplex(o); return c ? (this.real === c.real && this.imag === c.imag) : false; }
    conjugate() { return new PyComplex(this.real, -this.imag); }
    __truediv__(o) {
        const c = __toComplex(o); if (!c) return undefined;
        const d = c.real * c.real + c.imag * c.imag;
        return new PyComplex((this.real * c.real + this.imag * c.imag) / d, (this.imag * c.real - this.real * c.imag) / d);
    }
    __rtruediv__(o) { const c = __toComplex(o); return c ? c.__truediv__(this) : undefined; }
    __pow__(o) {
        const c = __toComplex(o); if (!c) return undefined;
        const r = Math.hypot(this.real, this.imag);
        if (r === 0) return new PyComplex(0, 0);
        const lnRe = Math.log(r), lnIm = Math.atan2(this.imag, this.real);
        const eRe = c.real * lnRe - c.imag * lnIm;
        const eIm = c.real * lnIm + c.imag * lnRe;
        const m = Math.exp(eRe);
        return new PyComplex(m * Math.cos(eIm), m * Math.sin(eIm));
    }
    __rpow__(o) { const c = __toComplex(o); return c ? c.__pow__(this) : undefined; }
    __repr__() { return __complexRepr(this.real, this.imag); }
    __str__() { return __complexRepr(this.real, this.imag); }
}
function pyComplex(re, im) { return new PyComplex(re, im); }
"#);
        }
        // WB-20 analogue: the hand-written inline pyContains mirror was
        // DELETED — its plain-object membership probe lacked the
        // unconditional read that registers a host read-trap dependency
        // (MobX `k in d` tracking), the exact drift class pyDictGet had.
        // The #170 extraction pulls the canonical package pyContains.
        // pyType (and the interned type-object singletons it returns) is no
        // longer hand-inlined here: the canonical runtime/src/runtime.js
        // definitions — now the CALLABLE __pyType* singletons shared with
        // value-position builtin type names — are pulled by the #170
        // extractor with their transitive deps, so the two copies cannot
        // drift (the old inline copy was exactly such a drift).
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
    if (key && key.__pyslice__ === true) {
        if (typeof obj === "string" || Array.isArray(obj) || typeof obj.__getitem__ === "function") return pySlice(obj, key.start, key.stop, key.step);
        const e = new Error("unhashable type: 'slice'"); e.name = "TypeError"; throw e;
    }
    if (typeof obj === "number" || typeof obj === "bigint" || typeof obj === "boolean" || obj.__pyfloat__ === true) { const e = new Error("'" + __pyTypeName(obj) + "' object is not subscriptable"); e.name = "TypeError"; throw e; } // #467: one type-name source
    if (obj instanceof Set) { const e = new Error("'set' object is not subscriptable"); e.name = "TypeError"; throw e; }
    const __sn = typeof obj === "string" ? "string" : (Array.isArray(obj) ? (obj.__pytuple__ ? "tuple" : "list") : "sequence"); // centralized seq name (delta)
    if (typeof key === "boolean") key = key ? 1 : 0;
    if (typeof key === "bigint" && (typeof obj === "string" || Array.isArray(obj))) {
        if (key >= -9007199254740991n && key <= 9007199254740991n) key = Number(key);
        else { const e = new Error(__sn + " index out of range"); e.name = "IndexError"; throw e; }
    }
    if ((typeof obj === "string" || Array.isArray(obj)) && typeof key === "number" && !Number.isInteger(key)) { const e = new Error(__sn === "string" ? "string indices must be integers" : (__sn + " indices must be integers or slices, not float")); e.name = "TypeError"; throw e; }
    if ((typeof obj === "string" || Array.isArray(obj)) && typeof key !== "number" && typeof key !== "string") { const tn = key === null || key === undefined ? "NoneType" : key.__pyfloat__ === true ? "float" : Array.isArray(key) ? (key.__pytuple__ ? "tuple" : "list") : key instanceof Map ? "dict" : key instanceof Set ? "set" : typeof key === "object" ? "dict" : typeof key; const e = new Error(__sn === "string" ? ("string indices must be integers, not '" + tn + "'") : (__sn + " indices must be integers or slices, not " + tn)); e.name = "TypeError"; throw e; }
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
        if (typeof key === "string" || (typeof key === "number" && !Number.isInteger(key))) { const e = new Error(__sn + " indices must be integers or slices, not " + (typeof key === "string" ? "str" : "float")); e.name = "TypeError"; throw e; }
        const n = obj.length; let i = key; if (i < 0) i += n;
        if (i < 0 || i >= n) { const e = new Error(__sn + " index out of range"); e.name = "IndexError"; throw e; }
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
    const pk = __pyPropKey(key); // delta4: presence-check and read agree on ONE coercion
    if (!Object.prototype.hasOwnProperty.call(obj, pk)) {
        const e = new Error(typeof key === "string" ? `'${key}'` : String(pk)); e.name = "KeyError"; throw e;
    }
    return obj[pk];
}
"#);
        }
        if needed.contains("pyFloat") {
            rt.push_str(r#"function pyFloat(x) {
    if (typeof x === "boolean") return __pyF(x ? 1 : 0);
    if (typeof x === "number") return __pyF(x);
    if (x != null && x.__pyfloat__ === true) return x;
    if (typeof x === "bigint") { const n = Number(x); if (!isFinite(n)) { const e = new Error("int too large to convert to float"); e.name = "OverflowError"; throw e; } return __pyF(n); }
    if (typeof x === "string") {
        const t = x.trim();
        const m = /^([+-]?)(inf|infinity|nan)$/i.exec(t);
        if (m) { if (m[2].toLowerCase() === "nan") return __pyF(NaN); return __pyF(m[1] === "-" ? -Infinity : Infinity); }
        let t2 = t;
        if (t.indexOf("_") !== -1) {
            const isDig = (c) => c >= 48 && c <= 57;
            for (let i = 0; i < t.length; i++) {
                if (t.charCodeAt(i) === 95 && !(isDig(t.charCodeAt(i - 1)) && isDig(t.charCodeAt(i + 1)))) { const e = new Error(`could not convert string to float: '${x}'`); e.name = "ValueError"; throw e; }
            }
            t2 = t.replace(/_/g, "");
        }
        if (t2 === "" || !/^[+-]?(\d+\.?\d*|\.\d+)([eE][+-]?\d+)?$/.test(t2)) { const e = new Error(`could not convert string to float: '${x}'`); e.name = "ValueError"; throw e; }
        return __pyF(Number(t2));
    }
    return __pyF(Number(x));
}
"#);
        }
        if needed.contains("pyIter") {
            // Mirrors runtime/src/runtime.js pyIter.
            rt.push_str(r#"function pyIter(obj) {
    if (obj == null) { const e = new Error("'NoneType' object is not iterable"); e.name = "TypeError"; throw e; }
    if (typeof obj[Symbol.iterator] === "function") return obj[Symbol.iterator]();
    if (typeof obj.__iter__ === "function") return obj.__iter__();
    if (obj.__pyfloat__ === true) { const e = new Error("'float' object is not iterable"); e.name = "TypeError"; throw e; }
    const e = new Error("'" + __pyTypeName(obj) + "' object is not iterable"); e.name = "TypeError"; throw e; // #467
}
"#);
        }
        // autotester arguments/decorators: the hand-written inline
        // __pyKwArgs/__pyCallKw mirror was DELETED (drift trap — it lacked
        // the __pyva__ variadic keyword-carrier path). The #170 extraction
        // pulls the canonical runtime copies with their deps (__pyMarkKw,
        // __PYKW_MARK, __pyDictWrite, ...).
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
        const t = Math.trunc(x);
        return Number.isSafeInteger(t) ? t : BigInt(t);
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
    const t = Math.trunc(Number(x));
    return Number.isSafeInteger(t) || !Number.isFinite(t) ? t : BigInt(t);
}
"#);
        }
        // pyDelItem: migrated to the #170 extraction with pySetItem (see the
        // blocker-2 comment above) — the canonical package copy carries the
        // 1b28bae5 error-kind fixes (TypeError for non-integer index / tuple /
        // non-subscriptable receivers) the inline mirror lacked.
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
        // autotester module_builtin: the hand-written inline __pyMinmax/pyMin/
        // pyMax mirror was DELETED (drift trap — it kept the pre-fix no-arg
        // ValueError). The #170 extraction pulls the canonical runtime copy.
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
"#);
            // #469: the old local __bitTypeName copy here lacked the
            // __pyfloat__ arm — a boxed integer-valued float leaked 'PyFloat'
            // in bit-op messages while `+` said 'float' for the SAME value
            // (and function/class/plain-object leaked 'Function'/'Object').
            // Bit messages now reference __pyTypeName, extracted ON USE from
            // the canonical runtime.js — one computer, no double-declare
            // (extraction dedupes), no drift by construction.
            if needed.contains("pyBitOr")
                || needed.contains("pyBitAnd")
                || needed.contains("pyBitXor")
            {
                rt.push_str(r#"function __reqBitInt(op, a, b, fctx) {
    fctx = fctx || 0;
    if (fctx || !__bitIntOk(a) || !__bitIntOk(b)) { const an = (fctx & 1) ? "float" : __pyTypeName(a); const bn = (fctx & 2) ? "float" : __pyTypeName(b); const e = new Error(`unsupported operand type(s) for ${op}: '${an}' and '${bn}'`); e.name = "TypeError"; throw e; }
}
function pyBitOr(a, b, fctx) {
    if (a instanceof Set && b instanceof Set) { const out = new (a.constructor)(a); for (const v of b) out.add(v); return out; }
    if (a != null && typeof a.__or__ === "function") return a.__or__(b);
    if (b != null && typeof b.__ror__ === "function") return b.__ror__(a);
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
    if (b != null && typeof b.__rand__ === "function") return b.__rand__(a);
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
    if (b != null && typeof b.__rxor__ === "function") return b.__rxor__(a);
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
    if (fctx || !__bitIntOk(a)) { const an = fctx ? "float" : __pyTypeName(a); const e = new Error(`bad operand type for unary ~: '${an}'`); e.name = "TypeError"; throw e; }
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
        items = __pyOwnKeys(iterable); // r6: symbol keys included
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

        // autotester string_format: the hand-written inline pyFormatSpec mirror
        // was DELETED (it drifted from the canonical runtime — the exact
        // whack-a-mole the #170 extraction fallback exists to prevent).
        // pyFormatSpec is now extracted from runtime/src/runtime.js on use;
        // its deps (__fixedHalfEven, pyFormatFloat) are preloaded above and
        // still hand-written inline, so the extractor links against them.

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

        // Bytes dispatch authority: __pyBytesKind is referenced by hand-written
        // blocks the extractor does not scan (PY_OBJECT_MODEL_JS's
        // __pyIsInstance bytes/bytearray cases). Emit it ON USE by scanning the
        // assembled runtime — same discipline as __pyOwnKeys below — but pull
        // the CANONICAL runtime.js definition via the extractor rather than a
        // second hand copy, so the recognizer can never drift.
        if rt.contains("__pyBytesKind(") && !Self::inline_defines(&rt, "__pyBytesKind") {
            let mut extra: HashSet<String> = HashSet::new();
            extra.insert("__pyBytesKind".to_string());
            Self::append_extracted_helpers(&extra, &mut rt);
        }

        // #467: __pyTypeName is THE value-model type-name source for error
        // messages ("'float' object is not iterable", …). Hand-written inline
        // mirrors (pySeq/pyForIter/pyIter/pyGetItem/pyLen) reference it, and
        // the extractor does not scan hand blocks — emit it ON USE (same
        // discipline as __pyBytesKind above), pulling the CANONICAL runtime.js
        // definition (with pyType + the interned type singletons as transitive
        // deps) so inline `pyths run` names types exactly like the package.
        if rt.contains("__pyTypeName(") && !Self::inline_defines(&rt, "__pyTypeName") {
            let mut extra: HashSet<String> = HashSet::new();
            extra.insert("__pyTypeName".to_string());
            Self::append_extracted_helpers(&extra, &mut rt);
        }

        // delta4 round-6: __pyOwnKeys (own enumerable string+symbol keys) is
        // referenced by MANY dict-family blocks — len/bool/eq/keys/values/
        // items/popitem/clear/iteration/repr/merge/update/dict()/PyDict.
        // Emit it ON USE, by scanning the assembled runtime, so a future
        // block that adopts it can never emit a dangling reference (a
        // per-consumer condition list is exactly what would drift). Function
        // declarations hoist, so appending at the end is order-safe; the
        // inline_defines guard avoids a duplicate when the #170 extraction
        // already pulled the canonical package copy.
        if rt.contains("__pyOwnKeys(") && !Self::inline_defines(&rt, "__pyOwnKeys") {
            rt.push_str(
                r#"function __pyOwnKeys(o) {
    const out = Object.keys(o);
    for (const s of Object.getOwnPropertySymbols(o)) { const d = Object.getOwnPropertyDescriptor(o, s); if (d && d.enumerable) out.push(s); }
    return out;
}
"#,
            );
        }

        rt.push_str("// --- End Runtime ---\n");
        rt
    }

    /// Package runtime sources embedded at build time (single source of truth
    /// for the #170 fallback). Order matters: first definition of a name wins,
    /// and runtime.js is preferred over operators.js.
    const PKG_RUNTIME_SOURCES: [&'static str; 4] = [
        include_str!("../../../runtime/src/runtime.js"),
        include_str!("../../../runtime/src/operators.js"),
        // types.js (pyBool — referenced by the __pyTypeBool singleton) and
        // classes.js (__pyIsSubclass) joined the extraction surface so the
        // inline `pyths run` path can never dangle on their helpers. Order
        // still matters: first definition of a name wins.
        include_str!("../../../runtime/src/types.js"),
        include_str!("../../../runtime/src/classes.js"),
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
            // DX-4: the shipped runtime files now end with a
            // `//# sourceMappingURL=<file>.js.map` comment (step-into
            // ignore-listing). The slicer attaches trailing lines to the LAST
            // declaration, which would smuggle a dangling map reference into
            // inline `pyths run` output — drop such lines before slicing.
            let lines: Vec<&str> = src
                .lines()
                .filter(|l| {
                    let t = l.trim_start();
                    // Drop map pragmas AND bare re-export statements
                    // (`export { PyDict } from "./runtime.js";` in types.js)
                    // — a slice that swallowed one would smuggle ESM module
                    // syntax into the inline `pyths run` script. The real
                    // definitions are their own slices.
                    !t.starts_with("//# sourceMappingURL=") && !t.starts_with("export {")
                })
                .collect();
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

    /// Like [`finish_with_sourcemap`] but returns the RAW resolved mappings
    /// (final-JS positions, prelude shift applied) instead of serialized JSON.
    /// Used by `pyths bundle --sourcemap` to compose per-module maps into one
    /// multi-source bundle map.
    pub fn finish_with_raw_map(mut self) -> (String, Vec<sourcemap::Mapping>) {
        let mut sm = self.sourcemap.take();
        let (js, body_shift, directive_len) = self.finish_certified();
        let mappings = if let Some(mut sm) = sm.take() {
            let body_start_byte = body_shift + directive_len;
            let body_start_line = js[..body_start_byte.min(js.len())].matches('\n').count() as u32;
            let directive_lines = js[..directive_len.min(js.len())].matches('\n').count() as u32;
            sm.set_line_shift(body_start_line - directive_lines, directive_lines);
            sm.resolved_mappings()
        } else {
            Vec::new()
        };
        (js, mappings)
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
        // delta4 drift guard, direction 1 of 2: the emitter must never
        // register a runtime import that is not in the checked manifest
        // (EMITTABLE_RUNTIME_SYMBOLS). Direction 2 — every manifest symbol is
        // actually EXPORTED by BOTH package entry points (index.js AND the
        // worker core.js) — is enforced by the cli_test.rs node cross-check
        // `runtime_export_surface_covers_all_emittable_symbols`. Together
        // they make "compiler emits a symbol the runtime doesn't export"
        // impossible to land: a new helper must be added to the manifest
        // (or this fires all over the debug test suite), and the manifest
        // entry fails CI until both entries export it. This is the SOLE
        // write path into `runtime_imports` — do not insert directly.
        debug_assert!(
            EMITTABLE_RUNTIME_SYMBOLS.contains(&name),
            "runtime helper `{name}` is not in EMITTABLE_RUNTIME_SYMBOLS              (emit.rs); add it there AND export it from BOTH              runtime/src/index.js and runtime/src/core.js (the export-*              surface makes that automatic once it lives in one of the four              canonical helper modules)"
        );
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
                    "bytes" => Some("bytes"),
                    "bytearray" => Some("bytearray"),
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

    /// Enter a new Python scope. `bindings` is the PRE-COMPUTED complete local
    /// binding set (issue #438) used for order-independent shadow resolution.
    fn push_scope(&mut self, bindings: HashSet<String>) {
        self.declared_scopes.push(HashSet::new());
        self.scope_bindings.push(bindings);
        // Empty by default; func/method sites populate it with their `global`
        // declarations right after pushing (see `set_scope_globals`).
        self.scope_globals.push(HashSet::new());
        self.hoisted_scopes.push(HashSet::new());
        self.sentinel_scopes.push(HashSet::new());
        self.local_types.push(HashMap::new());
        self.scope_import_decls.push(HashMap::new());
        self.dotted_import_scopes.push(DottedImportScope::default());
    }

    fn pop_scope(&mut self) {
        self.declared_scopes.pop();
        self.scope_bindings.pop();
        self.scope_globals.pop();
        self.hoisted_scopes.pop();
        self.sentinel_scopes.pop();
        self.local_types.pop();
        self.scope_import_decls.pop();
        self.dotted_import_scopes.pop();
    }

    /// Record the `global`-declared names of the just-pushed function scope
    /// (review finding 2), so a `global X` reference resolves at module scope.
    fn set_scope_globals(&mut self, globals: HashSet<String>) {
        if let Some(top) = self.scope_globals.last_mut() {
            *top = globals;
        }
    }

    /// Names declared `global` (NOT `nonlocal`) directly in this body, honoring
    /// nested control-flow but not nested def/class scopes.
    fn collect_global_declared(body: &[Stmt]) -> HashSet<String> {
        fn walk(stmts: &[Stmt], out: &mut HashSet<String>) {
            for s in stmts {
                match &s.kind {
                    StmtKind::Global(names) => {
                        for n in names {
                            out.insert(n.clone());
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
                    StmtKind::While { body, else_body, .. }
                    | StmtKind::For { body, else_body, .. } => {
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
        let mut out = HashSet::new();
        walk(body, &mut out);
        out
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

    /// PBT-2 / #452 (review blocker 2): classify a READ of a
    /// sentinel-initialized hoisted for-target, SCOPE-CHAIN aware — CPython's
    /// three distinct unbound-read behaviors depend on which scope OWNS the
    /// variable, not on where the read happens:
    ///   - `Local`: the innermost function scope owns it → UnboundLocalError
    ///     guard (`__pyChkLocal`).
    ///   - `Free`: an ENCLOSING function scope owns it (closure read,
    ///     `nonlocal` included) → free-variable NameError guard
    ///     (`__pyChkFree`).
    ///   - `Global`: module scope owns it (module-level read, a read from a
    ///     nested function with no intervening binding, or `global`-declared)
    ///     → CPython's dynamic globals → builtins chain: builtin-named reads
    ///     fall back to the builtin value, the rest raise NameError
    ///     (`__pyChkGlobal`).
    /// `None`: not a sentinel read — the name resolves to a real binding
    /// before any sentinel-owning frame is reached.
    fn sentinel_read(&self, name: &str) -> Option<SentinelRead> {
        let n = self.sentinel_scopes.len();
        // Innermost frame owns it as a sentinel → Local (Global at module).
        if self.sentinel_scopes[n - 1].contains(name) {
            return Some(if n == 1 {
                SentinelRead::Global
            } else {
                SentinelRead::Local
            });
        }
        if n == 1 {
            return None;
        }
        // `global name` in the innermost function scope resolves at module
        // scope ONLY — skip every intervening frame (same rule as
        // `is_declared_in_any_scope`, review finding 2).
        if !self.scope_globals[n - 1].contains(name) {
            // The innermost frame binds it normally → not a sentinel read.
            if self.scope_bindings[n - 1].contains(name) {
                return None;
            }
            // Walk the enclosing function-like frames outward: the first
            // frame that binds the name decides — a sentinel there is an
            // unbound FREE variable; a normal binding shadows anything
            // further out. (Comprehension frames bind only their targets and
            // never mark sentinels, so they pass through transparently.)
            for i in (1..n - 1).rev() {
                if self.sentinel_scopes[i].contains(name) {
                    return Some(SentinelRead::Free);
                }
                if self.scope_bindings[i].contains(name) {
                    return None;
                }
            }
        }
        // Module frame: a module-owned sentinel read from any depth is a
        // GLOBAL read.
        if self.sentinel_scopes[0].contains(name) {
            Some(SentinelRead::Global)
        } else {
            None
        }
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
        if !matches!(self.infer_type(test), JsInferredType::Primitive) {
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
                // float op COMPLEX is complex, not float (`8.0 + 2j`), and
                // pre-formatting a PyComplex through pyFormatFloat crashes —
                // a complex operand disqualifies the whole expression.
                if self.is_definitely_complex(left) || self.is_definitely_complex(right) {
                    return false;
                }
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
        // Option-B spike: Float excluded — a boxed 0.0 is a JS object and
        // objects are always JS-truthy, so floats route through pyBool.
        matches!(self.infer_type(expr), JsInferredType::Primitive)
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

    /// Fix A: build a from-import specifier whose BINDING is sanitized to match
    /// `sanitize_ident` at every reference site. `from m import x as default`
    /// must emit `x as default$` — a raw `default` binding is invalid JS AND
    /// mismatches the `default$` that references get. Emits the bare imported
    /// name when no rename is needed (`foo` → `foo`, `foo as bar` → `foo as
    /// bar`). `orig` is the exported name as it appears in the specifier
    /// (post snake→camel for React), `binding` the local name user code uses.
    fn import_specifier(orig: &str, binding: &str) -> String {
        let js = Self::sanitize_ident(binding);
        if js.as_ref() == orig {
            orig.to_string()
        } else {
            format!("{} as {}", orig, js)
        }
    }

    /// Is `name` bound as a local anywhere in the enclosing scope chain?
    ///
    /// FUNCTION-LIKE frames (index >= 1) consult the PRE-COMPUTED
    /// `scope_bindings` (issue #438), so resolution inside a function is
    /// ORDER-INDEPENDENT — a name bound anywhere in the function shadows a
    /// builtin throughout it (incl. forward references and param shadow).
    ///
    /// The MODULE frame (index 0) instead consults the INCREMENTAL
    /// `declared_scopes[0]` (source order): the module body executes
    /// top-to-bottom, so a name referenced before its module-level assignment
    /// is not yet declared — matching pre-pre-pass behavior exactly. Used for
    /// builtin- and PSX-tag-shadow decisions.
    fn is_declared_in_any_scope(&self, name: &str) -> bool {
        // Review finding 2: if the innermost function scope declares `name`
        // `global`, it resolves ONLY at module/builtin scope — skip every
        // intervening function frame (an enclosing local `name` must not
        // capture it). So `def outer(): len=…; def inner(): global len;
        // return len(…)` lowers `len` by the MODULE binding, not outer's.
        let is_global = self
            .scope_globals
            .last()
            .is_some_and(|g| g.contains(name));
        let module_has = self
            .declared_scopes
            .first()
            .is_some_and(|m| m.contains(name));
        if is_global {
            return module_has;
        }
        module_has || self.scope_bindings.iter().skip(1).any(|s| s.contains(name))
    }

    /// DX-B2: record that JS `binding` is imported from `module` under the
    /// Python source name `py_local`. Returns `Ok(true)` when the binding was
    /// already claimed at MODULE scope by a different import whose PYTHON
    /// name DIFFERS — the snake→camel-manufactured convergence, resolved by
    /// ALIASING the new import (both Python names stay usable). Returns
    /// `Err(diagnostic)` when the collision is a genuine double-bind of ONE
    /// Python name — a cross-module collision that the old flat
    /// `imported_bindings` set resolved by silently dropping one side.
    /// Fresh bindings and same-module re-imports return `Ok(false)`.
    ///
    /// Scope-aware (fix J): only MODULE-LEVEL imports are tracked/checked.
    /// Function-local imports (`def f(): import a as m` / `def g(): import b as
    /// m`) are distinct Python-scoped bindings — flagging them as a collision
    /// was a false positive. `declared_scopes.len() == 1` is exactly module
    /// scope (every function/method/comprehension body pushes a scope; the
    /// function-local-import hoist only re-zeroes `indent`, not the scope
    /// stack). A function-local import that would still clash at the hoisted
    /// module top is a pre-existing hoisting concern, out of DX-B2's scope.
    fn register_import_binding_module(
        &mut self,
        py_local: &str,
        binding: &str,
        module: &str,
        export: &str,
    ) -> Result<bool, String> {
        if self.declared_scopes.len() != 1 {
            return Ok(false);
        }
        // Review finding 5: the import IDENTITY is (module, exported symbol),
        // not module alone — `from react import useState as h` and `from
        // pyths.react import use_effect as h` both resolve to "react" but bind
        // DIFFERENT exports under `h`; deduping on module alone wrongly kept
        // useState. `\0` cannot appear in a module path or identifier.
        let identity = format!("{}\u{0}{}", module, export);
        if let Some((prev, prev_py)) = self.imported_binding_modules.get(binding) {
            if prev != &identity {
                // DX-B2 root fix: DISTINCT Python names converging on one JS
                // binding (our own snake→camel conversion manufactured the
                // clash). Both are valid Python bindings — alias, don't error.
                if prev_py != py_local {
                    return Ok(true);
                }
                let (prev_mod, prev_exp) = prev.split_once('\u{0}').unwrap_or((prev.as_str(), ""));
                let show = |m: &str, e: &str| {
                    if e.is_empty() {
                        format!("{:?}", m)
                    } else {
                        format!("`{}` from {:?}", e, m)
                    }
                };
                return Err(format!(
                    "import name collision: `{}` is bound by two different imports \
                     ({} vs {}) at module scope. ES modules cannot bind one name twice. \
                     Alias one side.",
                    binding,
                    show(prev_mod, prev_exp),
                    show(module, export)
                ));
            }
        } else {
            self.imported_binding_modules
                .insert(binding.to_string(), (identity, py_local.to_string()));
        }
        Ok(false)
    }

    /// public #3: hard compile-time error at EXPRESSION position — recorded
    /// in `codegen_errors` (fails `pyths compile` and `pyths check`) plus an
    /// inline throw-expression so any artifact that slips through a
    /// non-gating path (`pyths run`/`test`/`bundle` inline codegen) still
    /// fails LOUDLY with the named diagnostic, never a bare ReferenceError.
    fn emit_expr_error(&mut self, diag: &str) {
        eprintln!("error: {}", diag);
        self.codegen_errors.push(diag.to_string());
        self.write(&format!(
            "(() => {{ throw new Error({}); }})()",
            js_string_literal(&format!("PythScribe: {}", diag))
        ));
    }

    /// Emit a hard compile-time error (recorded in `codegen_errors` + an inline
    /// `throw`), matching the ImportSideEffect breakout-rejection precedent.
    fn emit_import_error(&mut self, diag: &str) {
        eprintln!("error: {}", diag);
        self.codegen_errors.push(diag.to_string());
        self.writeln(&format!(
            "throw new Error({});",
            js_string_literal(&format!("PythScribe: {}", diag))
        ));
    }

    /// Record a hard compile-time diagnostic WITHOUT altering the emitted
    /// code. Fails `pyths check` / `pyths compile` (via `codegen_errors`) but
    /// leaves the current lowering in place — used by NB-2, where the intrinsic
    /// HTML element correctly wins the name collision (React-consistent) and we
    /// only need to make the silently-shadowed user binding LOUD.
    fn record_codegen_error(&mut self, diag: &str) {
        eprintln!("error: {}", diag);
        self.codegen_errors.push(diag.to_string());
    }

    /// Is `name` bound to a user symbol somewhere reachable at this call site —
    /// a local/param, a module-level `def`/`class`, or an import? Used by NB-2
    /// to detect a user binding that an intrinsic HTML tag silently shadows
    /// inside `@component`/`@psx`. (A `class` binding is already routed to
    /// `new` earlier, so in practice this fires for defs / locals / imports.)
    fn has_user_binding(&self, name: &str) -> bool {
        self.is_declared_in_any_scope(name)
            || self.known_functions.contains(name)
            || self.known_classes.contains(name)
            || self.imported_bindings.contains(name)
            || self.react_imports.contains(name)
            || self.module_namespaces.contains(name)
    }

    /// FULL_SURFACE bug #1: `import pkg.sub` (dotted, NO alias). CPython
    /// binds the TOP name (`pkg`) and makes the dotted path reachable
    /// through it (the submodule is set as an attribute on its parent
    /// package). The old lowering reused the aliased-form path with
    /// `local = "pkg.sub"`, emitting `import * as pkg.sub from "pkg/sub"`
    /// — a JS SyntaxError (an ESM namespace binding must be an identifier).
    ///
    /// Lowering: hoist the namespace under a UNIQUE name and graft it onto
    /// a mutable head object:
    ///
    /// ```text
    /// import * as __pyimp_pkg_sub_0 from "pkg/sub";
    /// let pkg = {};                    // first dotted import of `pkg`
    /// pkg.sub = __pyimp_pkg_sub_0;
    /// ```
    ///
    /// Deeper paths materialize plain-object intermediates (`a.b = {};`),
    /// copying a previously-grafted frozen namespace into a mutable object
    /// first when that level must now also hold a child (`a.b =
    /// Object.assign({}, a.b);`), and a leaf grafted where an intermediate
    /// object already exists merges the namespace UNDER the existing
    /// children (`a.b = Object.assign({}, ns, a.b);`). Inside a function
    /// body, `emit_stmt`'s #201 capture hoists the `import` line to module
    /// top while the `let`/graft lines stay local — same as every other
    /// import form.
    ///
    /// Documented residuals (each loud or CPython-rare):
    /// * `import a` followed by `import a.b` is a hard compile error — the
    ///   plain form bound `a` to an immutable frozen ESM namespace, so the
    ///   submodule graft cannot be expressed; alias one side.
    /// * `import a.b` followed by `import a` rebinds `a` to the plain
    ///   namespace (CPython would keep `a.b` reachable on the module).
    /// * Parent-package `__init__` attributes are NOT loaded — only the
    ///   dotted path itself is reachable (the multi-file package-graph
    ///   boundary, unchanged by this fix).
    fn emit_dotted_no_alias_import(&mut self, dotted: &str) {
        // Idempotent re-import of the exact same dotted path — no-op.
        if self
            .dotted_import_scopes
            .last()
            .is_some_and(|s| s.ns_paths.contains(dotted))
        {
            return;
        }
        let segs: Vec<&str> = dotted.split('.').collect();
        let head = segs[0];
        let head_js = Self::sanitize_ident(head).into_owned();
        let head_identity = format!("__pydotted__\u{0}{}", head);
        let head_known = self
            .dotted_import_scopes
            .last()
            .is_some_and(|s| s.heads.contains(head));
        if !head_known {
            if self.is_declared(head) {
                let diag = format!(
                    "`import {dotted}`: `{head}` is already bound in this scope \
                     (a plain `import {head}`, a parameter, or a local). ES module \
                     namespace objects are immutable, so the submodule cannot be \
                     attached to the existing binding; alias the dotted import \
                     (`import {dotted} as <name>`) or the plain one."
                );
                self.emit_import_error(&diag);
                return;
            }
            // DX-B2 collision registration for the head name (module scope):
            // a later `import x as {head}` / `from m import {head}` collides
            // loudly instead of silently clobbering the package object.
            // The head is a mutable graft object (`let {head} = {{}}`), not a
            // hoisted import, so the alias-and-rewrite path does NOT apply to
            // it — an aliasable (distinct-Python-name) collision against an
            // earlier import is also a hard error here: `let {head}` beside
            // the existing binding would be a redeclaration SyntaxError.
            match self.register_import_binding_module(head, head, head, "") {
                Err(diag) => {
                    self.emit_import_error(&diag);
                    return;
                }
                Ok(true) => {
                    let diag = format!(
                        "`import {dotted}`: the package head `{head}` collides with \
                         an earlier import's JS binding of the same name. Alias the \
                         dotted import (`import {dotted} as <name>`) or the earlier \
                         import."
                    );
                    self.emit_import_error(&diag);
                    return;
                }
                Ok(false) => {}
            }
        }
        let module_path = self.resolve_module(dotted);
        let unique = format!(
            "__pyimp_{}_{}",
            segs.iter()
                .map(|s| Self::sanitize_ident(s).into_owned())
                .collect::<Vec<_>>()
                .join("_"),
            self.import_rename_counter
        );
        self.import_rename_counter += 1;
        self.writeln(&format!(
            "import * as {} from {};",
            unique,
            js_string_literal(&module_path)
        ));
        if !head_known {
            self.writeln(&format!("let {} = {{}};", head_js));
            self.declare(head);
            // Record like an ASSIGNABLE import binding so a later plain
            // `import {head}` in this scope reassigns (`{head} = ns;`)
            // instead of `let`-redeclaring (a SyntaxError), and reserve the
            // module-top alias for the fix-J unique-rename machinery.
            if let Some(m) = self.scope_import_decls.last_mut() {
                m.insert(head.to_string(), (head_identity.clone(), true));
            }
            if self.declared_scopes.len() == 1 {
                self.hoisted_alias_module
                    .entry(head_js.clone())
                    .or_insert(head_identity);
                self.imported_bindings.insert(head.to_string());
            }
            if let Some(s) = self.dotted_import_scopes.last_mut() {
                s.heads.insert(head.to_string());
            }
        }
        // Materialize intermediate levels, then graft the namespace leaf.
        let mut path_js = head_js;
        let mut path_py = head.to_string();
        for (i, seg) in segs.iter().enumerate().skip(1) {
            path_js = format!("{}.{}", path_js, seg);
            path_py = format!("{}.{}", path_py, seg);
            let is_leaf = i == segs.len() - 1;
            enum Action {
                None,
                FreshObj,
                CopyNsToObj,
                GraftLeaf,
                MergeLeafUnderChildren,
            }
            let action = {
                let scope = self
                    .dotted_import_scopes
                    .last_mut()
                    .expect("module scope always present");
                if is_leaf {
                    if scope.obj_paths.contains(&path_py) {
                        Action::MergeLeafUnderChildren
                    } else {
                        scope.ns_paths.insert(path_py.clone());
                        Action::GraftLeaf
                    }
                } else if scope.obj_paths.contains(&path_py) {
                    Action::None
                } else if scope.ns_paths.remove(&path_py) {
                    scope.obj_paths.insert(path_py.clone());
                    Action::CopyNsToObj
                } else {
                    scope.obj_paths.insert(path_py.clone());
                    Action::FreshObj
                }
            };
            match action {
                Action::None => {}
                Action::FreshObj => self.writeln(&format!("{} = {{}};", path_js)),
                Action::CopyNsToObj => self.writeln(&format!(
                    "{} = Object.assign({{}}, {});",
                    path_js, path_js
                )),
                Action::GraftLeaf => self.writeln(&format!("{} = {};", path_js, unique)),
                Action::MergeLeafUnderChildren => self.writeln(&format!(
                    "{} = Object.assign({{}}, {}, {});",
                    path_js, unique, path_js
                )),
            }
        }
    }

    /// Round-3 unification: THE single decision point for ANY import form
    /// that binds a name — plain/aliased `import`, generic `from ... import`,
    /// the recognized-lib hybrid from-import (`pyths.react`), and relative
    /// from-imports. For one (binding, module, exported-symbol) it:
    ///
    ///   (a) registers the import IDENTITY for DX-B2 collision detection
    ///       (module scope) — a cross-module collision hard-errors when the
    ///       PYTHON names match (genuine double-bind), or ALIASES the new
    ///       import (`Alias { unique }`) when the Python names differ — the
    ///       snake→camel convergence class, where both names are valid and
    ///       reference sites are rewritten via `import_ref_renames`;
    ///   (b) dedups an idempotent re-import (same scope, same identity —
    ///       tracked in `scope_import_decls`, NOT inferred from
    ///       `is_declared`, so a param that merely shares the name is not
    ///       mistaken for the import itself);
    ///   (c) applies the param-shadow rebind: a binding that is already a
    ///       param/earlier local REASSIGNS (`hook = __pyimp_hook_0`) instead
    ///       of `const`-redeclaring (a SyntaxError) or being shadowed;
    ///   (d) unique-renames a same-alias/different-identity hoist collision
    ///       (fix J) into a body-local `const` shadow.
    ///
    /// The pre-pass local set (`collect_bound_names`) already treats
    /// Import/ImportFrom uniformly, so this closes the loop: no import form
    /// can bypass the shared binding logic. Emission SYNTAX (named specifier
    /// vs `import * as`) stays with the caller; the DECISION lives here.
    ///
    /// Callers MUST `declare(binding)` only AFTER planning (the shadow check
    /// reads the declared set) and must abort the whole statement on
    /// [`ImportBindingPlan::Error`].
    fn plan_import_binding(
        &mut self,
        py_local: &str,
        binding: &str,
        reg_module: &str,
        exported: &str,
        identity_module: &str,
    ) -> ImportBindingPlan {
        let plan =
            self.plan_import_binding_impl(py_local, binding, reg_module, exported, identity_module);
        // #443: an import that REBINDS a name whose `class` definition
        // already executed (source order) shadows the class — Python is
        // last-wins. Drop it from the pre-scanned `known_classes` so calls
        // after this point lower as plain calls, not `new`-construction
        // (`class sqrt: …; from math import sqrt; sqrt(9.0)` must call
        // math.sqrt). A Fresh/Dedup plan leaves the heuristic sets alone —
        // stdlib CapWords class registrations (Counter, datetime, …) are
        // Fresh binds and stay `new`-lowered.
        if matches!(plan, ImportBindingPlan::Rebind { .. })
            && self.emitted_class_names.contains(binding)
        {
            self.known_classes.remove(binding);
        }
        plan
    }

    fn plan_import_binding_impl(
        &mut self,
        py_local: &str,
        binding: &str,
        reg_module: &str,
        exported: &str,
        identity_module: &str,
    ) -> ImportBindingPlan {
        // (a) DX-B2 registration — module-scope cross-module collision.
        // `aliasable` = the JS binding is claimed by an import bound under a
        // DIFFERENT Python name (snake→camel convergence, e.g. zustand's
        // `create_store` vs redux's `createStore`): both Python names are
        // valid distinct bindings, so the new import hoists under a unique
        // JS name and its references are rewritten — not a hard error.
        let aliasable =
            match self.register_import_binding_module(py_local, binding, reg_module, exported) {
                Err(diag) => {
                    self.emit_import_error(&diag);
                    return ImportBindingPlan::Error;
                }
                Ok(a) => a,
            };
        let js_binding = Self::sanitize_ident(binding).into_owned();
        // Identity = (resolved module, exported symbol); `\0` can appear in
        // neither, so the pair packs into one string (finding 5).
        let identity = format!("{}\u{0}{}", identity_module, exported);
        if aliasable {
            // Idempotent re-import of the SAME aliased import — dedup, the
            // existing unique hoist + rename already serve it.
            if self.aliased_import_identities.get(py_local) == Some(&identity) {
                return ImportBindingPlan::Dedup;
            }
            // A THIRD import rebinding this Python name (last-wins from here
            // on) overwrites the rename; earlier reference sites already
            // emitted the previous unique in source order, matching Python's
            // module-body execution order.
            let u = format!("__pyimp_{}_{}", js_binding, self.import_rename_counter);
            self.import_rename_counter += 1;
            self.import_ref_renames
                .insert(py_local.to_string(), u.clone());
            self.aliased_import_identities
                .insert(py_local.to_string(), identity);
            return ImportBindingPlan::Alias { unique: u };
        }
        // B8(b) CLASS rule: our snake→camel conversion must never CLAIM a JS
        // name that MODULE-level user code binds (`from zustand import
        // create_store` + a user `createStore = …` / `def createStore` —
        // "Identifier 'createStore' has already been declared", or a silently
        // killed import). Same resolution as the import-import convergence
        // above: hoist under a unique name and rewrite this Python name's
        // reference sites. Fires only when the conversion actually renamed
        // (`binding != py_local`) — a user binding of the import's OWN Python
        // name is the #443/B2 rebind lane, not a manufactured collision.
        if self.declared_scopes.len() == 1
            && binding != py_local
            && self.module_bound_names.contains(binding)
        {
            if self.aliased_import_identities.get(py_local) == Some(&identity) {
                return ImportBindingPlan::Dedup;
            }
            let u = format!("__pyimp_{}_{}", js_binding, self.import_rename_counter);
            self.import_rename_counter += 1;
            self.import_ref_renames
                .insert(py_local.to_string(), u.clone());
            self.aliased_import_identities
                .insert(py_local.to_string(), identity);
            return ImportBindingPlan::Alias { unique: u };
        }
        // What does `binding` refer to in THIS scope right now?
        // `prior_here` = a previous import in this scope: (identity,
        // assignable) where `assignable` means the JS binding the name
        // resolves to here is a body-local `let` / reassigned param (true)
        // rather than an immutable module-top import hoist (false).
        let prior_here = self
            .scope_import_decls
            .last()
            .and_then(|m| m.get(binding))
            .cloned();
        // (b) idempotent re-import: THIS scope already bound `binding` via
        // this exact import — Python's re-import is a no-op rebind.
        if let Some((prev_id, _)) = &prior_here {
            if prev_id == &identity {
                return ImportBindingPlan::Dedup;
            }
        }
        // Module scope (`declared_scopes` == [module]) is the ONLY scope
        // where the top-level ESM binding IS the Python binding. Every
        // function/method/comprehension body pushes a scope, so len > 1 means
        // the import binds a name LOCAL to a function.
        let at_module_scope = self.declared_scopes.len() == 1;
        let record = |zelf: &mut Self, assignable: bool| {
            if let Some(m) = zelf.scope_import_decls.last_mut() {
                m.insert(binding.to_string(), (identity.clone(), assignable));
            }
        };
        let unique = |zelf: &mut Self| {
            let u = format!("__pyimp_{}_{}", js_binding, zelf.import_rename_counter);
            zelf.import_rename_counter += 1;
            u
        };
        match prior_here {
            // A DIFFERENT import of the same name earlier in THIS scope. If
            // that import left a mutable body-local (`let` shadow / reassigned
            // param), REASSIGN; if it was this scope's own immutable hoist
            // (module scope), introduce the body-local `let` shadow now.
            Some((_, assignable)) => {
                let u = unique(self);
                record(self, true);
                ImportBindingPlan::Rebind {
                    js_binding,
                    unique: u,
                    reassign: assignable,
                }
            }
            None => {
                // (c) captured BEFORE the caller declares this import's
                // binding: the name is a param / earlier local of the CURRENT
                // scope, so the import must overwrite it (Python rebind), not
                // redeclare (SyntaxError) or be shadowed by it.
                if self.is_declared(binding) {
                    let u = unique(self);
                    record(self, true);
                    return ImportBindingPlan::Rebind {
                        js_binding,
                        unique: u,
                        reassign: true,
                    };
                }
                if !at_module_scope {
                    // Findings 2 & 3 — FUNCTION-LOCAL import. Python scoping
                    // makes this a name LOCAL to the function, so it must
                    // NEVER dedup against or reuse an outer/module ESM binding
                    // (that erases Python's local/use-before-import
                    // semantics — finding 2). The actual `import` is hoisted
                    // under a UNIQUE name and the local name is bound in the
                    // body via `let name = <unique>` at the import's position.
                    // Consequences: a use BEFORE the import is a TDZ fault
                    // (≈ UnboundLocalError) rather than a silent outer read;
                    // and a re-import later in the SAME function is a plain
                    // reassignment (prior_here → assignable), not a second
                    // `let` that would TDZ-shadow the earlier reads (finding
                    // 3). Not registered in `hoisted_alias_module` — that map
                    // is only for module-top dedup, which local scopes must
                    // not participate in.
                    self.imported_bindings.insert(binding.to_string());
                    let u = unique(self);
                    record(self, true);
                    return ImportBindingPlan::Rebind {
                        js_binding,
                        unique: u,
                        reassign: false,
                    };
                }
                match self.hoisted_alias_module.get(&js_binding).cloned() {
                    // Same identity already hoisted at module scope — the
                    // binding is visible here untouched; dedup.
                    Some(prev) if prev == identity => {
                        record(self, false);
                        ImportBindingPlan::Dedup
                    }
                    // (d) different module/export under the same module-top
                    // alias (fix J): hoist uniquely, `let`-shadow in the body.
                    Some(_) => {
                        let u = unique(self);
                        record(self, true);
                        ImportBindingPlan::Rebind {
                            js_binding,
                            unique: u,
                            reassign: false,
                        }
                    }
                    None => {
                        self.hoisted_alias_module
                            .insert(js_binding.clone(), identity.clone());
                        self.imported_bindings.insert(binding.to_string());
                        record(self, false);
                        ImportBindingPlan::Fresh
                    }
                }
            }
        }
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

    /// #452/#453: a guaranteed-fresh JS identifier for an internal codegen
    /// temporary. ONE rule applied at every temp-minting site: if the module
    /// uses `base` anywhere (as a reference or a binding — `module_idents` is
    /// the whole-module pre-pass, so the decision is order-independent), the
    /// temp gets a `$` suffix, which can never appear in a Python identifier
    /// (and `sanitize_ident` appends `$` only to JS RESERVED words, which the
    /// `__`-prefixed bases never are) — fresh by construction. Otherwise the
    /// bare base is kept, so emitted JS is unchanged in the common
    /// (non-colliding) case. Same-base temps in NESTED emission sites reuse
    /// the same name deliberately: each binds its own JS block/IIFE scope,
    /// where the shadowing mirrors Python's comprehension-scope nesting.
    fn fresh_temp(&self, base: &str) -> String {
        if self.module_idents.contains(base) {
            format!("{base}$")
        } else {
            base.to_string()
        }
    }

    /// Mark a name as declared in the current scope.
    fn declare(&mut self, name: &str) {
        if let Some(scope) = self.declared_scopes.last_mut() {
            scope.insert(name.to_string());
        }
    }

    /// Round-4 finding 1: forget any import-identity this scope recorded for
    /// `name`. The `scope_import_decls` cache lets `plan_import_binding` dedup
    /// an idempotent re-import, but ORDINARY code that rebinds the name
    /// between two imports (`from math import sqrt; sqrt = ...; from math
    /// import sqrt`) breaks that identity — the SECOND import must re-emit,
    /// not dedup to the (now-overwritten) first. Called from every non-import
    /// rebinding of a bare name (assignment / for-target / aug-assign /
    /// annotated-assign-with-value / del).
    fn invalidate_import_decl(&mut self, name: &str) {
        if let Some(m) = self.scope_import_decls.last_mut() {
            m.remove(name);
        }
    }

    /// Mark all names in an assignment/for target as declared.
    fn declare_target(&mut self, target: &Expr) {
        match &target.kind {
            ExprKind::Name(name) => {
                // A plain-name assignment target is a non-import rebind — it
                // invalidates any import identity cached for the name so a
                // later re-import is not wrongly deduped (finding 1).
                self.invalidate_import_decl(name);
                self.declare(name);
            }
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

    /// Collect every `class` name at ANY nesting depth (inside functions,
    /// class bodies, if/while/for/with/try/match blocks). Feeds
    /// `known_classes` so a class defined inside a function participates in
    /// the cooperative PyObject MRO model exactly like a module-level one
    /// (autotester classes / local_classes: `class C(A, B)` inside `def run`
    /// previously treated A as an external native base).
    fn collect_class_names(body: &[Stmt], out: &mut HashSet<String>) {
        for stmt in body {
            match &stmt.kind {
                StmtKind::ClassDef { name, body, .. } => {
                    out.insert(name.clone());
                    Self::collect_class_names(body, out);
                }
                StmtKind::FuncDef { body, .. } => Self::collect_class_names(body, out),
                StmtKind::If {
                    body,
                    elif_clauses,
                    else_body,
                    ..
                } => {
                    Self::collect_class_names(body, out);
                    for (_, b) in elif_clauses {
                        Self::collect_class_names(b, out);
                    }
                    if let Some(b) = else_body {
                        Self::collect_class_names(b, out);
                    }
                }
                StmtKind::While { body, else_body, .. } => {
                    Self::collect_class_names(body, out);
                    if let Some(b) = else_body {
                        Self::collect_class_names(b, out);
                    }
                }
                StmtKind::For { body, else_body, .. } => {
                    Self::collect_class_names(body, out);
                    if let Some(b) = else_body {
                        Self::collect_class_names(b, out);
                    }
                }
                StmtKind::With { body, .. } => Self::collect_class_names(body, out),
                StmtKind::Try {
                    body,
                    handlers,
                    else_body,
                    finally_body,
                } => {
                    Self::collect_class_names(body, out);
                    for h in handlers {
                        Self::collect_class_names(&h.body, out);
                    }
                    if let Some(b) = else_body {
                        Self::collect_class_names(b, out);
                    }
                    if let Some(b) = finally_body {
                        Self::collect_class_names(b, out);
                    }
                }
                StmtKind::Match { cases, .. } => {
                    for c in cases {
                        Self::collect_class_names(&c.body, out);
                    }
                }
                _ => {}
            }
        }
    }

    /// The docstring of a body: its FIRST statement when that is a bare
    /// string-literal expression (CPython rules; later strings are not
    /// docstrings).
    fn body_docstring(body: &[Stmt]) -> Option<&str> {
        match body.first().map(|st| &st.kind) {
            Some(StmtKind::Expr(e)) => match &e.kind {
                ExprKind::StringLiteral(lit) => Some(lit),
                _ => None,
            },
            _ => None,
        }
    }

    /// WB-15: does this method body directly contain a `this`-rebinding nested
    /// scope — a nested `function` def OR a nested `class` — that a bare `self`
    /// could close over? Such a method must capture `const __self = this;` so
    /// the nested scope reads the receiver via the alias (its own JS `this` is
    /// rebound). Covers both a nested def and a nested class (whose static
    /// method may close over the outer `self`). Recurses through control-flow
    /// blocks (if/while/for/with/try/match) but stops at the first nested scope.
    fn contains_nested_scope(body: &[Stmt]) -> bool {
        body.iter().any(|stmt| match &stmt.kind {
            StmtKind::FuncDef { .. } | StmtKind::ClassDef { .. } => true,
            StmtKind::If { body, elif_clauses, else_body, .. } => {
                Self::contains_nested_scope(body)
                    || elif_clauses.iter().any(|(_, b)| Self::contains_nested_scope(b))
                    || else_body.as_deref().is_some_and(Self::contains_nested_scope)
            }
            StmtKind::While { body, else_body, .. }
            | StmtKind::For { body, else_body, .. } => {
                Self::contains_nested_scope(body)
                    || else_body.as_deref().is_some_and(Self::contains_nested_scope)
            }
            StmtKind::With { body, .. } => Self::contains_nested_scope(body),
            StmtKind::Try { body, handlers, else_body, finally_body } => {
                Self::contains_nested_scope(body)
                    || handlers.iter().any(|h| Self::contains_nested_scope(&h.body))
                    || else_body.as_deref().is_some_and(Self::contains_nested_scope)
                    || finally_body.as_deref().is_some_and(Self::contains_nested_scope)
            }
            StmtKind::Match { cases, .. } => {
                cases.iter().any(|c| Self::contains_nested_scope(&c.body))
            }
            _ => false,
        })
    }

    /// WB-15: cross into a nested scope whose own JS `this` is rebound (a nested
    /// `function`, a `class` body, a static/classmethod) and that does NOT bind
    /// `self` as a local. A live receiver (`Receiver`) must switch to the
    /// `__self` alias there; `ReceiverAlias`/`Ordinary` are unchanged. Returns
    /// the previous state for the caller to restore. When the crossed scope
    /// instead binds `self` (a `self` param/const), the caller sets `Ordinary`
    /// directly rather than calling this.
    fn cross_self_this_boundary(&mut self) -> SelfLowering {
        let prev = self.self_lowering;
        if prev == SelfLowering::Receiver {
            self.self_lowering = SelfLowering::ReceiverAlias;
        }
        prev
    }

    /// WB-15 (S5): a comprehension is its own Python scope. If one of its
    /// for-targets is named `self`, that target SHADOWS any enclosing
    /// instance-method receiver — inside the comprehension's target + element +
    /// condition, `self` is the ordinary comprehension variable (binder position
    /// AND reference), never `this`/`__self`. Callers wrap ONLY that region: the
    /// OUTERMOST iterable is evaluated in the enclosing scope and keeps its
    /// lowering. Returns the previous state; restore it after the region.
    fn enter_comp_self_shadow(&mut self, generators: &[Comprehension]) -> SelfLowering {
        let prev = self.self_lowering;
        if Self::comprehension_target_names(generators).contains("self") {
            self.self_lowering = SelfLowering::Ordinary;
        }
        prev
    }

    /// WB-15 (B3): emit the OUTERMOST comprehension iterable — `generators[0]
    /// .iter`, with its sync/async protocol bridge — as an expression evaluated
    /// in the ENCLOSING scope. Every comprehension emission path (fast, loop, and
    /// genexp) evaluates the outer iterable OUTSIDE `enter_comp_self_shadow` and
    /// passes it in (the fast path via a lifted-scope inline; the loop paths as an
    /// IIFE argument `__comp_it`; genexps via `.call(this, …)`), so a
    /// receiver-reading outer iterable (`for self in self.items`) lowers to the
    /// receiver, never to the not-yet-bound comprehension variable. Bridge
    /// policy is UNIFORM with emit_for and emit_comp_loops: sync →
    /// `emit_iterable` (pyForIter/pyDictKeys shape dispatch); async →
    /// `__pyAsyncIter(<raw expr>)` — the async protocol bridge on the RAW
    /// expression, never the sync pyForIter wrap (an async iterable is
    /// never a dict, and the sync wrap turned a Python-protocol
    /// `__aiter__` class into its attribute keys — the comprehension
    /// matrix's async rows guard this).
    fn emit_outer_comp_iterable(&mut self, generators: &[Comprehension]) {
        let first = &generators[0];
        if first.is_async {
            self.need_runtime("__pyAsyncIter");
            self.write("__pyAsyncIter(");
            self.emit_expr(&first.iter);
            self.write(")");
        } else {
            self.emit_iterable(&first.iter);
        }
    }

    /// True iff any class body (at any depth) defines a `__call__` method.
    fn defines_dunder_call(body: &[Stmt]) -> bool {
        body.iter().any(|stmt| match &stmt.kind {
            StmtKind::ClassDef { body, .. } => {
                body.iter().any(|s| {
                    matches!(&s.kind, StmtKind::FuncDef { name, .. } if name == "__call__")
                }) || Self::defines_dunder_call(body)
            }
            StmtKind::FuncDef { body, .. } | StmtKind::With { body, .. } => {
                Self::defines_dunder_call(body)
            }
            StmtKind::If { body, elif_clauses, else_body, .. } => {
                Self::defines_dunder_call(body)
                    || elif_clauses.iter().any(|(_, b)| Self::defines_dunder_call(b))
                    || else_body.as_deref().is_some_and(Self::defines_dunder_call)
            }
            StmtKind::While { body, else_body, .. }
            | StmtKind::For { body, else_body, .. } => {
                Self::defines_dunder_call(body)
                    || else_body.as_deref().is_some_and(Self::defines_dunder_call)
            }
            StmtKind::Try { body, handlers, else_body, finally_body } => {
                Self::defines_dunder_call(body)
                    || handlers.iter().any(|h| Self::defines_dunder_call(&h.body))
                    || else_body.as_deref().is_some_and(Self::defines_dunder_call)
                    || finally_body.as_deref().is_some_and(Self::defines_dunder_call)
            }
            StmtKind::Match { cases, .. } => {
                cases.iter().any(|c| Self::defines_dunder_call(&c.body))
            }
            _ => false,
        })
    }

    pub fn emit_module(&mut self, module: &Module) {
        // Heuristic: ~80 bytes of JS per statement
        self.output.reserve(module.body.len() * 80);

        // Issue #438 (review: module scope stays SOURCE-ORDER): the module body
        // executes top-to-bottom, so a name referenced before its module-level
        // assignment is NOT yet a declared local. The module frame therefore
        // keeps the incremental `declared_scopes[0]` path (see
        // `is_declared_in_any_scope`); only FUNCTION-LIKE scopes get the
        // precomputed order-independent binding set. `scope_bindings[0]` stays
        // an unused placeholder.

        // Pre-scan: collect every `class` name — at ANY nesting depth — so
        // that calls inside @component functions can disambiguate dataclass
        // instantiation (`Alert(...)` → `new Alert(...)`) from React
        // component creation (`Header(...)` → `createElement(Header, ...)`),
        // and so a class defined INSIDE a function (autotester classes /
        // local_classes) is recognized as a compiled-here base: without
        // this, `class C(A, B)` inside a `def` saw A as an "external native"
        // base and lost the cooperative PyObject MRO model entirely (native
        // `constructor` + `super()` → wrong ctor order, dead `A.__init__`
        // unbound calls).
        Self::collect_class_names(&module.body, &mut self.known_classes);
        // B8(b): order-independent module-level USER binding pre-scan —
        // import-bound names are SUBTRACTED: import↔import JS-name
        // convergence is the DX-B2 register's lane (source-order aware), and
        // including a later import's binding here would wrongly alias the
        // EARLIER import against it. See `module_bound_names`.
        Self::collect_bound_names(&module.body, &mut self.module_bound_names);
        {
            let mut import_bound = HashSet::new();
            Self::collect_import_bound_names(&module.body, &mut import_bound);
            for n in &import_bound {
                self.module_bound_names.remove(n);
            }
        }
        // #452/#453 (naming soundness): whole-module identifier pre-pass —
        // references AND bindings at every nesting depth — so `fresh_temp`
        // can guarantee internal temporaries never collide with a user name,
        // order-independently (a user `__result` bound AFTER a comprehension
        // still forces the suffixed temp).
        Self::collect_all_idents(&module.body, &mut self.module_idents);
        self.module_has_dunder_call = Self::defines_dunder_call(&module.body);
        self.module_doc = Self::body_docstring(&module.body).map(str::to_owned);
        for stmt in &module.body {
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
                        if matches!(
                            a.name.as_str(),
                            "datetime" | "date" | "time" | "timedelta" | "timezone"
                        ) {
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
                // WB-8: a BARE (level-0) `from <module> import ...` whose
                // module is NOT a recognized external package (React/Next,
                // stdlib, a known npm mapping, a scoped `at_` package, …)
                // names a SIBLING project `.ps` module — compiled by this
                // same compiler with the PyObject model. Its exported
                // classes must be treated exactly like a relatively-imported
                // base (#300): `class Sub(Base)` gets the cooperative
                // `__pyClass`/MRO wrapping so `Sub` carries its OWN `__mro__`
                // and cross-module `super()` (esp. a grandparent method)
                // resolves. Before this, a bare-imported base was assumed
                // external-native → NO `__pyClass` → no own `__mro__` →
                // `__pySuper(...).<m> is not a function`. React/npm/stdlib
                // bases stay native `extends` via `is_external_pkg_module`.
                if *level == 0 && !module.is_empty() && !is_external_pkg_module(module) {
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
        let hoisted_names = Self::collect_hoisted_names(&module.body, true);
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
                // DX (Lane B2 row 7): Python `breakpoint()` used to emit a bare
                // `breakpoint();` — a call to an undefined global → runtime
                // `ReferenceError`, with no compile diagnostic. Lower the bare
                // builtin to a JS `debugger;` statement (its closest analogue).
                // Only when `breakpoint` is not shadowed by a user binding.
                //
                // Fix I: ONLY the zero-arg/zero-kwarg form maps to `debugger;`.
                // `breakpoint(**make_kwargs())` (or any args/kwargs) must not
                // silently drop the argument expressions' side effects — fall
                // through to normal emission, which evaluates them.
                if let ExprKind::Call {
                    func, args, kwargs, ..
                } = &expr.kind
                {
                    if args.is_empty() && kwargs.is_empty() {
                        if let ExprKind::Name(n) = &func.kind {
                            if n == "breakpoint" && !self.is_declared_in_any_scope("breakpoint") {
                                self.write_indent();
                                self.write("debugger;\n");
                                return;
                            }
                        }
                    }
                }
                self.write_indent();
                self.emit_expr(expr);
                self.write(";\n");
            }
            StmtKind::Assign { targets, value } => {
                self.emit_assign(targets, value);
            }
            StmtKind::AugAssign { target, op, value } => {
                // Finding 1: an aug-assign to a bare name rebinds it — forget
                // any cached import identity so a later re-import re-emits.
                if let ExprKind::Name(n) = &target.kind {
                    self.invalidate_import_decl(n);
                }
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
                                // MatMul MUST route through the helper path:
                                // JS has no `@=` operator, so the bare fast
                                // path would emit a syntax error — and the
                                // helper (pyIMatMul) is what dispatches
                                // __imatmul__ before __matmul__.
                                | AugAssignOp::MatMul
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
                            // Ops with in-place CONTAINER semantics route
                            // through the pyI* in-place protocol helpers so
                            // `d[k] += [..]` mutates the stored list (aliases
                            // observe it) instead of rebinding a fresh one —
                            // see the plain-NAME map below.
                            let helper = match op {
                                AugAssignOp::Add => Some("pyIAdd"),
                                AugAssignOp::Sub => Some("pyISub"),
                                AugAssignOp::Mul => Some("pyIMul"),
                                AugAssignOp::Div => Some("pyDiv"),
                                AugAssignOp::FloorDiv => Some("pyFloorDiv"),
                                AugAssignOp::Mod => Some("pyMod"),
                                AugAssignOp::Pow => Some("pyPow"),
                                AugAssignOp::BitAnd => Some("pyIBitAnd"),
                                AugAssignOp::BitOr => Some("pyIBitOr"),
                                AugAssignOp::BitXor => Some("pyIBitXor"),
                                AugAssignOp::ShiftLeft => Some("pyShiftLeft"),
                                AugAssignOp::ShiftRight => Some("pyShiftRight"),
                                // `@=` dispatches __imatmul__ first (CPython
                                // in-place protocol), falling back to
                                // __matmul__/__rmatmul__ inside the helper.
                                AugAssignOp::MatMul => Some("pyIMatMul"),
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
                // Attribute target (`obj.attr OP= v`) — the ONE AugAssign
                // form that still fell through to a raw JS operator (string
                // coercion for `self.xs += [..]`, no BigInt promotion, no
                // in-place protocol). Route through the same helpers as the
                // name/subscript paths, hoisting the RECEIVER once into a
                // const so a side-effecting receiver (`getobj().attr += v`)
                // is not double-evaluated (same discipline as `__aug_o{n}`
                // above). The read side mirrors the binary form's attribute
                // read (pyBoundMethod, data attrs pass through; direct read
                // for dunder attrs, matching the value-position rule); the
                // write side is the plain property store emit_assign uses.
                if let ExprKind::Attribute {
                    value: recv,
                    attr,
                    optional,
                } = &target.kind
                {
                    if !*optional {
                        let helper = match op {
                            AugAssignOp::Add => "pyIAdd",
                            AugAssignOp::Sub => "pyISub",
                            AugAssignOp::Mul => "pyIMul",
                            AugAssignOp::Div => "pyDiv",
                            AugAssignOp::FloorDiv => "pyFloorDiv",
                            AugAssignOp::Mod => "pyMod",
                            AugAssignOp::Pow => "pyPow",
                            AugAssignOp::BitAnd => "pyIBitAnd",
                            AugAssignOp::BitOr => "pyIBitOr",
                            AugAssignOp::BitXor => "pyIBitXor",
                            AugAssignOp::ShiftLeft => "pyShiftLeft",
                            AugAssignOp::ShiftRight => "pyShiftRight",
                            AugAssignOp::MatMul => "pyIMatMul",
                        };
                        let strict_dict =
                            matches!(self.infer_type(recv), JsInferredType::Dict);
                        let n = self.default_hoist_counter;
                        self.default_hoist_counter += 1;
                        let o = format!("__aug_o{}", n);
                        self.need_runtime(helper);
                        self.write_indent();
                        self.write(&format!("{{ const {} = ", o));
                        self.emit_expr(recv);
                        if attr.starts_with("__") {
                            // Dunder attrs read directly (value-position rule).
                            self.write(&format!(
                                "; {o}.{attr} = {h}({o}.{attr}, ",
                                o = o,
                                attr = attr,
                                h = helper
                            ));
                        } else {
                            self.need_runtime("pyBoundMethod");
                            self.write(&format!(
                                "; {o}.{attr} = {h}(pyBoundMethod({o}, {attr:?}{strict}), ",
                                o = o,
                                attr = attr,
                                h = helper,
                                strict = if strict_dict { ", 1" } else { "" }
                            ));
                        }
                        self.emit_expr(value);
                        self.write("); }\n");
                        return;
                    }
                }
                // Round-2 pythonic sweep: plain-NAME augmented assignment
                // routes through the same Python-operator helpers as the
                // binary form — raw JS `d |= {...}` coerces a dict to a
                // number (printed 0), raw `+=` skips BigInt promotion and
                // list concat. Reading a name twice is side-effect-free,
                // so `x = h(x, v)` is exact.
                if matches!(&target.kind, ExprKind::Name(_)) {
                    // Bug-1 (aliasing soundness): ops with in-place CONTAINER
                    // semantics lower to the pyI* helpers, which implement
                    // CPython's in-place protocol — a mutable target (list
                    // +=/*=, set |=/&=/-=/^=, dict |=) is MUTATED and returned
                    // (the `x =` rebind is then a no-op and `x is y` survives,
                    // like __iadd__/__ior__/...); immutables fall back to the
                    // value helper inside pyI* (a genuine rebind). Generalizes
                    // the pyIMatMul precedent below. `/=`,`//=`,`%=`,`**=` and
                    // the shifts have no in-place container semantics — they
                    // stay on the value helpers.
                    let helper = match op {
                        AugAssignOp::Add => Some("pyIAdd"),
                        AugAssignOp::Sub => Some("pyISub"),
                        AugAssignOp::Mul => Some("pyIMul"),
                        AugAssignOp::Div => Some("pyDiv"),
                        AugAssignOp::FloorDiv => Some("pyFloorDiv"),
                        AugAssignOp::Mod => Some("pyMod"),
                        AugAssignOp::Pow => Some("pyPow"),
                        AugAssignOp::BitAnd => Some("pyIBitAnd"),
                        AugAssignOp::BitOr => Some("pyIBitOr"),
                        AugAssignOp::BitXor => Some("pyIBitXor"),
                        AugAssignOp::ShiftLeft => Some("pyShiftLeft"),
                        AugAssignOp::ShiftRight => Some("pyShiftRight"),
                        // `@=` dispatches __imatmul__ first (CPython in-place
                        // protocol); pyIMatMul falls back to __matmul__.
                        AugAssignOp::MatMul => Some("pyIMatMul"),
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
                return_type,
                is_async,
            } => {
                if self.wasm_skip.contains(name) {
                    return; // compiled to WASM, re-exported from glue
                }
                // #443: `def X` REBINDS X (Python last-wins). Captured BEFORE
                // `declare` — an already-declared name (an import's binding, a
                // param, an earlier local) forces the assignment form so the
                // emitted `function X` cannot redeclare-collide with it, and
                // any import identity cached for the name is forgotten so a
                // later re-import re-emits instead of deduping to the def.
                let rebind_declared = self.is_declared(name);
                self.invalidate_import_decl(name);
                self.declare(name);
                self.emit_func_def(
                    name,
                    params,
                    body,
                    decorator_list,
                    return_type.as_ref(),
                    *is_async,
                    rebind_declared,
                );
            }
            StmtKind::ClassDef {
                name,
                bases,
                body,
                decorator_list,
            } => {
                // #443: `class X` rebinds exactly like `def X` — see above.
                let rebind_declared = self.is_declared(name);
                self.invalidate_import_decl(name);
                self.declare(name);
                self.emit_class_def(name, bases, body, decorator_list, rebind_declared);
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
                    // B4: `pyths.react` is a HYBRID surface — its symbols live in
                    // FOUR different modules (react, react-dom, react-dom/client,
                    // pyths-runtime/react), so it cannot be a single ESM
                    // namespace. `import pyths.react as R` used to emit
                    // `import * as R from "pyths-runtime/react"` (which exports
                    // none of the React APIs) → `R.create_element` undefined at
                    // load. Diagnose and steer to the per-symbol import form.
                    if alias.name == "pyths.react" {
                        let diag = "`import pyths.react` is not supported — `pyths.react` is a \
                             hybrid surface whose symbols live in several different modules, so \
                             it has no single namespace. Use `from pyths.react import \
                             create_element, use_state, ...` (member access like \
                             `R.create_element` cannot be routed)."
                            .to_string();
                        eprintln!("error: {}", diag);
                        self.codegen_errors.push(diag.clone());
                        let local = alias.alias.as_deref().unwrap_or("pyths");
                        self.declare(local);
                        continue;
                    }
                    // #448: `import importlib [as X]` — and the WHOLE importlib
                    // package surface (`import importlib.util`, `import
                    // importlib.machinery as m`, …). importlib is not a real
                    // module in the compiled output — emit NOTHING (the old code
                    // emitted a broken `import * as importlib from "importlib"`)
                    // and remember the bound name so EVERY reference to it (not
                    // just `.import_module` member calls) is diagnosed with the
                    // supported-form guidance — see the Name-arm class rule.
                    if alias.name == "importlib" || alias.name.starts_with("importlib.") {
                        // CPython binds the TOP name for a no-alias dotted
                        // import; the alias otherwise.
                        let local = alias
                            .alias
                            .as_deref()
                            .unwrap_or_else(|| alias.name.split('.').next().unwrap());
                        // Track the namespace WITHOUT declaring it: it has no
                        // runtime binding, and leaving it undeclared lets the
                        // reference rule distinguish a genuine importlib
                        // namespace from a user local that shadows the name.
                        self.importlib_namespaces.insert(local.to_string());
                        continue;
                    }
                    // FULL_SURFACE #1: `import pkg.sub` WITHOUT an alias
                    // binds the top name `pkg` (CPython semantics) — a
                    // dedicated lowering; the dotted name is not a legal ESM
                    // namespace binding. The aliased form stays below.
                    if alias.alias.is_none() && alias.name.contains('.') {
                        self.emit_dotted_no_alias_import(&alias.name);
                        continue;
                    }
                    let local = alias.alias.as_deref().unwrap_or(&alias.name);
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
                    if base == "datetime" {
                        self.datetime_namespaces.insert(local.to_string());
                    }
                    // Track-B: `import at_radix_ui.react_dialog as Dialog` —
                    // dotted PSX tags rooted at this alias (`Dialog.Root`)
                    // are library components; their props get snake→camel'd.
                    if is_react_or_next_module(&alias.name) {
                        self.react_lib_module_aliases.insert(local.to_string());
                    }
                    // 0.2.2 member-call class fix: a namespace alias of a CORE
                    // React module gets full member routing (camel + module
                    // check + removed check) — see react_namespace_alias_modules.
                    // (The no-alias dotted form `import react_dom.client` never
                    // reaches here — it binds a synthetic grafted head object,
                    // not the package namespace.)
                    if let Some(src) = react::core_react_module(&alias.name) {
                        self.react_namespace_alias_modules
                            .insert(local.to_string(), src);
                    }
                    let module_path = self.resolve_module(&alias.name);
                    // Round-3 unification: the binding DECISION (DX-B2
                    // registration, idempotent dedup, param-shadow rebind,
                    // fix-J unique rename) is `plan_import_binding`'s, shared
                    // with every other import form. A namespace import's
                    // exported symbol is "" (whole module). Planned BEFORE
                    // `declare` — the shadow check reads the declared set.
                    match self.plan_import_binding(local, local, &alias.name, "", &module_path) {
                        ImportBindingPlan::Error => continue,
                        ImportBindingPlan::Dedup => {
                            self.declare(local);
                        }
                        ImportBindingPlan::Fresh => {
                            self.declare(local);
                            // SECURITY (A2): module_path may be a verbatim
                            // `[npm.imports]` override — route through the
                            // escaper. Fix A: sanitize the namespace binding
                            // so `import numpy as default` emits
                            // `import * as default$` (references match).
                            self.writeln(&format!(
                                "import * as {} from {};",
                                Self::sanitize_ident(local),
                                js_string_literal(&module_path)
                            ));
                        }
                        ImportBindingPlan::Alias { unique } => {
                            // DX-B2 alias-and-rewrite: the JS name is claimed
                            // by a DIFFERENT Python name's import — hoist the
                            // namespace under the unique name; references are
                            // rewritten via `import_ref_renames`.
                            self.declare(local);
                            self.writeln(&format!(
                                "import * as {} from {};",
                                unique,
                                js_string_literal(&module_path)
                            ));
                        }
                        ImportBindingPlan::Rebind {
                            js_binding,
                            unique,
                            reassign,
                        } => {
                            // Hoist the module under a UNIQUE top-level name
                            // and bind the alias LOCALLY so it shadows the
                            // param / outer alias for this scope only.
                            // SECURITY (A2): escape the module specifier.
                            self.writeln(&format!(
                                "import * as {} from {};",
                                unique,
                                js_string_literal(&module_path)
                            ));
                            if reassign {
                                // Param / earlier local — Python rebind.
                                self.writeln(&format!("{} = {};", js_binding, unique));
                            } else {
                                self.writeln(&format!("let {} = {};", js_binding, unique));
                            }
                            self.declare(local);
                        }
                    }
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
                        if matches!(
                            a.name.as_str(),
                            "datetime" | "date" | "time" | "timedelta" | "timezone"
                        ) {
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
                    // RELATIVE star (`from .mod import *`): the CLI expands it
                    // into explicit named imports BEFORE codegen (the
                    // commands::relstar pass — the sibling source is on disk,
                    // so its public name set is knowable at compile time). If
                    // one reaches the emitter, the caller skipped that pass
                    // (direct library/embedding use): fail LOUD. The old
                    // behavior emitted NOTHING — a clean compile whose names
                    // exploded as bare ReferenceErrors at runtime with no
                    // hint of the cause (silent miscompile).
                    if *level > 0 {
                        let py_module =
                            format!("{}{}", ".".repeat(*level as usize), module);
                        let diag = format!(
                            "wildcard relative import `from {} import *` was not \
                             expanded — this compilation context has no source-file \
                             access to resolve the sibling module's public names. \
                             Compile through the pyths CLI, or list the imported \
                             names explicitly (`from {} import a, b`).",
                            py_module, py_module
                        );
                        eprintln!("error: {}", diag);
                        self.codegen_errors.push(diag.clone());
                        self.writeln(&format!(
                            "throw new Error({});",
                            js_string_literal(&format!("PythScribe: {}", diag))
                        ));
                        return;
                    }
                    // autotester module_math/module_itertools: a STDLIB
                    // star-import now really BINDS the shim's exports —
                    // the export list is parsed at build time from the
                    // embedded canonical source, each name resolving to
                    // `<ns>.<name>` at reference sites (declared locals
                    // shadow; builtin lowerings are suppressed for bound
                    // names, so `from math import *` makes `pow` math.pow).
                    if *level == 0 && STDLIB_MODULES.contains(&module.as_str()) {
                        if let Some(exports) = stdlib_export_names(module) {
                            let module_path = self.resolve_module(module);
                            let ns = format!("__pyStar{}", self.default_hoist_counter);
                            self.default_hoist_counter += 1;
                            self.writeln(&format!(
                                "import * as {} from {};",
                                ns,
                                js_string_literal(&module_path)
                            ));
                            for (n, is_class) in exports {
                                self.star_import_bindings
                                    .insert(n, (ns.clone(), is_class));
                            }
                            return;
                        }
                    }
                    // Non-stdlib modules keep the Tier C diagnostic: the
                    // silent failure mode was the worst part — the compile
                    // succeeded and every unqualified name later exploded as
                    // a bare runtime ReferenceError with no hint of the
                    // cause. Warn LOUDLY at compile time. Kept a warning
                    // (not a hard error): the common real-world shape is
                    // unused boilerplate, which the namespace-import no-op
                    // handles correctly.
                    if *level == 0
                        && !matches!(module.as_str(), "typing" | "dataclasses" | "pydantic")
                    {
                        eprintln!(
                            "warning: `from {} import *` (star-import) binds names only for \
                             stdlib modules — names from `{}` used unqualified WILL raise \
                             ReferenceError at runtime. Use `import {}` + qualified access or \
                             name the imports explicitly.",
                            module, module, module
                        );
                    }
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

                // Relative imports (`from .foo import x`) bypass the npm-name
                // remapping / pyths.react splitting / stdlib routing. They
                // emit a literal relative ESM specifier computed from the
                // dot-depth + dotted module name; no kebab-casing of the
                // trailing segment (B-006 dodge).
                if *level > 0 {
                    let prefix = "../".repeat((*level - 1) as usize);
                    // BUG #1 root fix: `from . import a` (leading-dot-only
                    // form) names a sibling SUBMODULE, not a symbol — the old
                    // lowering emitted `import { a } from "./"`, asking the
                    // package index to provide ITSELF a named export `a`
                    // (guaranteed ESM link error), and `a.X` then mis-lowered
                    // through pyBoundMethod. The correct lowering is a
                    // MODULE-NAMESPACE import of the submodule file
                    // (`import * as a from "./a"`, extensionless per the
                    // relative-specifier convention), with the binding
                    // registered in `module_namespaces` — the SAME tracking
                    // stdlib `import re` uses — so member access lowers to a
                    // direct property read (`a.X`), capitalized members
                    // `new`-call, and method-table lowerings are suppressed.
                    // Routed through plan_import_binding like every other
                    // import form (DX-B2 collision registration, idempotent
                    // dedup, param-shadow rebind, fix-J rename).
                    //
                    // `from .pkg import name` (module non-empty) stays a
                    // NAMED import: `name` is a symbol of pkg's index there
                    // (the working named-reexport form). A non-empty-module
                    // name that is itself a submodule remains a documented
                    // limitation (needs filesystem knowledge codegen doesn't
                    // have).
                    if module.is_empty() {
                        for a in names {
                            let local = a.alias.as_deref().unwrap_or(&a.name);
                            let sub_path = format!("./{}{}", prefix, a.name);
                            let py_module =
                                format!("{}{}", ".".repeat(*level as usize), a.name);
                            match self.plan_import_binding(
                                local, local, &py_module, "", &sub_path,
                            ) {
                                ImportBindingPlan::Error => return,
                                ImportBindingPlan::Dedup => {
                                    self.declare(local);
                                }
                                ImportBindingPlan::Fresh => {
                                    self.declare(local);
                                    // SECURITY (A2): source-derived specifier —
                                    // route through the escaper.
                                    self.writeln(&format!(
                                        "import * as {} from {};",
                                        Self::sanitize_ident(local),
                                        js_string_literal(&sub_path)
                                    ));
                                }
                                ImportBindingPlan::Alias { unique } => {
                                    self.declare(local);
                                    self.writeln(&format!(
                                        "import * as {} from {};",
                                        unique,
                                        js_string_literal(&sub_path)
                                    ));
                                }
                                ImportBindingPlan::Rebind {
                                    js_binding,
                                    unique,
                                    reassign,
                                } => {
                                    self.writeln(&format!(
                                        "import * as {} from {};",
                                        unique,
                                        js_string_literal(&sub_path)
                                    ));
                                    if reassign {
                                        self.writeln(&format!(
                                            "{} = {};",
                                            js_binding, unique
                                        ));
                                    } else {
                                        self.writeln(&format!(
                                            "let {} = {};",
                                            js_binding, unique
                                        ));
                                    }
                                    self.declare(local);
                                }
                            }
                            self.module_namespaces.insert(local.to_string());
                        }
                        return;
                    }
                    // Module sentinel "." (produced ONLY by the CLI pre-pass
                    // commands::relstar, never by the parser — leading dots
                    // parse into `level`): an FS-verified SYMBOL import from
                    // the package index (`from . import CONST` where CONST
                    // is defined in `__init__` and no submodule file
                    // exists). Lowers to a NAMED import from the index
                    // specifier — the correct pre-existing behavior for
                    // this half of the ambiguous form.
                    let module_path = if module == "." {
                        format!("./{}", prefix)
                    } else {
                        format!("./{}{}", prefix, module.replace('.', "/"))
                    };
                    // Round-3 item 3: relative from-imports route through the
                    // SAME planner as every other import form — they used to
                    // bypass collision/identity registration entirely, so
                    // `from .a import x` + `from .b import x` emitted two
                    // `import { x }` declarations (an ESM parse error where
                    // Python validly rebinds last-wins). Registration keys on
                    // the Python-source dotted form (".a") so the DX-B2
                    // diagnostic reads naturally and can never collide with
                    // an absolute module name.
                    // (Sentinel "." registers under the bare-dots form so a
                    // DX-B2 collision diagnostic reads `from .` not `from ..`.)
                    let py_module = if module == "." {
                        ".".repeat(*level as usize)
                    } else {
                        format!("{}{}", ".".repeat(*level as usize), module)
                    };
                    let mut import_names: Vec<String> = Vec::new();
                    let mut rebinds: Vec<(String, String, bool)> = Vec::new();
                    for a in names {
                        let binding = a.alias.as_deref().unwrap_or(&a.name);
                        match self.plan_import_binding(
                            binding,
                            binding,
                            &py_module,
                            &a.name,
                            &module_path,
                        ) {
                            ImportBindingPlan::Error => return,
                            ImportBindingPlan::Dedup => {}
                            ImportBindingPlan::Fresh => {
                                // Fix A: sanitize the binding (alias or bare name).
                                import_names.push(Self::import_specifier(&a.name, binding));
                            }
                            ImportBindingPlan::Rebind {
                                js_binding,
                                unique,
                                reassign,
                            } => {
                                import_names.push(format!("{} as {}", a.name, unique));
                                rebinds.push((js_binding, unique, reassign));
                            }
                            ImportBindingPlan::Alias { unique } => {
                                // DX-B2 alias-and-rewrite (JS name claimed by a
                                // different Python name's import).
                                import_names.push(format!("{} as {}", a.name, unique));
                            }
                        }
                    }
                    for a in names {
                        let local = a.alias.as_deref().unwrap_or(&a.name);
                        self.declare(local);
                    }
                    // SECURITY (A2): relative specifier is source-derived
                    // (dotted module name) — escape it defensively so no import
                    // specifier is ever built by raw interpolation.
                    if !import_names.is_empty() {
                        self.writeln(&format!(
                            "import {{ {} }} from {};",
                            import_names.join(", "),
                            js_string_literal(&module_path)
                        ));
                    }
                    for (binding, unique, reassign) in rebinds {
                        if reassign {
                            self.writeln(&format!("{} = {};", binding, unique));
                        } else {
                            self.writeln(&format!("let {} = {};", binding, unique));
                        }
                    }
                    return;
                }

                // Skip compile-time-only imports
                if module == "dataclasses" || module == "pydantic" || module == "typing" {
                    return;
                }

                // #448: `from importlib import import_module [as X]`. importlib
                // is not a real module in the compiled output; `import_module`
                // lowers to native ES dynamic `import(spec)` at the call site.
                // Emit NOTHING for the import and register the local name so a
                // call on it routes to the native form. The name is NOT
                // declared, so the unaliased `import_module(...)` also matches
                // the builtin lowering (belt and braces). Any other name
                // imported from importlib is unsupported — diagnose it rather
                // than emit a broken `import { … } from "importlib"`.
                if module == "importlib" || module.starts_with("importlib.") {
                    for a in names {
                        let local = a.alias.as_deref().unwrap_or(&a.name);
                        // `import_module` lives at the importlib TOP level only
                        // — submodule from-imports (`from importlib.util import
                        // …`) are all unsupported.
                        if module == "importlib" && a.name == "import_module" {
                            self.import_module_fns.insert(local.to_string());
                        } else {
                            let diag = format!(
                                "`from {} import {}` is not supported \
                                 (pythscribe-v3.x); only `from importlib import \
                                 import_module` is implemented (→ native dynamic \
                                 `import()`).",
                                module, a.name
                            );
                            eprintln!("error: {}", diag);
                            self.codegen_errors.push(diag);
                        }
                    }
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
                        // WB-12: `js_class` is a COMPILE-TIME class decorator
                        // (like `@dataclass`/`@component`), not a runtime value —
                        // it makes the decorated class emit a plain JS class with
                        // no `extends PyObject` (for foreign-lib interop, e.g.
                        // MobX `makeAutoObservable`, which rejects any class that
                        // has a superclass). Consumed by codegen; the import binds
                        // the name (so the source resolves) but emits nothing.
                        if a.name == "js_class" {
                            self.declare(local);
                            continue;
                        }
                        if STDLIB_MODULES.contains(&a.name.as_str()) {
                            // Round-4 finding 4: route through the shared
                            // planner like every other import form — a
                            // namespace import of a stdlib module (exported
                            // symbol = ""). This gets the DX-B2 collision
                            // diagnostic (`from pyths import math as m` +
                            // `from pyths import json as m` → hard error, not
                            // two immutable `import * as m` = a SyntaxError),
                            // idempotent dedup, and the param-shadow/fix-J
                            // rebinds. Planned BEFORE `declare`.
                            let stdlib_mod =
                                format!("pyths-runtime/stdlib/{}", a.name);
                            match self.plan_import_binding(
                                local,
                                local,
                                &stdlib_mod,
                                "",
                                &stdlib_mod,
                            ) {
                                ImportBindingPlan::Error => continue,
                                ImportBindingPlan::Dedup => {
                                    self.declare(local);
                                }
                                ImportBindingPlan::Fresh => {
                                    self.declare(local);
                                    // Fix A: sanitize the namespace binding.
                                    self.writeln(&format!(
                                        "import * as {} from \"{}\";",
                                        Self::sanitize_ident(local),
                                        stdlib_mod
                                    ));
                                }
                                ImportBindingPlan::Rebind {
                                    js_binding,
                                    unique,
                                    reassign,
                                } => {
                                    self.writeln(&format!(
                                        "import * as {} from \"{}\";",
                                        unique, stdlib_mod
                                    ));
                                    if reassign {
                                        self.writeln(&format!("{} = {};", js_binding, unique));
                                    } else {
                                        self.writeln(&format!(
                                            "let {} = {};",
                                            js_binding, unique
                                        ));
                                    }
                                    self.declare(local);
                                }
                                ImportBindingPlan::Alias { unique } => {
                                    // DX-B2 alias-and-rewrite (JS name claimed
                                    // by a different Python name's import).
                                    self.declare(local);
                                    self.writeln(&format!(
                                        "import * as {} from \"{}\";",
                                        unique, stdlib_mod
                                    ));
                                }
                            }
                        } else {
                            self.declare(local);
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
                    // `pyths.react` is a HYBRID surface whose symbols live in
                    // FOUR genuinely different modules: react core, react-dom,
                    // react-dom/client, and the pyths runtime's codegen-meta
                    // helpers. `react::react_helper_source` is the single
                    // audited routing table (root fix for the WB-22/WB-23
                    // family: a symbol mapped to a module that does not export
                    // it is a load-time crash). Each name still routes through
                    // `plan_import_binding` with its EFFECTIVE module — same
                    // DX-B2 registration, identity dedup, param-shadow rebind,
                    // and fix-J rename as every other import form. Only the
                    // multi-statement SPLIT (one `import` per distinct module,
                    // emitted in a fixed order) is this path's own.
                    use react::ReactHelperSource as Src;
                    // Fixed emission order → deterministic output.
                    let src_order = [
                        Src::ReactCore,
                        Src::ReactDom,
                        Src::ReactDomClient,
                        Src::PythsRuntime,
                    ];
                    // Per-source (resolved module string, specifiers).
                    let mut buckets: Vec<(Src, String, Vec<String>)> = src_order
                        .iter()
                        .map(|s| (*s, self.resolve_module(s.module()), Vec::new()))
                        .collect();
                    let mut rebinds: Vec<(String, String, bool)> = Vec::new();
                    for a in names {
                        // B5: a symbol the WB-22/23 table would route to a module
                        // that no longer exports it in React 19 (findDOMNode) —
                        // emitting the import is a load-time "no such export"
                        // crash. Diagnose and skip the dead import (declaration
                        // still happens below so later refs don't cascade).
                        if let Some(msg) = react::react_19_removed(&a.name) {
                            self.record_codegen_error(msg);
                            continue;
                        }
                        let js_name = react::snake_to_camel(&a.name);
                        let binding = a.alias.clone().unwrap_or_else(|| js_name.clone());
                        // The PYTHON-visible name (pre-conversion) — what the
                        // user's reference sites are written against.
                        let py_local = a.alias.as_deref().unwrap_or(&a.name);
                        let src = react::react_helper_source(&a.name);
                        let bucket = buckets
                            .iter_mut()
                            .find(|(s, _, _)| *s == src)
                            .expect("every ReactHelperSource has a bucket");
                        let eff_mod = bucket.1.clone();
                        match self.plan_import_binding(py_local, &binding, &eff_mod, &js_name, &eff_mod)
                        {
                            ImportBindingPlan::Error => return,
                            ImportBindingPlan::Dedup => {}
                            ImportBindingPlan::Fresh => {
                                bucket.2.push(Self::import_specifier(&js_name, &binding));
                            }
                            ImportBindingPlan::Rebind {
                                js_binding,
                                unique,
                                reassign,
                            } => {
                                bucket.2.push(format!("{} as {}", js_name, unique));
                                rebinds.push((js_binding, unique, reassign));
                            }
                            ImportBindingPlan::Alias { unique } => {
                                // DX-B2 alias-and-rewrite (JS name claimed by
                                // a different Python name's import).
                                bucket.2.push(format!("{} as {}", js_name, unique));
                            }
                        }
                    }
                    // Track declarations + PSX dispatch hints — the SAME
                    // tracking the general react-import path does below. B4: the
                    // hybrid path used to register NONE of the factory-transform
                    // hints, so `from pyths.react import create_element` lowered
                    // its props dict VERBATIM (`{"on_click": 1}` — a dead
                    // handler) while `from react import …` transformed it. Mirror
                    // the general path's `react_lib_bindings` /
                    // `react_create_element_fns` / `react_member_component_bases`
                    // registration so props lower identically on every import
                    // spelling.
                    for a in names {
                        if react::react_19_removed(&a.name).is_some() {
                            continue;
                        }
                        let local = a.alias.as_deref().unwrap_or(&a.name);
                        self.declare(local);
                        if a.alias.is_none() {
                            self.react_imports.insert(a.name.clone());
                        }
                        self.react_lib_bindings.insert(local.to_string());
                        // B4: remember a local bound to React's createElement
                        // factory so a direct call's props dict gets the
                        // PSX-prop snake→camel/kebab transform (alias-aware).
                        if a.name == "create_element" || a.name == "createElement" {
                            self.react_create_element_fns.insert(local.to_string());
                        }
                        if a.name == "motion" {
                            self.react_member_component_bases.insert(local.to_string());
                        }
                    }
                    for (_src, module_str, specs) in &buckets {
                        if !specs.is_empty() {
                            // SECURITY (#414/A2): module_str is `resolve_module`'s
                            // output — it may be a verbatim `[npm.imports]`
                            // config override (untrusted). Escape it exactly like
                            // the sibling general-path site ~120 lines down; the
                            // old raw `"{}"` interpolation let a `"`/newline in
                            // the override break out of the specifier string.
                            self.writeln(&format!(
                                "import {{ {} }} from {};",
                                specs.join(", "),
                                js_string_literal(module_str)
                            ));
                        }
                    }
                    // Round-3 item 2: a binding that is ALSO a param/earlier
                    // local REASSIGNS (`hook = __pyimp_hook_0;`) — the old
                    // unconditional `const` was a redeclaration SyntaxError
                    // inside `def f(hook): from pyths.react import
                    // use_effect as hook`.
                    for (binding, unique, reassign) in rebinds {
                        if reassign {
                            self.writeln(&format!("{} = {};", binding, unique));
                        } else {
                            self.writeln(&format!("let {} = {};", binding, unique));
                        }
                    }
                    return;
                }

                // B5 scoping (0.2.2 class fix): the React-19-removed diagnostic
                // fires only for the CORE React packages the removal actually
                // happened in (react / react-dom / react-dom/client; the
                // pyths.react hybrid has its own check above). It used to key
                // on `is_react_module` — the WHOLE react ecosystem — which,
                // with the full removed set (`render`, `hydrate`, …), would
                // misfire on legitimate exports of other packages
                // (`from at_testing_library.react import render`).
                let react_19_removed_scope = react::core_react_module(module).is_some();
                let module_path = self.resolve_module(module);
                // Round-3 unification: every name routes through
                // `plan_import_binding` — DX-B2 collision registration,
                // idempotent-re-import dedup, the param-shadow rebind
                // (finding 5), and the fix-J unique renames all live THERE,
                // shared with plain imports, the recognized-lib hybrid path,
                // and relative imports. Only the named-specifier SYNTAX is
                // this path's own. rebinds: (js_binding, unique, reassign).
                let mut import_names: Vec<String> = Vec::new();
                let mut rebinds: Vec<(String, String, bool)> = Vec::new();
                for a in names {
                    // B5: a React symbol removed in React 19 (findDOMNode,
                    // render, hydrate, unmountComponentAtNode, createFactory) —
                    // routing it emits a dead `import { … } from …` that fails
                    // to load. Diagnose and skip, scoped to the CORE React
                    // packages (see react_19_removed_scope above).
                    if react_19_removed_scope {
                        if let Some(msg) = react::react_19_removed(&a.name) {
                            self.record_codegen_error(msg);
                            continue;
                        }
                    }
                    let js_name = if is_react_module {
                        react::snake_to_camel(&a.name)
                    } else {
                        a.name.clone()
                    };
                    let binding = a.alias.clone().unwrap_or_else(|| js_name.clone());
                    // The PYTHON-visible name (pre-conversion) — the name the
                    // user's reference sites are written against. It differs
                    // from `binding` exactly when our snake→camel conversion
                    // renamed an unaliased import (`create_store` →
                    // `createStore`), which is what makes the DX-B2
                    // alias-and-rewrite class detectable.
                    let py_local = a.alias.as_deref().unwrap_or(&a.name);
                    match self.plan_import_binding(py_local, &binding, module, &js_name, &module_path)
                    {
                        ImportBindingPlan::Error => return,
                        ImportBindingPlan::Dedup => {}
                        ImportBindingPlan::Fresh => {
                            // Fix A: sanitize the binding so a reserved-word
                            // import (`from m import x as default`) emits
                            // `x as default$`, matching references.
                            import_names.push(Self::import_specifier(&js_name, &binding));
                        }
                        ImportBindingPlan::Rebind {
                            js_binding,
                            unique,
                            reassign,
                        } => {
                            import_names.push(format!("{} as {}", js_name, unique));
                            rebinds.push((js_binding, unique, reassign));
                        }
                        ImportBindingPlan::Alias { unique } => {
                            // DX-B2 alias-and-rewrite: hoist under the unique
                            // name only — no body rebind; reference sites for
                            // this Python name are rewritten at emission.
                            import_names.push(format!("{} as {}", js_name, unique));
                        }
                    }
                }
                // Every name was already imported — emit nothing (the bindings
                // are all in scope). Side-effect tracking below still runs.
                if import_names.is_empty() && rebinds.is_empty() {
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
                    // B5: keep a React-19-removed symbol out of the tracking
                    // sets too — it was diagnosed and never imported above.
                    if react_19_removed_scope && react::react_19_removed(&a.name).is_some() {
                        continue;
                    }
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
                        // TB-1: remember a local bound to React's createElement
                        // factory (`create_element`/`createElement`, alias-aware)
                        // so a direct call's props dict is transformed as
                        // PSX-props — the only dict-literal position that gets
                        // the snake→camel/kebab prop-name transform.
                        if a.name == "create_element" || a.name == "createElement" {
                            self.react_create_element_fns.insert(local.to_string());
                        }
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
                // SECURITY (A2): module_path may be a verbatim `[npm.imports]`
                // override value — config-derived, untrusted. Escape it.
                if !import_names.is_empty() {
                    self.writeln(&format!(
                        "import {{ {} }} from {};",
                        import_names.join(", "),
                        js_string_literal(&module_path)
                    ));
                }
                // Fix J: body-local re-binds for cross-scope alias collisions
                // (function-local; stays in the body, shadows the outer binding).
                // A param/earlier-local shadow REASSIGNS; a fresh collision
                // introduces a `let` (finding 5) — `let`, not `const`, so a
                // THIRD import of the same name can reassign it (Python
                // last-wins chains stay valid JS).
                for (binding, unique, is_reassign) in rebinds {
                    if is_reassign {
                        self.writeln(&format!("{} = {};", binding, unique));
                    } else {
                        self.writeln(&format!("let {} = {};", binding, unique));
                    }
                }
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
                }
                // A bare `assert x` raises AssertionError() with NO message
                // (CPython) — repr is AssertionError(), not
                // AssertionError('Assertion failed').
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
                            // Finding 1: `del x` unbinds the name — a later
                            // re-import must re-emit, not dedup.
                            self.invalidate_import_decl(name);
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
                // Finding 1: tuple-unpack targets are non-import rebinds too.
                self.invalidate_import_decl(n);
                if !self.is_declared(n) {
                    self.write_indent();
                    // Module-level unpack targets EXPORT, exactly like plain
                    // module-level assignments (B-015) and AnnAssign — they
                    // are ordinary Python module globals. This was the one
                    // binding form the export model missed: `x, y = 1, 2`
                    // compiled to un-exported `let`s, so `from .m import x`
                    // link-failed per-module and (worse) bound `undefined`
                    // in a bundle. `export let n;` followed by the
                    // destructuring assignment is valid ESM.
                    if self.indent == 0 {
                        self.write("export ");
                    }
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
        // autotester properties: `A.q = 5678` AFTER class creation — a plain
        // JS property on the class object is invisible to instances (their
        // lookup walks the prototype chain). Route static class-attribute
        // assignment through __pyClassAttr, the same installer class-body
        // attributes use (class prop + live prototype accessor = Python
        // attribute lookup).
        if let ExprKind::Attribute {
            value: obj,
            attr,
            optional: false,
        } = &target.kind
        {
            if let ExprKind::Name(n) = &obj.kind {
                if self.known_classes.contains(n) && !attr.starts_with("__") {
                    self.need_runtime("__pyClassAttr");
                    self.write(&format!(
                        "__pyClassAttr({}, \"{}\", ",
                        Self::sanitize_ident(n),
                        attr
                    ));
                    self.emit_expr(value);
                    self.write(");\n");
                    return;
                }
            }
        }
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
            // Finding 1: a non-import rebind breaks any import identity this
            // scope cached for the name — a later re-import must re-emit.
            self.invalidate_import_decl(name);
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

    /// Order-independent scope binding PRE-PASS (issue #438). Collects EVERY
    /// name bound in `body` as a local of this scope: assign / aug-assign /
    /// ann-assign targets (incl. tuple/list/starred), walrus (`:=`), `import` &
    /// `from-import` aliases, `def`/`class` names, for-loop targets, `with ...
    /// as`, `except ... as`, and match capture patterns — descending through
    /// control-flow blocks (if/for/while/with/try/match) but NOT into nested
    /// def/class/lambda scopes. `params` seed the set. Names declared
    /// `global`/`nonlocal` are EXCLUDED (they resolve at module/builtin scope,
    /// so e.g. `global len` restores the `len` builtin lowering — case H).
    ///
    /// Consulted by `is_declared_in_any_scope` so a builtin shadowed by a local
    /// is resolved to the local regardless of SOURCE ORDER — closing DX-B1
    /// (forward-reference + param shadow), E (comprehension targets), F (a
    /// later-declared enclosing binding seen by an inner def), G (class methods),
    /// and H (`global` builtin fallback) at the root.
    fn collect_local_bindings(body: &[Stmt], params: &[String]) -> HashSet<String> {
        let mut bound: HashSet<String> = params.iter().cloned().collect();
        Self::collect_bound_names(body, &mut bound);
        for g in Self::collect_global_names(body) {
            bound.remove(&g);
        }
        bound
    }

    /// Issue #438 (case E): the for-target names bound by a comprehension's
    /// generators — these bind the comprehension's OWN scope, shadowing a
    /// builtin inside the element/condition (`[len(x) for len in xs]`).
    ///
    /// Review edge: the OUTERMOST (leftmost) iterable is evaluated in the
    /// ENCLOSING scope, so a target must NOT shadow a name referenced there —
    /// `[x for len in len([1,2,3])]`'s leftmost `len(...)` is the builtin, not
    /// the `len` target. Names referenced in `generators[0].iter` are therefore
    /// excluded from the comprehension's local binding set.
    fn comprehension_target_names(generators: &[Comprehension]) -> HashSet<String> {
        fn tnames(e: &Expr, out: &mut HashSet<String>) {
            match &e.kind {
                ExprKind::Name(n) => {
                    out.insert(n.clone());
                }
                ExprKind::Tuple(elts) | ExprKind::List(elts) => {
                    for x in elts {
                        tnames(x, out);
                    }
                }
                ExprKind::Starred(inner) => tnames(inner, out),
                _ => {}
            }
        }
        let mut out = HashSet::new();
        for g in generators {
            tnames(&g.target, &mut out);
        }
        out
    }

    /// #441: collect the TOPMOST walrus (`NamedExpr`) nodes of an expression,
    /// in source order — the def-time-evaluated side effects of a function
    /// ANNOTATION. CPython evaluates every annotation expression when the
    /// `def` statement executes; PythScribe erases annotations (type names
    /// routinely have no JS runtime binding), so the OBSERVABLE part — each
    /// walrus assignment — is extracted and emitted as a def-site statement
    /// instead. Topmost only: emitting an outer walrus emits any nested one
    /// as part of its value. Does NOT enter a lambda body (its walrus runs
    /// at call time) or a comprehension (its walrus runs per element when
    /// the comprehension itself runs — not extractable as a one-shot
    /// def-site statement; such targets still HOIST via walrus_in_expr, so
    /// scoping stays sound).
    fn collect_named_exprs<'e>(expr: &'e Expr, out: &mut Vec<&'e Expr>) {
        use ExprKind as E;
        match &expr.kind {
            E::NamedExpr { .. } => out.push(expr),
            E::BinOp { left, right, .. } => {
                Self::collect_named_exprs(left, out);
                Self::collect_named_exprs(right, out);
            }
            E::UnaryOp { operand, .. } => Self::collect_named_exprs(operand, out),
            E::Compare { left, comparisons } => {
                Self::collect_named_exprs(left, out);
                for (_, e) in comparisons {
                    Self::collect_named_exprs(e, out);
                }
            }
            E::Call {
                func, args, kwargs, ..
            } => {
                Self::collect_named_exprs(func, out);
                for a in args {
                    Self::collect_named_exprs(a, out);
                }
                for k in kwargs {
                    Self::collect_named_exprs(&k.value, out);
                }
            }
            E::Attribute { value, .. } => Self::collect_named_exprs(value, out),
            E::Subscript { value, index, .. } => {
                Self::collect_named_exprs(value, out);
                Self::collect_named_exprs(index, out);
            }
            E::Slice { lower, upper, step } => {
                for e in [lower, upper, step].into_iter().flatten() {
                    Self::collect_named_exprs(e, out);
                }
            }
            E::List(elts) | E::Tuple(elts) | E::Set(elts) => {
                for e in elts {
                    Self::collect_named_exprs(e, out);
                }
            }
            E::Dict { items } => {
                for it in items {
                    match it {
                        DictItem::KeyValue { key, value } => {
                            Self::collect_named_exprs(key, out);
                            Self::collect_named_exprs(value, out);
                        }
                        DictItem::Spread(e) => Self::collect_named_exprs(e, out),
                    }
                }
            }
            E::FString { parts } => {
                for p in parts {
                    if let FStringPart::Expr(e) = p {
                        Self::collect_named_exprs(e, out);
                    }
                }
            }
            E::IfExpr {
                test,
                body,
                else_body,
            } => {
                Self::collect_named_exprs(test, out);
                Self::collect_named_exprs(body, out);
                Self::collect_named_exprs(else_body, out);
            }
            E::Starred(e) | E::Await(e) => Self::collect_named_exprs(e, out),
            _ => {}
        }
    }

    /// Collect walrus (`x := ...`) target names anywhere in an expression.
    /// PEP 572: `:=` binds the ENCLOSING function scope (comprehensions
    /// included), so these are locals of the surrounding function.
    fn collect_walrus_targets(expr: &Expr, out: &mut HashSet<String>) {
        match &expr.kind {
            ExprKind::NamedExpr { target, value } => {
                if let ExprKind::Name(n) = &target.kind {
                    out.insert(n.clone());
                }
                Self::collect_walrus_targets(value, out);
            }
            ExprKind::BinOp { left, right, .. } => {
                Self::collect_walrus_targets(left, out);
                Self::collect_walrus_targets(right, out);
            }
            ExprKind::UnaryOp { operand, .. } => Self::collect_walrus_targets(operand, out),
            ExprKind::Compare { left, comparisons } => {
                Self::collect_walrus_targets(left, out);
                for (_, e) in comparisons {
                    Self::collect_walrus_targets(e, out);
                }
            }
            ExprKind::Call {
                func, args, kwargs, ..
            } => {
                Self::collect_walrus_targets(func, out);
                for a in args {
                    Self::collect_walrus_targets(a, out);
                }
                for k in kwargs {
                    Self::collect_walrus_targets(&k.value, out);
                }
            }
            ExprKind::Attribute { value, .. } => Self::collect_walrus_targets(value, out),
            ExprKind::Subscript { value, index, .. } => {
                Self::collect_walrus_targets(value, out);
                Self::collect_walrus_targets(index, out);
            }
            ExprKind::List(elts) | ExprKind::Tuple(elts) | ExprKind::Set(elts) => {
                for e in elts {
                    Self::collect_walrus_targets(e, out);
                }
            }
            // Review finding 4: walrus inside dicts / f-strings / slices.
            ExprKind::Dict { items } => {
                for it in items {
                    match it {
                        DictItem::KeyValue { key, value } => {
                            Self::collect_walrus_targets(key, out);
                            Self::collect_walrus_targets(value, out);
                        }
                        DictItem::Spread(e) => Self::collect_walrus_targets(e, out),
                    }
                }
            }
            ExprKind::FString { parts } => {
                for p in parts {
                    if let FStringPart::Expr(e) = p {
                        Self::collect_walrus_targets(e, out);
                    }
                }
            }
            ExprKind::Slice { lower, upper, step } => {
                for e in [lower, upper, step].into_iter().flatten() {
                    Self::collect_walrus_targets(e, out);
                }
            }
            ExprKind::IfExpr {
                test,
                body,
                else_body,
            } => {
                Self::collect_walrus_targets(test, out);
                Self::collect_walrus_targets(body, out);
                Self::collect_walrus_targets(else_body, out);
            }
            ExprKind::ListComp { elt, generators }
            | ExprKind::SetComp { elt, generators }
            | ExprKind::GeneratorExp { elt, generators } => {
                Self::collect_walrus_targets(elt, out);
                for g in generators {
                    Self::collect_walrus_targets(&g.iter, out);
                    for c in &g.ifs {
                        Self::collect_walrus_targets(c, out);
                    }
                }
            }
            ExprKind::DictComp {
                key,
                value,
                generators,
            } => {
                Self::collect_walrus_targets(key, out);
                Self::collect_walrus_targets(value, out);
                for g in generators {
                    Self::collect_walrus_targets(&g.iter, out);
                    for c in &g.ifs {
                        Self::collect_walrus_targets(c, out);
                    }
                }
            }
            ExprKind::Starred(e) | ExprKind::Await(e) | ExprKind::YieldFrom(e) => {
                Self::collect_walrus_targets(e, out)
            }
            ExprKind::Yield(Some(e)) => Self::collect_walrus_targets(e, out),
            _ => {}
        }
    }

    /// Names bound by a match `case` pattern (capture / star / as / nested).
    fn pattern_bound_names(pat: &Pattern, out: &mut HashSet<String>) {
        match pat {
            Pattern::Capture(n) => {
                out.insert(n.clone());
            }
            Pattern::Star(Some(n)) => {
                out.insert(n.clone());
            }
            Pattern::As { pattern, name } => {
                out.insert(name.clone());
                Self::pattern_bound_names(pattern, out);
            }
            Pattern::Class { args, .. } => {
                for a in args {
                    Self::pattern_bound_names(a, out);
                }
            }
            Pattern::Sequence(ps) | Pattern::Or(ps) => {
                for p in ps {
                    Self::pattern_bound_names(p, out);
                }
            }
            Pattern::Mapping(entries) => {
                for (_, p) in entries {
                    Self::pattern_bound_names(p, out);
                }
            }
            _ => {}
        }
    }

    /// #452/#453 (naming soundness): collect EVERY identifier the module can
    /// surface as a bare JS name — `Name` references and binding names alike,
    /// at ANY nesting depth. The matches are exhaustive (no `_` arm on the
    /// node enums), so a new AST variant fails compilation here instead of
    /// silently leaking a name past `fresh_temp`'s freshness guarantee.
    ///
    /// Deliberately far larger than the usual visitor: the length IS the
    /// safety mechanism — a compact `_`-defaulted walker would compile
    /// silently while missing a node kind, and a single missed `Name` voids
    /// the freshness invariant. Compiler-enforced totality > brevity here.
    fn collect_all_idents(body: &[Stmt], out: &mut HashSet<String>) {
        for stmt in body {
            Self::collect_idents_stmt(stmt, out);
        }
    }

    fn collect_idents_stmt(stmt: &Stmt, out: &mut HashSet<String>) {
        match &stmt.kind {
            StmtKind::Expr(e) => Self::collect_idents_expr(e, out),
            StmtKind::Assign { targets, value } => {
                for t in targets {
                    Self::collect_idents_expr(t, out);
                }
                Self::collect_idents_expr(value, out);
            }
            StmtKind::AugAssign { target, op: _, value } => {
                Self::collect_idents_expr(target, out);
                Self::collect_idents_expr(value, out);
            }
            StmtKind::FuncDef {
                name,
                params,
                body,
                decorator_list,
                return_type,
                is_async: _,
            } => {
                out.insert(name.clone());
                for p in params {
                    Self::collect_idents_param(p, out);
                }
                Self::collect_all_idents(body, out);
                for d in decorator_list {
                    Self::collect_idents_expr(d, out);
                }
                if let Some(rt) = return_type {
                    Self::collect_idents_expr(rt, out);
                }
            }
            StmtKind::ClassDef {
                name,
                bases,
                body,
                decorator_list,
            } => {
                out.insert(name.clone());
                for b in bases {
                    Self::collect_idents_expr(b, out);
                }
                Self::collect_all_idents(body, out);
                for d in decorator_list {
                    Self::collect_idents_expr(d, out);
                }
            }
            StmtKind::Return(v) => {
                if let Some(v) = v {
                    Self::collect_idents_expr(v, out);
                }
            }
            StmtKind::If {
                test,
                body,
                elif_clauses,
                else_body,
            } => {
                Self::collect_idents_expr(test, out);
                Self::collect_all_idents(body, out);
                for (c, b) in elif_clauses {
                    Self::collect_idents_expr(c, out);
                    Self::collect_all_idents(b, out);
                }
                if let Some(b) = else_body {
                    Self::collect_all_idents(b, out);
                }
            }
            StmtKind::While {
                test,
                body,
                else_body,
            } => {
                Self::collect_idents_expr(test, out);
                Self::collect_all_idents(body, out);
                if let Some(b) = else_body {
                    Self::collect_all_idents(b, out);
                }
            }
            StmtKind::For {
                target,
                iter,
                body,
                else_body,
                is_async: _,
            } => {
                Self::collect_idents_expr(target, out);
                Self::collect_idents_expr(iter, out);
                Self::collect_all_idents(body, out);
                if let Some(b) = else_body {
                    Self::collect_all_idents(b, out);
                }
            }
            StmtKind::Break | StmtKind::Continue | StmtKind::Pass => {}
            StmtKind::Import { names } => {
                for a in names {
                    match &a.alias {
                        Some(alias) => {
                            out.insert(alias.clone());
                        }
                        // `import a.b.c` binds the FIRST segment.
                        None => {
                            if let Some(first) = a.name.split('.').next() {
                                out.insert(first.to_string());
                            }
                        }
                    }
                }
            }
            StmtKind::ImportSideEffect(_) => {}
            StmtKind::ImportFrom {
                module: _,
                names,
                level: _,
            } => {
                for a in names {
                    if a.name != "*" {
                        let local = a.alias.as_deref().unwrap_or(&a.name);
                        out.insert(local.to_string());
                    }
                }
            }
            StmtKind::Try {
                body,
                handlers,
                else_body,
                finally_body,
            } => {
                Self::collect_all_idents(body, out);
                for h in handlers {
                    if let Some(t) = &h.exc_type {
                        Self::collect_idents_expr(t, out);
                    }
                    if let Some(n) = &h.name {
                        out.insert(n.clone());
                    }
                    Self::collect_all_idents(&h.body, out);
                }
                if let Some(b) = else_body {
                    Self::collect_all_idents(b, out);
                }
                if let Some(b) = finally_body {
                    Self::collect_all_idents(b, out);
                }
            }
            StmtKind::Raise(value, cause) => {
                for e in [value, cause].into_iter().flatten() {
                    Self::collect_idents_expr(e, out);
                }
            }
            StmtKind::Assert { test, msg } => {
                Self::collect_idents_expr(test, out);
                if let Some(m) = msg {
                    Self::collect_idents_expr(m, out);
                }
            }
            StmtKind::Global(names) | StmtKind::Nonlocal(names) => {
                for n in names {
                    out.insert(n.clone());
                }
            }
            StmtKind::Del(exprs) => {
                for e in exprs {
                    Self::collect_idents_expr(e, out);
                }
            }
            StmtKind::With {
                items,
                body,
                is_async: _,
            } => {
                for item in items {
                    Self::collect_idents_expr(&item.context_expr, out);
                    if let Some(v) = &item.optional_var {
                        Self::collect_idents_expr(v, out);
                    }
                }
                Self::collect_all_idents(body, out);
            }
            StmtKind::AnnAssign {
                target,
                annotation,
                value,
            } => {
                Self::collect_idents_expr(target, out);
                Self::collect_idents_expr(annotation, out);
                if let Some(v) = value {
                    Self::collect_idents_expr(v, out);
                }
            }
            StmtKind::Match { subject, cases } => {
                Self::collect_idents_expr(subject, out);
                for c in cases {
                    Self::collect_idents_pattern(&c.pattern, out);
                    if let Some(g) = &c.guard {
                        Self::collect_idents_expr(g, out);
                    }
                    Self::collect_all_idents(&c.body, out);
                }
            }
        }
    }

    fn collect_idents_param(p: &Param, out: &mut HashSet<String>) {
        out.insert(p.name.clone());
        if let Some(a) = &p.annotation {
            Self::collect_idents_expr(a, out);
        }
        if let Some(d) = &p.default {
            Self::collect_idents_expr(d, out);
        }
    }

    fn collect_idents_pattern(pat: &Pattern, out: &mut HashSet<String>) {
        match pat {
            Pattern::Wildcard => {}
            Pattern::Capture(n) => {
                out.insert(n.clone());
            }
            Pattern::Literal(e) | Pattern::Value(e) => Self::collect_idents_expr(e, out),
            Pattern::Class { cls, args } => {
                out.insert(cls.clone());
                for a in args {
                    Self::collect_idents_pattern(a, out);
                }
            }
            Pattern::Sequence(ps) | Pattern::Or(ps) => {
                for p in ps {
                    Self::collect_idents_pattern(p, out);
                }
            }
            Pattern::Mapping(entries) => {
                for (k, p) in entries {
                    Self::collect_idents_expr(k, out);
                    Self::collect_idents_pattern(p, out);
                }
            }
            Pattern::As { pattern, name } => {
                out.insert(name.clone());
                Self::collect_idents_pattern(pattern, out);
            }
            Pattern::Star(n) => {
                if let Some(n) = n {
                    out.insert(n.clone());
                }
            }
        }
    }

    fn collect_idents_expr(e: &Expr, out: &mut HashSet<String>) {
        match &e.kind {
            ExprKind::IntLiteral(_)
            | ExprKind::FloatLiteral(_)
            | ExprKind::ImagLiteral(_)
            | ExprKind::StringLiteral(_)
            | ExprKind::BytesLiteral(_)
            | ExprKind::BoolLiteral(_)
            | ExprKind::NoneLiteral => {}
            ExprKind::FString { parts } => {
                for p in parts {
                    if let FStringPart::Expr(e) = p {
                        Self::collect_idents_expr(e, out);
                    }
                }
            }
            ExprKind::Name(n) => {
                out.insert(n.clone());
            }
            ExprKind::BinOp { left, op: _, right } => {
                Self::collect_idents_expr(left, out);
                Self::collect_idents_expr(right, out);
            }
            ExprKind::UnaryOp { op: _, operand } => Self::collect_idents_expr(operand, out),
            ExprKind::Compare { left, comparisons } => {
                Self::collect_idents_expr(left, out);
                for (_, c) in comparisons {
                    Self::collect_idents_expr(c, out);
                }
            }
            ExprKind::Call {
                func,
                args,
                kwargs,
                optional: _,
            } => {
                Self::collect_idents_expr(func, out);
                for a in args {
                    Self::collect_idents_expr(a, out);
                }
                for k in kwargs {
                    Self::collect_idents_expr(&k.value, out);
                }
            }
            ExprKind::Attribute {
                value,
                attr: _,
                optional: _,
            } => Self::collect_idents_expr(value, out),
            ExprKind::Subscript {
                value,
                index,
                optional: _,
            } => {
                Self::collect_idents_expr(value, out);
                Self::collect_idents_expr(index, out);
            }
            ExprKind::Slice { lower, upper, step } => {
                for e in [lower, upper, step].into_iter().flatten() {
                    Self::collect_idents_expr(e, out);
                }
            }
            ExprKind::List(elts) | ExprKind::Tuple(elts) | ExprKind::Set(elts) => {
                for e in elts {
                    Self::collect_idents_expr(e, out);
                }
            }
            ExprKind::Dict { items } => {
                for item in items {
                    match item {
                        DictItem::KeyValue { key, value } => {
                            Self::collect_idents_expr(key, out);
                            Self::collect_idents_expr(value, out);
                        }
                        DictItem::Spread(e) => Self::collect_idents_expr(e, out),
                    }
                }
            }
            ExprKind::ListComp { elt, generators }
            | ExprKind::SetComp { elt, generators }
            | ExprKind::GeneratorExp { elt, generators } => {
                Self::collect_idents_expr(elt, out);
                for g in generators {
                    Self::collect_idents_expr(&g.target, out);
                    Self::collect_idents_expr(&g.iter, out);
                    for c in &g.ifs {
                        Self::collect_idents_expr(c, out);
                    }
                }
            }
            ExprKind::DictComp {
                key,
                value,
                generators,
            } => {
                Self::collect_idents_expr(key, out);
                Self::collect_idents_expr(value, out);
                for g in generators {
                    Self::collect_idents_expr(&g.target, out);
                    Self::collect_idents_expr(&g.iter, out);
                    for c in &g.ifs {
                        Self::collect_idents_expr(c, out);
                    }
                }
            }
            ExprKind::Lambda { params, body } => {
                for p in params {
                    Self::collect_idents_param(p, out);
                }
                Self::collect_idents_expr(body, out);
            }
            ExprKind::IfExpr {
                test,
                body,
                else_body,
            } => {
                Self::collect_idents_expr(test, out);
                Self::collect_idents_expr(body, out);
                Self::collect_idents_expr(else_body, out);
            }
            ExprKind::Starred(e) | ExprKind::Await(e) | ExprKind::YieldFrom(e) => {
                Self::collect_idents_expr(e, out)
            }
            ExprKind::Yield(v) => {
                if let Some(v) = v {
                    Self::collect_idents_expr(v, out);
                }
            }
            ExprKind::NamedExpr { target, value } => {
                Self::collect_idents_expr(target, out);
                Self::collect_idents_expr(value, out);
            }
        }
    }

    /// Statement walk for `collect_local_bindings` — records every bound name,
    /// descending into control-flow bodies but NOT nested def/class scopes.
    fn collect_bound_names(body: &[Stmt], out: &mut HashSet<String>) {
        fn tnames(e: &Expr, out: &mut HashSet<String>) {
            match &e.kind {
                ExprKind::Name(n) => {
                    out.insert(n.clone());
                }
                ExprKind::Tuple(elts) | ExprKind::List(elts) => {
                    for x in elts {
                        tnames(x, out);
                    }
                }
                ExprKind::Starred(inner) => tnames(inner, out),
                _ => {} // attribute / subscript targets bind no new local
            }
        }
        for s in body {
            match &s.kind {
                StmtKind::Assign { targets, value } => {
                    for t in targets {
                        tnames(t, out);
                        Self::collect_walrus_targets(t, out);
                    }
                    Self::collect_walrus_targets(value, out);
                }
                StmtKind::AnnAssign {
                    target,
                    annotation,
                    value,
                } => {
                    // The annotated TARGET is a static local even with no
                    // value (`len: int` alone → CPython UnboundLocalError on
                    // a later read; PEP 526).
                    tnames(target, out);
                    // Round-3 review: a walrus inside the ANNOTATION
                    // expression (`x: (len := int)`) also binds this scope
                    // statically — the annotation itself is never evaluated
                    // in a function body, but its walrus target is a local
                    // in the symbol table, so a later `len(...)` is an
                    // unbound-local read, NOT the builtin.
                    Self::collect_walrus_targets(annotation, out);
                    if let Some(v) = value {
                        Self::collect_walrus_targets(v, out);
                    }
                }
                StmtKind::AugAssign { target, value, .. } => {
                    tnames(target, out);
                    Self::collect_walrus_targets(value, out);
                }
                StmtKind::Import { names } | StmtKind::ImportFrom { names, .. } => {
                    for a in names {
                        if a.name == "*" {
                            continue;
                        }
                        out.insert(a.alias.clone().unwrap_or_else(|| a.name.clone()));
                    }
                }
                StmtKind::FuncDef {
                    name,
                    params,
                    decorator_list,
                    return_type,
                    ..
                } => {
                    out.insert(name.clone());
                    // Review finding 4 + round 3: a nested def's default args,
                    // decorators, and ANNOTATIONS (param + return) are
                    // evaluated in THIS (enclosing) scope at def time, so a
                    // walrus there (`def inner(x=(y := ...))`, `@(z := ...)`,
                    // `def inner(x: (w := ...))`) binds this scope. Do NOT
                    // descend into the nested body/params.
                    for p in params {
                        if let Some(d) = &p.default {
                            Self::collect_walrus_targets(d, out);
                        }
                        if let Some(ann) = &p.annotation {
                            Self::collect_walrus_targets(ann, out);
                        }
                    }
                    for d in decorator_list {
                        Self::collect_walrus_targets(d, out);
                    }
                    if let Some(rt) = return_type {
                        Self::collect_walrus_targets(rt, out);
                    }
                }
                StmtKind::ClassDef {
                    name,
                    bases,
                    decorator_list,
                    ..
                } => {
                    out.insert(name.clone());
                    // A nested class's bases and decorators are evaluated in the
                    // enclosing scope too.
                    for b in bases {
                        Self::collect_walrus_targets(b, out);
                    }
                    for d in decorator_list {
                        Self::collect_walrus_targets(d, out);
                    }
                }
                StmtKind::For {
                    target,
                    iter,
                    body,
                    else_body,
                    ..
                } => {
                    tnames(target, out);
                    Self::collect_walrus_targets(iter, out);
                    Self::collect_bound_names(body, out);
                    if let Some(e) = else_body {
                        Self::collect_bound_names(e, out);
                    }
                }
                StmtKind::If {
                    test,
                    body,
                    elif_clauses,
                    else_body,
                } => {
                    Self::collect_walrus_targets(test, out);
                    Self::collect_bound_names(body, out);
                    for (c, b) in elif_clauses {
                        Self::collect_walrus_targets(c, out);
                        Self::collect_bound_names(b, out);
                    }
                    if let Some(e) = else_body {
                        Self::collect_bound_names(e, out);
                    }
                }
                StmtKind::While {
                    test,
                    body,
                    else_body,
                } => {
                    Self::collect_walrus_targets(test, out);
                    Self::collect_bound_names(body, out);
                    if let Some(e) = else_body {
                        Self::collect_bound_names(e, out);
                    }
                }
                StmtKind::With { items, body, .. } => {
                    for it in items {
                        Self::collect_walrus_targets(&it.context_expr, out);
                        if let Some(ov) = &it.optional_var {
                            tnames(ov, out);
                        }
                    }
                    Self::collect_bound_names(body, out);
                }
                StmtKind::Try {
                    body,
                    handlers,
                    else_body,
                    finally_body,
                } => {
                    Self::collect_bound_names(body, out);
                    for h in handlers {
                        if let Some(n) = &h.name {
                            out.insert(n.clone());
                        }
                        Self::collect_bound_names(&h.body, out);
                    }
                    if let Some(e) = else_body {
                        Self::collect_bound_names(e, out);
                    }
                    if let Some(f) = finally_body {
                        Self::collect_bound_names(f, out);
                    }
                }
                StmtKind::Match { subject, cases } => {
                    Self::collect_walrus_targets(subject, out);
                    for c in cases {
                        Self::pattern_bound_names(&c.pattern, out);
                        // Review finding 4: walrus inside a match guard.
                        if let Some(g) = &c.guard {
                            Self::collect_walrus_targets(g, out);
                        }
                        Self::collect_bound_names(&c.body, out);
                    }
                }
                StmtKind::Return(Some(e))
                | StmtKind::Expr(e)
                | StmtKind::Raise(Some(e), _) => {
                    Self::collect_walrus_targets(e, out);
                }
                StmtKind::Assert { test, msg } => {
                    Self::collect_walrus_targets(test, out);
                    if let Some(m) = msg {
                        Self::collect_walrus_targets(m, out);
                    }
                }
                // Review finding 4: `del name` makes `name` a LOCAL of this
                // scope (Python static scoping), so a subsequent reference is an
                // unbound local, NOT the builtin.
                StmtKind::Del(exprs) => {
                    for e in exprs {
                        tnames(e, out);
                    }
                }
                // Global/Nonlocal are handled by the exclusion pass; Pass/Break/
                // Continue/etc. bind nothing.
                _ => {}
            }
        }
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

    /// #443: every name bound by an `import` / `from-import` in this body
    /// (any nesting depth, not descending into def/class scopes). A NON-
    /// import binding form that rebinds one of these names needs the
    /// import emitted ASSIGNABLY (hoisted `let` + unique import + assign),
    /// or the rebind hits an immutable ESM binding (`with CM() as floor`
    /// after `from math import floor` threw "Assignment to constant
    /// variable"; `def floor` was a redeclaration SyntaxError).
    /// Also consulted by emit_module (B8b) to split user bindings from
    /// import bindings in the module pre-scan.
    fn collect_import_bound_names(stmts: &[Stmt], out: &mut HashSet<String>) {
        for stmt in stmts {
            match &stmt.kind {
                StmtKind::Import { names } => {
                    for a in names {
                        match &a.alias {
                            Some(al) => {
                                out.insert(al.clone());
                            }
                            None => {
                                // `import a.b.c` binds the HEAD `a`.
                                let head = a.name.split('.').next().unwrap_or(&a.name).to_string();
                                out.insert(head);
                            }
                        }
                    }
                }
                StmtKind::ImportFrom { names, .. } => {
                    for a in names {
                        if a.name != "*" {
                            out.insert(a.alias.clone().unwrap_or_else(|| a.name.clone()));
                        }
                    }
                }
                StmtKind::If {
                    body,
                    elif_clauses,
                    else_body,
                    ..
                } => {
                    Self::collect_import_bound_names(body, out);
                    for (_, b) in elif_clauses {
                        Self::collect_import_bound_names(b, out);
                    }
                    if let Some(e) = else_body {
                        Self::collect_import_bound_names(e, out);
                    }
                }
                StmtKind::While {
                    body, else_body, ..
                }
                | StmtKind::For {
                    body, else_body, ..
                } => {
                    Self::collect_import_bound_names(body, out);
                    if let Some(e) = else_body {
                        Self::collect_import_bound_names(e, out);
                    }
                }
                StmtKind::With { body, .. } => Self::collect_import_bound_names(body, out),
                StmtKind::Try {
                    body,
                    handlers,
                    else_body,
                    finally_body,
                } => {
                    Self::collect_import_bound_names(body, out);
                    for h in handlers {
                        Self::collect_import_bound_names(&h.body, out);
                    }
                    if let Some(e) = else_body {
                        Self::collect_import_bound_names(e, out);
                    }
                    if let Some(f) = finally_body {
                        Self::collect_import_bound_names(f, out);
                    }
                }
                StmtKind::Match { cases, .. } => {
                    for c in cases {
                        Self::collect_import_bound_names(&c.body, out);
                    }
                }
                // def/class bodies are separate scopes.
                _ => {}
            }
        }
    }

    /// Returns each hoist-eligible name with a `promoted` flag: `true` when
    /// the name's first binding is a depth-0 statement (`x = 5` before a
    /// loop that rebinds it) — see the #288 promotion pass at the bottom.
    /// Promoted module-scope names must keep the `export` their inline
    /// first assignment would have carried.
    ///
    /// `at_module`: true for the MODULE body, false for function/method
    /// bodies. The B2 import-rebind promotion below is module-only — a
    /// function-local import already emits assignably (`let X = __pyimp_X_n`
    /// at the import's position), and pre-hoisting it would replace the
    /// intended use-before-import TDZ fault (≈ UnboundLocalError) with a
    /// silent `undefined` read.
    fn collect_hoisted_names(body: &[Stmt], at_module: bool) -> Vec<(String, bool)> {
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
        let collect_import_bound = Self::collect_import_bound_names;
        struct WalkCtx {
            seen: Vec<(String, u32)>,
            /// #288: for-target names that are "reused" (reassigned or
            /// leaked-read elsewhere) — candidates for depth-0 promotion.
            promote: std::collections::HashSet<String>,
            /// Names bound by a def/class in this body: never promoted
            /// (`let f;` + `function f` is a JS SyntaxError).
            defclass: std::collections::HashSet<String>,
            /// #443: names also bound by an import in this body (see
            /// `collect_import_bound`).
            import_bound: std::collections::HashSet<String>,
            /// B2: true when walking the MODULE body (the import-rebind
            /// promotion + `del` recording are module-only).
            b2_module: bool,
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
                        for (i, item) in items.iter().enumerate() {
                            if let Some(v) = &item.optional_var {
                                // autotester control_structures: items after
                                // the first are emitted INSIDE the try{}
                                // nesting (one JS block deeper than the
                                // statement), so their targets must hoist to
                                // stay visible after the statement — Python
                                // scopes them to the enclosing function.
                                // #443: a first-item target that ALSO carries
                                // an import binding hoists too, so the import
                                // emits assignably and the `as` rebind lands
                                // on a mutable `let` instead of the immutable
                                // ESM import binding.
                                let mut names = Vec::new();
                                target_names(v, &mut names);
                                for n in names {
                                    let d = if i == 0 && !ctx.import_bound.contains(&n) {
                                        depth
                                    } else {
                                        depth + 1
                                    };
                                    record(&mut ctx.seen, &n, d);
                                }
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
                    // is recorded at the current depth but we don't recurse
                    // into the BODY. #441: a def's DEFAULTS, ANNOTATIONS
                    // (param + return), and DECORATORS are evaluated in THIS
                    // (enclosing) scope at def time (CPython def-time
                    // evaluation), so a walrus in any of them binds — and
                    // must hoist — HERE. Mirrors collect_bound_names' arm
                    // exactly: the two walkers must agree on what evaluates
                    // in the enclosing scope, or a walrus target ends up in
                    // the binding set with no `let` (a strict-mode
                    // ReferenceError on the emitted def-time assignment).
                    StmtKind::FuncDef {
                        name,
                        params,
                        decorator_list,
                        return_type,
                        ..
                    } => {
                        record(&mut ctx.seen, name, depth);
                        ctx.defclass.insert(name.clone());
                        for p in params {
                            if let Some(d) = &p.default {
                                walrus_in_expr(d, &mut ctx.seen);
                            }
                            if let Some(ann) = &p.annotation {
                                walrus_in_expr(ann, &mut ctx.seen);
                            }
                        }
                        for d in decorator_list {
                            walrus_in_expr(d, &mut ctx.seen);
                        }
                        if let Some(rt) = return_type {
                            walrus_in_expr(rt, &mut ctx.seen);
                        }
                    }
                    // A class's BASES and DECORATORS likewise evaluate in the
                    // enclosing scope at class-creation time.
                    StmtKind::ClassDef {
                        name,
                        bases,
                        decorator_list,
                        ..
                    } => {
                        record(&mut ctx.seen, name, depth);
                        ctx.defclass.insert(name.clone());
                        for b in bases {
                            walrus_in_expr(b, &mut ctx.seen);
                        }
                        for d in decorator_list {
                            walrus_in_expr(d, &mut ctx.seen);
                        }
                    }
                    // B2: `del X` REBINDS (unbinds) a bare name — for an
                    // import-bound name it must force the assignable-import
                    // path exactly like an assignment (the emitted
                    // `X = undefined` otherwise writes the immutable ESM
                    // binding). Recorded at depth ≥ 1 so it always hoists;
                    // non-import names keep their existing emission untouched.
                    // Module-only, like the whole B2 promotion (see below).
                    StmtKind::Del(exprs) if ctx.b2_module => {
                        for e in exprs {
                            if let ExprKind::Name(n) = &e.kind {
                                if ctx.import_bound.contains(n) {
                                    record(&mut ctx.seen, n, depth.max(1));
                                }
                            }
                        }
                    }
                    _ => {}
                }
            }
        }
        // B2: names `global`-declared inside any def (at any nesting) rebind
        // the MODULE binding when assigned there — an import-bound such name
        // must emit assignably. Over-approximating on the declaration alone
        // is safe: hoisting flips the import onto its Rebind path
        // (`import { X as __pyimp_X_n }` + `X = __pyimp_X_n;`), which is
        // semantically identical when no rebind ever runs.
        fn collect_global_declared(stmts: &[Stmt], in_def: bool, out: &mut HashSet<String>) {
            for stmt in stmts {
                match &stmt.kind {
                    StmtKind::Global(names) if in_def => {
                        for n in names {
                            out.insert(n.clone());
                        }
                    }
                    StmtKind::FuncDef { body, .. } => collect_global_declared(body, true, out),
                    StmtKind::ClassDef { body, .. } => collect_global_declared(body, in_def, out),
                    StmtKind::If {
                        body,
                        elif_clauses,
                        else_body,
                        ..
                    } => {
                        collect_global_declared(body, in_def, out);
                        for (_, b) in elif_clauses {
                            collect_global_declared(b, in_def, out);
                        }
                        if let Some(e) = else_body {
                            collect_global_declared(e, in_def, out);
                        }
                    }
                    StmtKind::While {
                        body, else_body, ..
                    }
                    | StmtKind::For {
                        body, else_body, ..
                    } => {
                        collect_global_declared(body, in_def, out);
                        if let Some(e) = else_body {
                            collect_global_declared(e, in_def, out);
                        }
                    }
                    StmtKind::With { body, .. } => collect_global_declared(body, in_def, out),
                    StmtKind::Try {
                        body,
                        handlers,
                        else_body,
                        finally_body,
                    } => {
                        collect_global_declared(body, in_def, out);
                        for h in handlers {
                            collect_global_declared(&h.body, in_def, out);
                        }
                        if let Some(e) = else_body {
                            collect_global_declared(e, in_def, out);
                        }
                        if let Some(f) = finally_body {
                            collect_global_declared(f, in_def, out);
                        }
                    }
                    StmtKind::Match { cases, .. } => {
                        for c in cases {
                            collect_global_declared(&c.body, in_def, out);
                        }
                    }
                    _ => {}
                }
            }
        }
        let mut import_bound = std::collections::HashSet::new();
        collect_import_bound(body, &mut import_bound);
        let mut ctx = WalkCtx {
            seen: Vec::new(),
            promote: std::collections::HashSet::new(),
            defclass: std::collections::HashSet::new(),
            import_bound,
            b2_module: at_module,
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
        // #443: a def/class name that ALSO carries an import binding in this
        // body. Python's `def X`/`class X` after `from m import X` is a plain
        // rebind, so the import must emit assignably: hoist an (exported, at
        // module scope) `let X;`, which flips the import onto its Rebind path
        // (`import { X as __pyimp_X_n }` + `X = __pyimp_X_n;`) and lets the
        // def/class emit in the assignment form (`X = function …` /
        // `X = class …`, `rebind_declared` in emit_func_def/emit_class_def)
        // instead of a redeclaration SyntaxError. The general defclass
        // exclusion above stays: `let f;` + `function f` only clashes when
        // the def still emits a DECLARATION, which `rebind_declared` rules
        // out exactly for these hoisted names.
        for (n, d) in ctx.seen.iter_mut() {
            if *d == 0 && ctx.defclass.contains(n) && ctx.import_bound.contains(n) {
                *d = 1;
                promoted.insert(n.clone());
            }
        }
        // B2 (CLASS rule): an import-bound name that is REBOUND by ANY
        // non-import binding form at ANY depth — depth-0 plain assignment,
        // tuple-unpack, aug-assign, for/with target, del, … — must hoist so
        // the import emits ASSIGNABLY (Rebind path: unique import + `X =
        // __pyimp_X_n;`) instead of an immutable ESM `import { X }` binding
        // that the rebind then hits ("Assignment to constant variable").
        // The pre-#B2 code promoted only depth>0 rebinds (the `d > 0` filter
        // below silently dropped module-level ones). ONE predicate, all
        // forms: rebound-anywhere ∧ import-bound ⇒ hoist. Def/class rebinds
        // are the pass above; `global`-declared rebinds are the pass below.
        // Module-only (see the doc comment on `at_module`).
        if at_module {
            for (n, d) in ctx.seen.iter_mut() {
                if *d == 0 && ctx.import_bound.contains(n) && !ctx.defclass.contains(n) {
                    *d = 1;
                    promoted.insert(n.clone());
                }
            }
            // B2: `global X` inside a def whose X is import-bound — the
            // function body assigns the MODULE binding directly, so it too
            // must be a `let`.
            let mut global_declared: HashSet<String> = HashSet::new();
            collect_global_declared(body, false, &mut global_declared);
            for n in &global_declared {
                if ctx.import_bound.contains(n) && !ctx.defclass.contains(n) {
                    if let Some((_, d)) = ctx.seen.iter_mut().find(|(sn, _)| sn == n) {
                        if *d == 0 {
                            *d = 1;
                        }
                    } else {
                        ctx.seen.push((n.clone(), 1));
                    }
                    promoted.insert(n.clone());
                }
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

    /// Emit the function-scope `let` hoists for a function-LIKE body (a `def`
    /// body OR a class-method body — both are full Python scopes). A local
    /// first-assigned inside a nested block (an if/else branch, a loop, a try)
    /// is function-scoped in Python, so it must be declared once at the top of
    /// the JS body rather than block-scoped `let` inside the branch. Without
    /// this, `if c: x = a else: x = b; return x` emits a block-scoped
    /// `let x = a` in the if-branch and a BARE `x = b` in the else — a
    /// `ReferenceError` under ESM strict mode (WB-5, previously only wired for
    /// `emit_func_def`, so METHOD bodies leaked the bug). Sentinel-init
    /// (`__UNBOUND`) covers unbound-local reads exactly as before (PBT-2/#288/
    /// #325). Must be called AFTER the scope is pushed and params declared, and
    /// (for a derived constructor) AFTER the `super(...)` call.
    fn emit_hoisted_local_decls(&mut self, body: &[Stmt]) {
        let hoisted_names = Self::collect_hoisted_names(body, false);
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
    }

    fn emit_func_def(
        &mut self,
        name: &str,
        params: &[Param],
        body: &[Stmt],
        decorator_list: &[Expr],
        return_type: Option<&Expr>,
        is_async: bool,
        rebind_declared: bool,
    ) {
        // WB-15: a `function` def rebinds JS `this`, so a live instance-method
        // receiver switches to the `__self` alias captured in the enclosing
        // method (any nesting depth — the alias is a closed-over const). BUT a
        // function that binds `self` as a real PARAMETER shadows the receiver:
        // inside it, `self` is that ordinary param (innermost binding wins).
        let prev_self_lowering = self.self_lowering;
        if params.iter().any(|p| p.name == "self") {
            self.self_lowering = SelfLowering::Ordinary;
        } else {
            self.cross_self_this_boundary();
        }
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
            // #441: CPython evaluates function ANNOTATIONS at def time —
            // after the defaults (dis order: defaults L→R, then annotations
            // L→R params-then-return) — so a walrus inside one assigns its
            // enclosing-scope target when the `def` executes. Annotations
            // are otherwise erased (type names routinely have no JS runtime
            // binding, so evaluating the WHOLE annotation would crash on
            // `x: SomeProtocol`); the observable walrus assignments are
            // extracted and emitted as def-site statements. Their targets
            // are hoisted `let`s (collect_hoisted_names' FuncDef arm scans
            // the same surfaces) and enclosing-scope bindings
            // (collect_bound_names' arm) — the same eval-timing family as
            // the genexp eager-iter fix (#463).
            let mut ann_walruses: Vec<&Expr> = Vec::new();
            for param in params {
                if let Some(ann) = &param.annotation {
                    Self::collect_named_exprs(ann, &mut ann_walruses);
                }
            }
            if let Some(rt) = return_type {
                Self::collect_named_exprs(rt, &mut ann_walruses);
            }
            for w in ann_walruses {
                self.write_indent();
                self.emit_expr(w);
                self.write(";\n");
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
        // #443 extension: `rebind_declared` — the def's name was ALREADY
        // declared in the current scope when the def executed (an import's
        // binding, a param, an earlier local). Python `def` is a plain rebind
        // there, so the assignment form applies at ANY indent (a nested
        // `def floor` after a function-local `from math import floor`
        // previously emitted `function floor` beside the import's `let floor`
        // — a redeclaration SyntaxError).
        let redefine = (self.indent == 0 && !self.module_decl_names.insert(name.to_string()))
            || rebind_declared;
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
        // autotester arguments/decorators: recover the keyword channel of a
        // `*args, <kw-only>, **kwargs` signature (emitted `...args`-last).
        self.emit_varargs_kw_prologue(params, name);
        // Issue #438: precompute this function's complete local binding set for
        // order-independent shadow resolution (params + body locals).
        let param_names: Vec<String> = params.iter().map(|p| p.name.clone()).collect();
        self.push_scope(Self::collect_local_bindings(body, &param_names));
        self.set_scope_globals(Self::collect_global_declared(body));
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
            // `cls` is an ordinary param in a plain function (class-decorator
            // idiom); only `self` stays implicit (its references lower to
            // `this`).
            if param.name != "self" {
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
        // instead of reading as undefined→None. Shared with class methods
        // (WB-5) via emit_hoisted_local_decls.
        self.emit_hoisted_local_decls(body);
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
        // autotester decorators: DECORATED functions now get metadata too —
        // the assignments run BEFORE the decorator application line, so they
        // attach to the ORIGINAL function object, which is exactly what the
        // decorator receives (the wrapper carries its own metadata). Without
        // this, `f(*args, **kwargs)` inside a decorator fell to the legacy
        // trailing-options convention and fed the kw dict into a positional
        // parameter of the decorated function.
        if !is_component && !is_handler {
            // B1: store the RAW Python param name. The binding is positional
            // by index (__pyKwArgs), and the call site emits raw keyword names
            // (emit_kwargs_value), so __pyparams__ must match those raw names —
            // NOT the sanitized JS parameter declaration. A param named like a
            // JS reserved word (`default`) is declared `default$` in the
            // signature but must appear as "default" here or every keyword call
            // misses (TypeError: unexpected keyword argument). Mirrors the
            // dataclass path, which already stores raw field names.
            //
            // autotester arguments/decorators: varargs signatures now carry
            // metadata too — __pyparams__ lists only the names BEFORE `*args`
            // (positionally bindable), __pyva__ marks the variadic convention
            // (keywords travel as the marked trailing carrier — __pyMarkKw),
            // and __pykw__ is set when a keyword channel exists (**kwargs or
            // keyword-only params).
            let star = params.iter().position(|p| p.is_args);
            let names: Vec<String> = params
                .iter()
                .enumerate()
                .filter(|(i, p)| {
                    // `self` stays: in a PLAIN function it is a real,
                    // keyword-bindable first parameter (decorator wrappers).
                    !p.is_kwargs
                        && !p.is_args
                        && p.name != "cls"
                        && star.is_none_or(|s| *i < s)
                })
                .map(|(_, p)| format!("\"{}\"", p.name))
                .collect();
            let has_kw = params.iter().any(|p| p.is_kwargs)
                || (star.is_some() && Self::varargs_kw_split(params).is_some());
            if !names.is_empty() || has_kw || star.is_some() {
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
                if star.is_some() {
                    self.write_indent();
                    self.write(&format!("{}.__pyva__ = true;\n", js_name));
                }
            }
            // autotester docstrings: attach the function docstring.
            if let Some(doc) = Self::body_docstring(body) {
                self.write_indent();
                self.write(&format!(
                    "{}.__doc__ = {};\n",
                    js_name,
                    js_string_literal(doc)
                ));
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
                // __pyCall: a decorator may be a callable INSTANCE
                // (`@Repeater(2)` — a class whose __call__ wraps). One-time
                // application, so the wrapper costs nothing on hot paths.
                self.need_runtime("__pyCall");
                self.write_indent();
                self.write(&format!("{} = __pyCall(", js_name));
                self.emit_expr(decorator);
                self.write(&format!(", [{}]);\n", js_name));
            }
        }

        // B-029 follow-up (C): emit `export default { fetch: <fn> };`
        // for top-level @handler functions.  Only at module level
        // (self.indent == 0 after the closing `}`); nested @handler
        // is silently ignored (no sensible meaning).
        if is_handler && self.indent == 0 {
            self.write(&format!("export default {{ fetch: {} }};\n", js_name));
        }

        self.self_lowering = prev_self_lowering;
    }

    /// autotester arguments/decorators: does this signature carry a keyword
    /// channel AFTER a `*args` rest param — keyword-only params and/or
    /// `**kwargs`? JS cannot declare parameters after a rest param, so such
    /// defs emit `...args` LAST in the JS signature and recover the keyword
    /// channel in a body prologue (see emit_varargs_kw_prologue): call sites
    /// with keywords append a Symbol-marked carrier object (__pyMarkKw in
    /// __pyKwArgs), the prologue pops it (__pyTakeKw), extracts keyword-only
    /// params (__pyKwPop — CPython missing/unexpected TypeError shapes), and
    /// what remains is the `**kwargs` dict.
    fn varargs_kw_split(params: &[Param]) -> Option<(usize, Vec<&Param>, Option<&Param>)> {
        let star = params.iter().position(|p| p.is_args)?;
        let kwonly: Vec<&Param> = params[star + 1..]
            .iter()
            .filter(|p| !p.is_kwargs)
            .collect();
        let kwargs = params.iter().find(|p| p.is_kwargs);
        if kwonly.is_empty() && kwargs.is_none() {
            return None;
        }
        Some((star, kwonly, kwargs))
    }

    /// Emit the keyword-channel prologue for a varargs+keyword signature,
    /// plus the `*args`-is-a-tuple marker for ANY varargs signature.
    /// Must be called immediately after the opening `{` of the function or
    /// method body (before any user statements). No-op for other signatures.
    fn emit_varargs_kw_prologue(&mut self, params: &[Param], fname: &str) {
        self.emit_varargs_kw_channel(params, fname);
        // S2: `*args` collects into a TUPLE in Python (type/isinstance/repr).
        // Mark the fresh JS rest array in place — after the kw channel above
        // has popped the keyword carrier off its tail.
        if let Some(star) = params.iter().position(|p| p.is_args) {
            self.need_runtime("__pyMarkTuple");
            self.write_indent();
            self.write(&format!(
                "__pyMarkTuple({});\n",
                Self::sanitize_ident(&params[star].name)
            ));
        }
    }

    fn emit_varargs_kw_channel(&mut self, params: &[Param], fname: &str) {
        let Some((star, kwonly, kwargs)) = Self::varargs_kw_split(params) else {
            return;
        };
        let args_js = Self::sanitize_ident(&params[star].name).into_owned();
        let kw_var = match kwargs {
            Some(k) => Self::sanitize_ident(&k.name).into_owned(),
            None => {
                let n = self.default_hoist_counter;
                self.default_hoist_counter += 1;
                format!("__kwonly{}", n)
            }
        };
        self.need_runtime("__pyTakeKw");
        self.write_indent();
        self.write(&format!("const {} = __pyTakeKw({});\n", kw_var, args_js));
        if kwargs.is_none() {
            // BEFORE the pops: CPython reports an unexpected keyword ahead
            // of a missing keyword-only one, so validate the key set first
            // (the kw-only names are the allowed remainder).
            self.need_runtime("__pyNoExtraKw");
            let allowed: Vec<String> = kwonly
                .iter()
                .map(|p| format!("\"{}\"", p.name))
                .collect();
            self.write_indent();
            self.write(&format!(
                "__pyNoExtraKw({}, \"{}\", [{}]);\n",
                kw_var,
                fname,
                allowed.join(", ")
            ));
        }
        for p in &kwonly {
            self.need_runtime("__pyKwPop");
            self.write_indent();
            self.write(&format!(
                "let {} = __pyKwPop({}, \"{}\", \"{}\"",
                Self::sanitize_ident(&p.name),
                kw_var,
                p.name,
                fname
            ));
            if let Some(d) = &p.default {
                self.write(", ");
                self.emit_expr(d);
            }
            self.write(");\n");
        }
    }

    fn emit_params(&mut self, params: &[Param]) {
        self.emit_params_ctx(params, false)
    }

    /// `drop_first`: drop the FIRST param — the method receiver bound to JS
    /// `this` (an instance method's `self`, or a `@classmethod`'s first param).
    /// WB-15: this is now driven by the caller's single receiver decision, NOT
    /// a name test — a `@staticmethod`'s `self`/`cls` param is an ordinary
    /// argument and is KEPT (dropping it emitted a signature whose first
    /// argument silently shifted / left the body's `self` unbound). In a PLAIN
    /// function (or lambda) `drop_first` is false, so a `self`/`cls` param is an
    /// ordinary parameter — the decorator idioms `def deco(cls): ...` and
    /// `def wrapper(self, name): ...`.
    fn emit_params_ctx(&mut self, params: &[Param], drop_first: bool) {
        // Varargs + keyword channel: the rest param is emitted LAST and the
        // keyword-only/**kwargs params move to the body prologue.
        let stop_at_star = Self::varargs_kw_split(params).is_some();
        let star_idx = params.iter().position(|p| p.is_args);
        let mut first = true;
        for (i, param) in params.iter().enumerate() {
            if drop_first && i == 0 {
                continue;
            }
            if stop_at_star && star_idx.is_some_and(|s| i > s) {
                continue; // keyword-only / **kwargs → prologue
            }
            if !first {
                self.write(", ");
            }
            first = false;
            if param.is_args {
                self.write("...");
            }
            self.write(&Self::sanitize_ident(&param.name));
            if param.is_args && stop_at_star {
                break; // nothing may follow a JS rest param
            }
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
        rebind_declared: bool,
    ) {
        // autotester reprtest: `class X(object):` — an explicit `object` base
        // is Python's default and means "no base at all", but it was emitted
        // verbatim as `extends object` → ReferenceError at load. Strip it
        // HERE, at the single entry point, so every downstream consumer (the
        // `extends` slot, the __pyClass mixin list, the dataclass path) sees
        // the canonical no-base form. A user class actually NAMED `object`
        // shadows the builtin and is kept (known_classes distinguishes it).
        let is_object_base = |b: &Expr| {
            matches!(&b.kind, ExprKind::Name(n)
                if n == "object" && !self.known_classes.contains(n))
        };
        let filtered_bases: Vec<Expr>;
        let bases: &[Expr] = if bases.iter().any(is_object_base) {
            filtered_bases = bases
                .iter()
                .filter(|b| !is_object_base(b))
                .cloned()
                .collect();
            &filtered_bases
        } else {
            bases
        };
        // Check for @dataclass or @dataclass(frozen=True)
        let mut dc_opts = DataclassOptions::default();
        let is_dataclass = decorator_list.iter().any(|d| {
            let (is_dc, opts) = parse_dataclass_decorator(d);
            if is_dc {
                dc_opts = opts;
            }
            is_dc
        });

        // WB-12: `@js_class` opt-in — emit a plain JS class (NO `extends
        // PyObject`, NO cooperative-MRO `__pyClass` install) so libraries that
        // reject any class with a superclass work (MobX `makeAutoObservable`,
        // and any lib inspecting the prototype chain for "has a superclass").
        // The DEFAULT (`extends PyObject`) is UNCHANGED — it carries Python
        // object semantics (repr / isinstance / cooperative super); `@js_class`
        // deliberately trades those away for foreign-lib interop. `__init__`
        // then lowers to a plain JS `constructor` (via `pyobject_model == false`)
        // and, with no base, no `super()` is synthesized.
        let is_js_class = decorator_list
            .iter()
            .any(|d| matches!(&d.kind, ExprKind::Name(n) if n == "js_class"));

        // #350: a module-level class redefinition (a name already declared at
        // module scope) is Python last-wins — emit it as a `name = class …`
        // assignment so JS doesn't reject a duplicate `class name` declaration.
        // #443: `rebind_declared` extends this to a class whose name was
        // already declared when the class executed (an import's binding, a
        // param, an earlier local) — a plain Python rebind, at any indent.
        let redefine = (self.indent == 0 && !self.module_decl_names.insert(name.to_string()))
            || rebind_declared;
        // #443: record that this class DEFINITION has executed — an import
        // that rebinds the name from here on drops it from `known_classes`
        // (see plan_import_binding), so post-rebind calls stop `new`-lowering.
        self.emitted_class_names.insert(name.to_string());
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
        // Does ANY base extend a builtin JS-level class (Exception &
        // friends)? Those keep the native `extends` + native-`super()` path;
        // the cooperative-MRO machinery is only wired for pure-PythScribe
        // class hierarchies.
        //
        // r7 BLOCKING fix: scan ALL bases, not just bases[0]. In CPython,
        // `class E(Mixin, ValueError)` IS an exception (raisable, str(e) =
        // args[0], caught by `except ValueError`) — the mixin-first ordering
        // is the COMMON multiple-inheritance idiom. Treating it as a plain
        // PyObject-model class made the raised instance a non-Error with no
        // message and no ValueError linkage. The exception base now goes on
        // the JS `extends` chain (instances are real Errors → raise/except/
        // str/args all work), and the remaining bases mix in via __pyClass
        // exactly like every other multi-base class — C3 gives the mixin
        // precedence over the chain, matching CPython's MRO.
        let exception_base_idx = bases
            .iter()
            .position(|b| matches!(&b.kind, ExprKind::Name(n) if is_builtin_exception(n)));
        let first_base_is_exception = exception_base_idx.is_some();
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
        let pyobject_model = !is_dataclass
            && !first_base_is_exception
            && !first_base_is_external_native
            && !is_js_class;
        if !bases.is_empty() {
            // Auto-import builtin exception bases (Exception, ValueError, …)
            // when subclassed — mirrors the `raise X(...)` auto-import. Without
            // this, `class Foo(Exception)` emits `extends Exception` with no
            // import → ReferenceError at load.
            //
            // r7 BLOCKING fix: scan ALL bases, not just bases[0]. `__pyClass`
            // below emits EVERY base into its mixin list, so an exception as a
            // NON-FIRST base — `class E(Mixin, ValueError)`, the common
            // multiple-inheritance pattern — referenced ValueError without
            // ever importing it → undefined reference at runtime.
            for base in bases {
                if let ExprKind::Name(base_name) = &base.kind {
                    if is_builtin_exception(base_name) {
                        self.need_runtime(base_name);
                    }
                }
            }
            // Only ONE base goes on the JS prototype chain (single
            // `extends`); methods from the remaining bases are mixed in by
            // `__pyClass` below, in C3-MRO order. For regular hierarchies the
            // chain bottoms out at `PyObject` via the root class, enabling
            // cooperative MRO `__init__` dispatch. r7: when a builtin
            // exception appears among the bases it takes the chain slot even
            // when non-first — the instance must BE an Error for raise/
            // except/str to work; the C3 flattening in __pyClass still gives
            // the preceding mixins CPython's precedence for method lookup.
            self.write(" extends ");
            self.emit_expr(&bases[exception_base_idx.unwrap_or(0)]);
        } else if pyobject_model {
            // No explicit base → extend the runtime `PyObject` so `new C(...)`
            // routes through its cooperative `__init__` dispatcher.
            self.need_runtime("PyObject");
            self.write(" extends PyObject");
        }
        self.write(" {\n");
        self.indent += 1;
        // Issue #438: a Python class scope does NOT enclose its methods' scopes
        // (a method sees module/builtin names, never class-level names except
        // via self/cls). Push an EMPTY binding set so class-level names never
        // pollute method shadow resolution.
        self.push_scope(HashSet::new());
        self.class_stack.push(ClassCtx {
            name: name.to_string(),
            pyobject_model,
            has_bases: !bases.is_empty(),
        });

        if is_dataclass {
            // Dataclass INHERITANCE: prepend every base dataclass's field
            // statements (already flattened) so this constructor / __repr__
            // / __eq__ covers inherited fields in CPython's order; register
            // this class's flattened set for its own subclasses.
            let mut inherited: Vec<Stmt> = Vec::new();
            for b in bases {
                if let ExprKind::Name(bn) = &b.kind {
                    if let Some(fs) = self.dataclass_field_stmts.get(bn) {
                        inherited.extend(fs.iter().cloned());
                    }
                }
            }
            let mut flat = inherited.clone();
            flat.extend(
                body.iter()
                    .filter(|st| matches!(st.kind, StmtKind::AnnAssign { .. }))
                    .cloned(),
            );
            self.dataclass_field_stmts.insert(name.to_string(), flat);
            // B6: the derived constructor must call `super(...)` with the
            // JS-extended base's OWN constructor contract — its init fields,
            // in order (`extends` targets the FIRST base). A bare `super()`
            // ran the base's field validators on `undefined` and threw for
            // any base with required fields.
            let super_fields: Vec<String> = bases
                .first()
                .and_then(|b| match &b.kind {
                    ExprKind::Name(bn) => self.dataclass_field_stmts.get(bn),
                    _ => None,
                })
                .map(|fs| {
                    let mut v: Vec<String> = Vec::new();
                    for f in collect_dataclass_fields(fs) {
                        if !f.property_default && !v.contains(&f.name.to_string()) {
                            v.push(f.name.to_string());
                        }
                    }
                    v
                })
                .unwrap_or_default();
            self.emit_dataclass_body(
                name,
                body,
                &inherited,
                !bases.is_empty(),
                &super_fields,
                &dc_opts,
            );
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
        // exception subclasses keep their native paths — EXCEPT (r7) a
        // multi-base exception class (`class E(Mixin, ValueError)`), which
        // still needs __pyClass so the mixin's methods land on the prototype
        // (with C3 precedence) and `__mro__` exists for isinstance(e, Mixin).
        // Construction stays native (the exception chain's constructor);
        // __pyC3 tolerates the runtime exception base having no __mro__.
        if pyobject_model || (first_base_is_exception && bases.len() > 1) {
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
        // autotester docstrings: class + method docstrings.
        if let Some(doc) = Self::body_docstring(body) {
            self.write_indent();
            self.write(&format!(
                "{}.__doc__ = {};\n",
                js_name,
                js_string_literal(doc)
            ));
        }
        for stmt in body {
            if let StmtKind::FuncDef {
                name: m_name,
                body: m_body,
                ..
            } = &stmt.kind
            {
                if let Some(doc) = Self::body_docstring(m_body) {
                    self.write_indent();
                    self.write(&format!(
                        "if ({cls}.prototype.{m}) {cls}.prototype.{m}.__doc__ = {d};\n",
                        cls = js_name,
                        m = Self::sanitize_ident(m_name),
                        d = js_string_literal(doc)
                    ));
                }
            }
        }
        // 1. Class attributes: `Cls.attr = value` + a live prototype
        //    accessor so instances read through and instance assignment
        //    shadows (Python attribute lookup).
        //    autotester properties: while emitting these VALUES, sibling
        //    method names are class-local (`x = property(getX, setX)`) and
        //    resolve to Cls.prototype.<name>.
        let method_names: HashSet<String> = body
            .iter()
            .filter_map(|s| match &s.kind {
                StmtKind::FuncDef { name, .. } => Some(name.clone()),
                _ => None,
            })
            .collect();
        self.class_attr_subst = Some((js_name.to_string(), method_names));
        for stmt in body {
            let (target, value, ann) = match &stmt.kind {
                StmtKind::Assign { targets, value } if targets.len() == 1 => {
                    (&targets[0], value, None)
                }
                StmtKind::AnnAssign {
                    target,
                    value: Some(v),
                    annotation,
                } => (target, v, Some(annotation)),
                _ => continue,
            };
            // Dataclass fields are constructor params — EXCEPT ClassVar
            // pseudo-fields, plain (un-annotated) assignments, and
            // property(...) defaults, which CPython keeps as class
            // attributes (autotester data_classes: m = 101010,
            // v = property(getV, setV), w: int = property(getW, setW)).
            if is_dataclass
                && ann.is_some()
                && !ann.is_some_and(is_classvar_annotation)
                && !is_property_call(value)
            {
                continue;
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
                continue;
            }
            // autotester classes/properties: class-body TUPLE-target
            // assignment (`p, q = 456, 789` / `x, y = property(...), …`)
            // previously fell through to emit_stmt inside the class body →
            // invalid bare `let p;`. Install each name as a class attribute
            // here instead. Matching-arity literal RHS assigns element-wise;
            // any other RHS is materialized once and indexed.
            if let ExprKind::Tuple(elts) | ExprKind::List(elts) = &target.kind {
                if !elts
                    .iter()
                    .all(|e| matches!(&e.kind, ExprKind::Name(_)))
                {
                    continue; // non-Name elements in a class body: unsupported
                }
                self.need_runtime("__pyClassAttr");
                let literal_vals: Option<&Vec<Expr>> = match &value.kind {
                    ExprKind::Tuple(vals) | ExprKind::List(vals)
                        if vals.len() == elts.len()
                            && !vals
                                .iter()
                                .any(|v| matches!(v.kind, ExprKind::Starred(_))) =>
                    {
                        Some(vals)
                    }
                    _ => None,
                };
                let tmp = if literal_vals.is_none() {
                    let t = format!("__clsattr{}", self.default_hoist_counter);
                    self.default_hoist_counter += 1;
                    self.write_indent();
                    self.write(&format!("const {} = [...(", t));
                    self.emit_expr(value);
                    self.write(")];\n");
                    Some(t)
                } else {
                    None
                };
                for (i, elt) in elts.iter().enumerate() {
                    let ExprKind::Name(attr_name) = &elt.kind else {
                        unreachable!()
                    };
                    self.write_indent();
                    self.write(&format!(
                        "__pyClassAttr({}, \"{}\", ",
                        js_name,
                        Self::sanitize_ident(attr_name)
                    ));
                    match (&literal_vals, &tmp) {
                        (Some(vals), _) => self.emit_expr(&vals[i]),
                        (None, Some(t)) => self.write(&format!("{}[{}]", t, i)),
                        _ => unreachable!(),
                    }
                    self.write(");\n");
                }
            }
        }
        self.class_attr_subst = None;
        // 1b. autotester classes: NESTED classes install as class attributes
        //     (`Outer.Inner`). Emitted inside a bare block so the local
        //     `class Inner` binding neither leaks to module scope nor
        //     collides with a sibling class's nested name; __pyClassCall's
        //     class-detection constructs it with `new` at call sites.
        for stmt in body {
            if let StmtKind::ClassDef {
                name: nested_name,
                bases: nested_bases,
                body: nested_body,
                decorator_list: nested_decs,
            } = &stmt.kind
            {
                self.writeln("{");
                self.indent += 1;
                // Fresh block scope — the nested name cannot collide here.
                self.emit_class_def(nested_name, nested_bases, nested_body, nested_decs, false);
                self.need_runtime("__pyClassAttr");
                self.write_indent();
                self.write(&format!(
                    "__pyClassAttr({}, \"{}\", {});\n",
                    js_name,
                    nested_name,
                    Self::sanitize_ident(nested_name)
                ));
                self.indent -= 1;
                self.writeln("}");
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
                if !is_accessor {
                    // B1: RAW param name (see function-site rationale). The
                    // call site emits raw keyword names; __pyparams__ must
                    // match them, not the sanitized JS signature form.
                    // autotester arguments: varargs methods carry metadata
                    // too (names before `*args` + __pyva__ marker) — see the
                    // function-site rationale.
                    let star = m_params.iter().position(|p| p.is_args);
                    let names: Vec<String> = m_params
                        .iter()
                        .enumerate()
                        .filter(|(i, p)| {
                            !p.is_kwargs
                                && !p.is_args
                                && p.name != "self"
                                && p.name != "cls"
                                && star.is_none_or(|s| *i < s)
                        })
                        .map(|(_, p)| format!("\"{}\"", p.name))
                        .collect();
                    let has_kw = m_params.iter().any(|p| p.is_kwargs)
                        || (star.is_some() && Self::varargs_kw_split(m_params).is_some());
                    if !names.is_empty() || has_kw || star.is_some() {
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
                        if star.is_some() {
                            self.write_indent();
                            self.write(&format!("{}.__pyva__ = true;\n", target));
                        }
                    }
                }
                // autotester method_and_class_decorators: USER method
                // decorators were silently DROPPED (the worst failure mode —
                // the undecorated method ran). Apply them here, bottom-up,
                // after the metadata assignments (the decorator receives the
                // plain function; the wrapper carries its own metadata).
                // Recognized structural decorators (static/class/property/
                // setter/validator/check) keep their dedicated lowerings.
                let is_structural = |d: &Expr| {
                    matches!(&d.kind, ExprKind::Name(n)
                        if n == "staticmethod" || n == "classmethod"
                            || n == "property" || n == "check")
                        || matches!(&d.kind, ExprKind::Attribute { attr, .. }
                            if attr == "setter" || attr == "getter" || attr == "deleter")
                        || matches!(&d.kind, ExprKind::Call { func, .. }
                            if matches!(&func.kind, ExprKind::Name(n) if n == "validator"))
                };
                for dec in m_decs.iter().rev() {
                    if is_structural(dec) {
                        continue;
                    }
                    let target = if is_static_m || is_class_m {
                        format!("{}.{}", js_name, m_js)
                    } else {
                        format!("{}.prototype.{}", js_name, m_js)
                    };
                    self.write_indent();
                    if is_class_m {
                        // @classmethod decoration: the decorator's wrapper
                        // signature is (cls, ...) — thread the class the
                        // way __pyDecorateMethod threads self.
                        self.need_runtime("__pyDecorateClassMethod");
                        self.write(&format!("{} = __pyDecorateClassMethod(", target));
                        self.emit_expr(dec);
                        self.write(&format!(", {}, {});\n", target, js_name));
                    } else if is_static_m {
                        // No self to thread — plain application (via
                        // __pyCall: the decorator may be a callable instance).
                        self.need_runtime("__pyCall");
                        self.write(&format!("{} = __pyCall(", target));
                        self.emit_expr(dec);
                        self.write(&format!(", [{}]);\n", target));
                    } else {
                        self.need_runtime("__pyDecorateMethod");
                        self.write(&format!("{} = __pyDecorateMethod(", target));
                        self.emit_expr(dec);
                        self.write(&format!(", {});\n", target));
                    }
                }
            }
        }
        if is_dataclass {
            // Dataclass constructors bind keywords by field order — the
            // FLATTENED order (inherited fields first), matching the
            // constructor signature emitted by emit_dataclass_body.
            let flat_stmts = self
                .dataclass_field_stmts
                .get(name)
                .cloned()
                .unwrap_or_default();
            let mut fields: Vec<DataclassField> = Vec::new();
            for f in collect_dataclass_fields(&flat_stmts) {
                if let Some(existing) = fields.iter_mut().find(|e| e.name == f.name) {
                    *existing = f;
                } else {
                    fields.push(f);
                }
            }
            fields.retain(|f| !f.property_default); // descriptors: not params
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

        // Apply class decorators (skip dataclass/component/js_class and their
        // call forms — those are compile-time markers consumed by codegen, not
        // runtime wrappers). `@js_class` (WB-12) already reshaped the emitted
        // class above; applying it as `Store = __pyCall(js_class, [Store])`
        // would reference an undefined runtime symbol.
        for decorator in decorator_list.iter().rev() {
            let (is_dc, _) = parse_dataclass_decorator(decorator);
            let skip = is_dc
                || matches!(&decorator.kind, ExprKind::Name(n) if n == "component" || n == "js_class");
            if !skip {
                self.need_runtime("__pyCall");
                self.write_indent();
                self.write(&format!("{} = __pyCall(", Self::sanitize_ident(name)));
                self.emit_expr(decorator);
                self.write(&format!(", [{}]);\n", Self::sanitize_ident(name)));
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
                // autotester classes: class-body tuple-target assignment is
                // installed post-class via __pyClassAttr (all-Name patterns)
                // — see emit_class_def. Skip it inside the body.
                StmtKind::Assign { targets, .. }
                    if targets.len() == 1
                        && matches!(&targets[0].kind,
                            ExprKind::Tuple(elts) | ExprKind::List(elts)
                                if elts.iter().all(|e| matches!(&e.kind, ExprKind::Name(_)))) => {}
                StmtKind::AnnAssign {
                    target,
                    value: Some(_),
                    ..
                } if matches!(&target.kind, ExprKind::Name(_)) => {}
                StmtKind::Assign { .. } | StmtKind::AnnAssign { .. } => {
                    self.emit_stmt(stmt);
                }
                // autotester classes: a NESTED class miscompiled to a raw
                // `class Inner {…}` inside the class body (invalid JS). It is
                // installed post-class as a class attribute — see
                // emit_class_def.
                StmtKind::ClassDef { .. } => {}
                // autotester arguments/method_and_class_decorators: Python
                // allows bare EXPRESSION statements in a class body (executed
                // at class creation — docstrings, `__pragma__(...)` shims). A
                // JS class body cannot hold statements, and their effects
                // beyond name binding have no compiled representation — drop
                // them instead of emitting invalid JS.
                StmtKind::Expr(_) => {}
                StmtKind::Pass => {}
                _ => self.emit_stmt(stmt),
            }
        }
    }

    fn emit_dataclass_body<'b>(
        &mut self,
        class_name: &str,
        body: &'b [Stmt],
        inherited: &'b [Stmt],
        has_bases: bool,
        super_fields: &[String],
        opts: &DataclassOptions,
    ) {
        // Base fields first (CPython inheritance order); a redeclared field
        // keeps its ORIGINAL position with the derived default (CPython).
        // Dedupe across the whole chain — the registry stores raw statement
        // concatenations, so a grandparent field can appear twice.
        let mut fields: Vec<DataclassField> = Vec::new();
        for f in collect_dataclass_fields(inherited)
            .into_iter()
            .chain(collect_dataclass_fields(body))
        {
            if let Some(existing) = fields.iter_mut().find(|e| e.name == f.name) {
                *existing = f;
            } else {
                fields.push(f);
            }
        }

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
        // property-default fields are descriptors, not __init__ params.
        let init_fields: Vec<&DataclassField> =
            fields.iter().filter(|f| !f.property_default).collect();
        for (i, field) in init_fields.iter().enumerate() {
            if i > 0 {
                self.write(", ");
            }
            // Fix C: the constructor param is a LOCAL binding — a reserved-word
            // field (`default`, `new`, `in`) must be sanitized so it isn't
            // emitted as `constructor(default)` (SyntaxError). The instance
            // PROPERTY (`this.default`) keeps the raw name (member access allows
            // reserved words); only the local var reference is renamed.
            self.write(&Self::sanitize_ident(&field.name));
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
        // B6: this runs BEFORE the `super(...)` call below (legal — it never
        // touches `this`) so the kwargs-object form feeds real field values
        // into the base constructor too.
        if !init_fields.is_empty() {
            // Fix C: the first param is referenced as a local — sanitize it.
            let f0 = Self::sanitize_ident(&init_fields[0].name).into_owned();
            self.write_indent();
            self.write("if (arguments.length === 1 && ");
            self.write(&f0);
            self.write(" !== null && typeof ");
            self.write(&f0);
            self.write(" === \"object\" && !Array.isArray(");
            self.write(&f0);
            self.write(")) {\n");
            self.indent += 1;
            self.write_indent();
            self.write("({");
            for (i, field) in init_fields.iter().enumerate() {
                if i > 0 {
                    self.write(", ");
                }
                // Fix C: destructure the raw property key into the sanitized
                // local binding (`{default: default$}`) for reserved-word fields.
                let jf = Self::sanitize_ident(&field.name);
                if jf.as_ref() != field.name {
                    self.write(&format!("{}: {}", field.name, jf));
                } else {
                    self.write(&field.name);
                }
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
            self.write(&f0);
            self.write(");\n");
            self.indent -= 1;
            self.write_indent();
            self.write("}\n");
        }

        // B6 root fix — a derived JS class MUST call super() before touching
        // `this`, and the call must satisfy the BASE constructor's contract:
        // pass the base's init fields (this ctor's params include them all,
        // base-first). A bare `super();` ran the base's field validators on
        // `undefined` — every construction of a derived dataclass with a
        // required base field threw. A base without a registered dataclass
        // contract (JS/interop class) keeps the bare call. The base re-runs
        // its own validation/assignment for its fields; the derived ctor
        // re-assigns them below with identical values (idempotent).
        if has_bases {
            self.write_indent();
            self.write("super(");
            for (i, f) in super_fields.iter().enumerate() {
                if i > 0 {
                    self.write(", ");
                }
                self.write(&Self::sanitize_ident(f));
            }
            self.write(");\n");
        }

        // 1. Coercion (if coerce=True). Fix C: these helpers read/mutate the
        // field as a LOCAL var (the ctor param), so a reserved-word field is
        // addressed by its sanitized name here.
        if opts.coerce {
            for field in &init_fields {
                if let Some(ann) = field.annotation {
                    self.emit_coercion(
                        class_name,
                        &field.name,
                        Self::sanitize_ident(&field.name).as_ref(),
                        ann,
                    );
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
        for field in &init_fields {
            if let Some(ann) = field.annotation {
                self.emit_type_validation(class_name, &field.name, ann);
            }
        }

        // 4. String transforms (trim, to_lower, to_upper) — before constraint
        // validation. Fix C: addressed by the sanitized local name.
        for field in &init_fields {
            self.emit_transform_constraints(
                Self::sanitize_ident(&field.name).as_ref(),
                &field.constraints,
            );
        }

        // 5. Constraint validation per field. Fix C: sanitized JS var for the
        // conditions; review 3: RAW field name for the message labels.
        for field in &init_fields {
            self.emit_constraint_validation(
                class_name,
                Self::sanitize_ident(&field.name).as_ref(),
                &field.name,
                &field.constraints,
            );
        }

        // 6. Throw collected errors (if collect_errors=True)
        if opts.collect_errors {
            self.collecting_errors = false;
            self.write_indent();
            self.write("if (__errors.length > 0) throw new TypeError(__errors.join(\"; \"));\n");
        }

        // 7. Field assignments
        for field in &init_fields {
            // Fix C: LHS is the instance property (raw name is a valid member);
            // RHS is the local ctor param, which was sanitized for reserved
            // words — `this.default = default$`.
            self.write_indent();
            self.write(&format!(
                "this.{prop} = {local};\n",
                prop = field.name,
                local = Self::sanitize_ident(&field.name)
            ));
        }

        // 8. @validator calls
        for (field_name, method_name) in &validators {
            // SECURITY (#3): `field_name` is the @validator("...") string arg —
            // source-derived, arbitrary. Emitting `this.<field_name>` splices it
            // raw into member-access syntax, injecting JS. Use safe COMPUTED
            // access `this[<encoded>]` so it can only ever be a property name.
            // `method_name` is a FuncDef identifier (already legal JS), safe.
            self.write_indent();
            let sel = js_string_literal(field_name);
            self.write(&format!(
                "this[{sel}] = this.{m}(this[{sel}]);\n",
                sel = sel,
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

        // @dataclass(order=True): CPython's generated ordering compares the
        // field tuples.
        if opts.order {
            self.write_indent();
            self.write("_astuple() {\n");
            self.indent += 1;
            self.write_indent();
            self.write("return [");
            for (i, field) in fields.iter().enumerate() {
                if i > 0 {
                    self.write(", ");
                }
                self.write(&format!("this.{}", field.name));
            }
            self.write("];\n");
            self.indent -= 1;
            self.writeln("}");
            for (dunder, helper) in [
                ("__lt__", "pyLt"),
                ("__le__", "pyLe"),
                ("__gt__", "pyGt"),
                ("__ge__", "pyGe"),
            ] {
                self.need_runtime(helper);
                self.write_indent();
                self.write(&format!("{}(other) {{\n", dunder));
                self.indent += 1;
                self.write_indent();
                self.write(&format!(
                    "return {}(this._astuple(), other._astuple());\n",
                    helper
                ));
                self.indent -= 1;
                self.writeln("}");
            }
        }

        // Emit toDict()
        self.write_indent();
        self.write("toDict() {\n");
        self.indent += 1;
        self.write_indent();
        self.write("return { ");
        for (i, field) in init_fields.iter().enumerate() {
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
        for (i, field) in init_fields.iter().enumerate() {
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
        // Fix C: the value is read from the sanitized local (the ctor param),
        // while messages keep the raw field name.
        let var = Self::sanitize_ident(field_name).into_owned();
        self.emit_type_check(class_name, field_name, &var, &tc);
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
                    &format!(
                        "typeof {v} !== \"number\" && !({v} != null && {v}.__pyfloat__ === true)",
                        v = var
                    ),
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

    /// Emit constraint validation checks for a dataclass field. `field_name` is
    /// the SANITIZED JS variable (the ctor param); `label` is the RAW Python
    /// field name used only in user-visible error-message labels (review 3 —
    /// so a reserved-word field reads `C.default: ...`, not `C.default$: ...`).
    fn emit_constraint_validation(
        &mut self,
        class_name: &str,
        field_name: &str,
        label: &str,
        constraints: &FieldConstraints,
    ) {
        if let Some(gt) = constraints.gt {
            self.emit_validation_error(
                &format!("{f} <= {v}", f = field_name, v = format_f64(gt)),
                &format!(
                    "\"{c}.{label}: must be > {v}, got \" + {f}",
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
                    "\"{c}.{label}: must be >= {v}, got \" + {f}",
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
                    "\"{c}.{label}: must be < {v}, got \" + {f}",
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
                    "\"{c}.{label}: must be <= {v}, got \" + {f}",
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
                    "\"{c}.{label}: length must be >= {v}\"",
                    v = min_len,
                    c = class_name
                ),
            );
        }
        if let Some(max_len) = constraints.max_length {
            self.emit_validation_error(
                &format!("{f}.length > {v}", f = field_name, v = max_len),
                &format!(
                    "\"{c}.{label}: length must be <= {v}\"",
                    v = max_len,
                    c = class_name
                ),
            );
        }
        if let Some(pattern) = &constraints.pattern {
            // SECURITY (#3): `pattern` is a source-derived string. A `/` or
            // newline in it closes a `/.../ ` regex literal early and injects
            // arbitrary JS; a `"` breaks the message literal. Build the regex
            // via `new RegExp(<encoded>)` and encode the message. field_name /
            // class_name are Python identifiers (no quote), safe as plaintext.
            self.emit_validation_error(
                &format!(
                    "!new RegExp({p}).test({f})",
                    f = field_name,
                    p = js_string_literal(pattern)
                ),
                &js_string_literal(&format!(
                    "{c}.{label}: must match pattern /{p}/",
                    p = pattern,
                    c = class_name
                )),
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
                    "\"{c}.{label}: must be a valid email\"",
                    c = class_name
                ),
            );
        }
        if constraints.url {
            self.emit_validation_error(
                &format!("!/^https?:\\/\\/.+/.test({f})", f = field_name),
                &format!(
                    "\"{c}.{label}: must be a valid URL\"",
                    c = class_name
                ),
            );
        }
        if constraints.uuid {
            self.emit_validation_error(
                &format!("!/^[0-9a-f]{{8}}-[0-9a-f]{{4}}-[0-9a-f]{{4}}-[0-9a-f]{{4}}-[0-9a-f]{{12}}$/i.test({f})", f = field_name),
                &format!("\"{c}.{label}: must be a valid UUID\"", c = class_name),
            );
        }
        if let Some(prefix) = &constraints.starts_with {
            // SECURITY (#3): encode the source-derived prefix in both the
            // condition and the message literal.
            self.emit_validation_error(
                &format!(
                    "!{f}.startsWith({p})",
                    f = field_name,
                    p = js_string_literal(prefix)
                ),
                &js_string_literal(&format!(
                    "{c}.{label}: must start with '{p}'",
                    p = prefix,
                    c = class_name
                )),
            );
        }
        if let Some(suffix) = &constraints.ends_with {
            // SECURITY (#3): encode the source-derived suffix.
            self.emit_validation_error(
                &format!(
                    "!{f}.endsWith({s})",
                    f = field_name,
                    s = js_string_literal(suffix)
                ),
                &js_string_literal(&format!(
                    "{c}.{label}: must end with '{s}'",
                    s = suffix,
                    c = class_name
                )),
            );
        }
        if let Some(substr) = &constraints.includes {
            // SECURITY (#3): encode the source-derived substring.
            self.emit_validation_error(
                &format!(
                    "!{f}.includes({s})",
                    f = field_name,
                    s = js_string_literal(substr)
                ),
                &js_string_literal(&format!(
                    "{c}.{label}: must include '{s}'",
                    s = substr,
                    c = class_name
                )),
            );
        }
        // Number validators
        if constraints.positive {
            self.emit_validation_error(
                &format!("{f} <= 0", f = field_name),
                &format!(
                    "\"{c}.{label}: must be positive\"",
                    c = class_name
                ),
            );
        }
        if constraints.negative {
            self.emit_validation_error(
                &format!("{f} >= 0", f = field_name),
                &format!(
                    "\"{c}.{label}: must be negative\"",
                    c = class_name
                ),
            );
        }
        if constraints.nonnegative {
            self.emit_validation_error(
                &format!("{f} < 0", f = field_name),
                &format!(
                    "\"{c}.{label}: must be nonnegative\"",
                    c = class_name
                ),
            );
        }
        if let Some(divisor) = constraints.multiple_of {
            self.emit_validation_error(
                &format!("{f} % {d} !== 0", f = field_name, d = format_f64(divisor)),
                &format!(
                    "\"{c}.{label}: must be a multiple of {d}\"",
                    d = format_f64(divisor),
                    c = class_name
                ),
            );
        }
        if constraints.finite {
            self.emit_validation_error(
                &format!("!Number.isFinite({f})", f = field_name),
                &format!(
                    "\"{c}.{label}: must be finite\"",
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
                "{c}.{label}: must be one of [{items}]",
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

    /// Emit type coercion for a dataclass field (when coerce=True). `label` is
    /// the RAW field name (message labels); `var` is the SANITIZED JS variable.
    fn emit_coercion(&mut self, class_name: &str, label: &str, var: &str, annotation: &Expr) {
        let tc = resolve_type_check(annotation);
        self.emit_coercion_for_type(class_name, label, var, &tc);
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

        // WB-16 (naming soundness, NB-1/NB-2 family): a Python method literally
        // named `constructor` lowers to a JS `constructor() {}` method — which
        // SILENTLY BECOMES the class's JS constructor, colliding with the
        // model's own construction path and producing an un-instantiable class
        // with no diagnostic. `constructor` is a reserved method slot in an
        // emitted class; make the collision a hard compile error (the same
        // discipline as the intrinsic-tag / #420 naming-collision family).
        if name == "constructor" && !emit_as_constructor {
            self.record_codegen_error(
                "`def constructor(self, ...)` collides with the JavaScript class \
                 `constructor` slot — the emitted method would silently replace the \
                 class constructor and make the class un-instantiable. Rename the method \
                 (Python's own constructor is `__init__`).",
            );
        }

        // Round-3: `__repr__`/`__str__` keep their REAL names so
        // repr()/str() can dispatch on them distinctly; emit_class_def
        // installs a `toString` alias (preferring __str__) for JS string
        // coercion (template literals, string concat).
        let js_name = if emit_as_constructor {
            "constructor"
        } else {
            name
        };

        // WB-15 — the SINGLE receiver decision that drives param-dropping, the
        // `__self`/`const cls` bindings, and how a bare `self` lowers in the
        // body. Computed once from the method kind + first param:
        //  * instance method (non-static, non-classmethod) whose first param is
        //    `self` → the instance receiver: dropped from the signature, `self`
        //    lowers to `this` (`__self` inside a nested `this`-rebinding scope).
        //  * @classmethod → the FIRST param (ANY name) is the CLASS receiver:
        //    dropped from the signature, aliased `const <name> = this`. If that
        //    name is literally `self`, the body's `self` reads that ordinary
        //    local (fixes `@classmethod def m(self)`; no receiver-lowering).
        //  * @staticmethod (and a `def m(me)` instance method) → NO receiver:
        //    every param is ordinary and KEPT (a static's `self`/`cls` param is
        //    a real argument, not `this`); the body's free `self` stays ordinary.
        let first_param_name = params.first().map(|p| p.name.as_str());
        let has_self_receiver =
            !is_static && !is_classmethod && first_param_name == Some("self");
        let class_receiver_name: Option<String> = if is_classmethod {
            first_param_name.map(str::to_string)
        } else {
            None
        };
        let drop_first = has_self_receiver || class_receiver_name.is_some();

        if is_generator {
            self.write(&format!("*{}(", js_name));
        } else {
            self.write(&format!("{}(", js_name));
        }
        self.emit_params_ctx(params, drop_first);
        self.write(") {\n");
        self.indent += 1;
        // autotester arguments: methods with a `*args, <kw-only>, **kwargs`
        // signature recover the keyword channel here too.
        self.emit_varargs_kw_prologue(params, name);
        // DX-B1 / issue #438: a method is a full Python scope. Push it with its
        // PRE-COMPUTED complete local binding set (params + method-body locals)
        // so a builtin-named param or local shadows the builtin regardless of
        // source order (incl. from a nested fn/lambda inside the method — case
        // G). Still `declare` the params for the incremental let-emission state.
        let param_names: Vec<String> = params.iter().map(|p| p.name.clone()).collect();
        self.push_scope(Self::collect_local_bindings(body, &param_names));
        self.set_scope_globals(Self::collect_global_declared(body));
        // Declare every param EXCEPT the dropped receiver (bound as `this` /
        // `const <name>`). A static method's `self`/`cls` param is a real local
        // and must be declared (else it reads as an undeclared global).
        for (i, param) in params.iter().enumerate() {
            if drop_first && i == 0 {
                continue;
            }
            self.declare(&param.name);
        }
        self.record_param_types(params);

        // WB-15 — set the single `self`-lowering predicate for this method body
        // from the receiver decision above, and (for an instance receiver with a
        // nested `this`-rebinding scope) capture the `__self` alias. A method is
        // a fresh JS `this` frame, so this fully OVERRIDES the enclosing state.
        let prev_self_lowering = self.self_lowering;
        // WB-15 (S6): the method's OWN scope may REBIND `self` as a local — an
        // assignment/for/with/except target named `self` (`self = 5`,
        // `for self in xs`). Python makes `self` an ordinary local for the whole
        // method then; JS `this` is not assignable, so the receiver is captured
        // once into a mutable `let self = this;` and every `self` reference — read
        // or write — is the ordinary local. (A native constructor may not touch
        // `this` before super(), and rebinding `self` in `__init__` is absurd, so
        // this path is gated to non-constructors.)
        let self_rebound_locally = has_self_receiver
            && !emit_as_constructor
            && Self::collect_local_bindings(body, &[]).contains("self");
        // Whether this instance method must capture `const __self = this;` — it
        // owns a receiver, is NOT locally rebinding `self`, AND contains a nested
        // `function`/`class`/static that rebinds `this` and could close over
        // `self`. Placed after super() in a native constructor; at the body top
        // otherwise.
        let emit_self_alias =
            has_self_receiver && !self_rebound_locally && Self::contains_nested_scope(body);
        if self_rebound_locally {
            // Receiver captured as a mutable local; `self` is Ordinary throughout
            // (reads AND the rebinding write). Declared here so the hoister and
            // the assignment path both see it as already-bound (`self = 5`, not
            // `let this = 5`).
            self.self_lowering = SelfLowering::Ordinary;
            self.write_indent();
            self.write("let self = this;\n");
            self.declare("self");
            self.mark_hoisted("self");
        } else if has_self_receiver {
            self.self_lowering = SelfLowering::Receiver;
        } else if class_receiver_name.as_deref() == Some("self") {
            // `@classmethod def m(self)`: `self` is the class param, bound below
            // as `const self = this` — an ordinary identifier reads that local.
            self.self_lowering = SelfLowering::Ordinary;
        } else {
            // static / classmethod(cls) / non-`self` instance first param: a
            // fresh `this` frame that does NOT bind `self`. A live outer
            // receiver survives via `__self`; otherwise `self` stays ordinary.
            self.cross_self_this_boundary();
        }
        if emit_self_alias && !emit_as_constructor {
            self.write_indent();
            self.write("const __self = this;\n");
        }

        // @classmethod: the first param (any name) is the class — JS `this` in a
        // static method (subclass static dispatch keeps it accurate). Bind it
        // under its ACTUAL name so `@classmethod def m(self)` resolves `self`.
        let prev_in_classmethod = self.in_classmethod;
        if let Some(rname) = &class_receiver_name {
            self.in_classmethod = true;
            self.write_indent();
            self.write(&format!("const {} = this;\n", Self::sanitize_ident(rname)));
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
            // WB-15 (S2): capture the receiver alias AFTER super() — a derived
            // constructor may not touch `this` before super() — so a nested
            // `function`/`class` inside the constructor reads the receiver via
            // `__self` instead of its own rebound `this`.
            if emit_self_alias {
                self.write_indent();
                self.write("const __self = this;\n");
            }
            // WB-5: hoist function-scope `let`s AFTER super() (a derived
            // constructor may not touch anything before super()) so a local
            // assigned only inside a branch is method-scoped, not block-scoped.
            self.emit_hoisted_local_decls(body);
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
            // WB-5: a method body is a full Python scope — hoist function-scope
            // `let`s for locals first-assigned inside a branch/loop, exactly as
            // emit_func_def does, so `if c: x=a else: x=b; return x` no longer
            // emits a block-scoped `let x` + a bare else-branch `x` (a strict-ESM
            // ReferenceError).
            self.emit_hoisted_local_decls(body);
            for stmt in body {
                self.emit_stmt(stmt);
            }
        }

        self.await_ok = prev_await_ok;
        self.in_classmethod = prev_in_classmethod;
        self.self_lowering = prev_self_lowering;
        self.pop_scope();
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
        // step (literal 1 when absent) — evaluated once, in Python arg order.
        if args.len() == 3 {
            self.write_indent();
            self.write(&format!("const {} = ", step_t));
            self.emit_expr(&args[2]);
            self.write(";\n");
        }
        let step_ref = if args.len() == 3 { step_t.as_str() } else { "1" };

        // ROOT FIX: iterate the shared LAZY `__pyRangeIter` instead of a
        // hand-rolled value-controlled `i += step` counter. That counter
        // diverged from pyRange — a BigInt/Number-mix crash, a 2**53
        // non-progress hang, a rejected `range(True)`. The generator applies
        // the SAME __pyRangeNorm guards + BigInt/2**53-safe counted stepping and
        // stays lazy (a huge finite range never materializes). `const` per
        // iteration matches the for-of binding (correct closure capture).
        self.need_runtime("__pyRangeIter");
        self.write_indent();
        self.write(&format!(
            "for (const {} of __pyRangeIter({}, {}, {})) {{\n",
            ri, start_t, stop_t, step_ref
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
        // #452: mark the target declared only AFTER the iterable is emitted —
        // Python evaluates the iterable in the ENCLOSING environment before
        // the target binds, so at module scope `for list in list(xs)` must
        // lower `list(xs)` to the builtin, not to the not-yet-bound loop
        // variable (a JS TDZ ReferenceError). The body below still sees the
        // declaration.
        self.declare_target(target);
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
            self.need_runtime(name);
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
                    // SECURITY (#13): sanitize the exception alias — a reserved
                    // word (`let`, `default`, ...) would emit `let let = __exc`
                    // (SyntaxError). Body references go through the same
                    // sanitize_ident, so the rename stays consistent.
                    self.writeln(&format!("let {} = __exc;", Self::sanitize_ident(name)));
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
                    self.need_runtime(name);
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
            // autotester control_structures: the `as` target must ASSIGN, not
            // re-declare — a second `with A() as x:` (or `with A() as x,
            // B() as y:` twice) in the same JS block scope emitted a duplicate
            // `const x` → "Identifier 'x' has already been declared". Stash
            // __enter__'s result in the per-statement unique temp, then route
            // the target through emit_assign — the single assignment path
            // that already handles first-use `let`, rebinding, tuple/list
            // destructuring, and subscript/attribute targets.
            let res = format!("{}_r", mgr);
            self.write(&format!(
                "const {r} = ({m} !== null && typeof {m}.{e} === \"function\") ? {aw}{m}.{e}() : {m};\n",
                r = res,
                m = mgr,
                e = enter,
                aw = aw
            ));
            let res_expr = Expr {
                kind: ExprKind::Name(res),
                span: item.context_expr.span,
            };
            self.emit_assign(std::slice::from_ref(var), &res_expr);
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
                        | "bytes" | "bytearray"
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
                    // SECURITY (#13): sanitize the capture binding — reserved
                    // words emitted `let let = ...` (SyntaxError). Matches the
                    // hoisted branch above and general Name references.
                    self.writeln(&format!("let {} = {};", Self::sanitize_ident(name), subject));
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
                        // SECURITY (#9): `s` is a source-derived key spliced
                        // between JS quotes. A `"`/newline/backslash in it broke
                        // out of the literal; encode it via js_string_literal.
                        let k = js_string_literal(s);
                        self.emit_pattern_bindings(
                            pat,
                            &format!(
                                "({subj} instanceof Map ? {subj}.get({k}) : {subj}[{k}])",
                                subj = subject,
                                k = k
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
                    // SECURITY (#13): sanitize the as-pattern binding.
                    self.writeln(&format!("let {} = {};", Self::sanitize_ident(name), subject));
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
            ExprKind::FloatLiteral(n) => {
                // Option B (minimal): ONLY an integer-valued float literal
                // (8.0) boxes — it is otherwise indistinguishable from int 8
                // at runtime (containers, dynamic contexts). A non-integer
                // literal (3.14) stays a bare native Number: Number.isInteger
                // already discriminates it, and native floats keep JS-interop
                // and arithmetic at full native speed. The boxing decision is
                // static here; __pyF re-checks only for runtime-computed
                // values. NaN/inf can't appear as literals (they parse as
                // names), so `n` is always finite.
                if n.fract() == 0.0 && n.is_finite() {
                    self.need_runtime("__pyF");
                    self.write(&format!("__pyF({})", n));
                } else {
                    self.write(&format!("{}", n));
                }
            }
            // autotester byte_arrays: bytes literal -> immutable PyBytes
            // (Uint8Array subclass) built from the raw byte values.
            ExprKind::BytesLiteral(bytes) => {
                self.need_runtime("pyBytes");
                self.write("pyBytes([");
                for (i, b) in bytes.iter().enumerate() {
                    if i > 0 {
                        self.write(", ");
                    }
                    self.write(&b.to_string());
                }
                self.write("])");
            }
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
                                self.emit_format_float_arg(e);
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
                // autotester properties: inside a post-class attribute value,
                // a sibling method name is class-local (Python class-body
                // scoping) — emit it as Cls.prototype.name.
                if let Some((cls, names)) = &self.class_attr_subst {
                    if names.contains(name) {
                        let out = format!("{}.prototype.{}", cls, name);
                        self.write(&out);
                        return;
                    }
                }
                if name == "self" {
                    // WB-15: ONE predicate decides this — `self_lowering`, set
                    // from the enclosing method's real receiver structure (see
                    // `SelfLowering`). `Receiver` → JS `this`; `ReceiverAlias` →
                    // the `__self` const captured in the enclosing instance
                    // method (nested `function`/`class`/static rebinds `this`);
                    // `Ordinary` → the bare identifier `self` (module scope, a
                    // plain function, a static/classmethod body, or a `self`
                    // param/const of a nearer scope). Emitting `this` for an
                    // ordinary `self` produced `export let this = …` (a hard
                    // syntax error) / bound the call-site receiver instead.
                    self.write(match self.self_lowering {
                        SelfLowering::Receiver => "this",
                        SelfLowering::ReceiverAlias => "__self",
                        SelfLowering::Ordinary => "self",
                    });
                } else {
                    // Synthetic runtime-helper references injected by
                    // earlier lowering passes (e.g. pyFormatSpec from
                    // f-string spec lowering) need to be imported even
                    // though they appear as bare Name nodes in the AST.
                    // #452 family: never in a WRITE/binding position — a
                    // target named `pyStr` is a user binding, and importing
                    // the helper would collide with it.
                    if !self.in_lhs_target
                        && matches!(
                            name.as_str(),
                            // pyRepr/pyStr/pyAscii: injected by f-string
                            // `!r`/`!s`/`!a` conversions and `{x=}` self-doc
                            // (Pythonic-checks; !a wired in public #3).
                            "pyFormatSpec"
                                | "pyFormatDynamic"
                                | "pyNormalizeStyle"
                                | "pyFixed"
                                | "pyRepr"
                                | "pyStr"
                                | "pyAscii"
                        )
                        && !self.is_declared(name)
                    {
                        self.need_runtime(name);
                    }
                    // autotester docstrings: a bare `__doc__` read is the
                    // module docstring (None when absent) unless shadowed.
                    // #452 family: a WRITE target is always the user binding.
                    if name == "__doc__"
                        && !self.in_lhs_target
                        && !self.is_declared_in_any_scope(name)
                    {
                        match self.module_doc.clone() {
                            Some(d) => self.write(&js_string_literal(&d)),
                            None => self.write("null"),
                        }
                        return;
                    }
                    // Star-import bindings (`from math import *`): an
                    // undeclared reference to a bound export resolves to
                    // `<ns>.<name>`. BEFORE the builtin value mapping —
                    // CPython's star-import rebinds the namespace, so
                    // `pow`/`e` mean the module's, not the builtin.
                    // #452 family: never for a WRITE/binding position — a
                    // write creates a new local binding, it never assigns
                    // into the imported namespace.
                    if !self.in_lhs_target && !self.is_declared_in_any_scope(name) {
                        if let Some((ns, _)) = self.star_import_bindings.get(name) {
                            self.write(&format!("{}.{}", ns, name));
                            return;
                        }
                    }
                    // #110: Python builtins referenced as VALUES (not
                    // called) — defaultdict(list), starmap(pow),
                    // key=len, map(int, ...). Previously these passed
                    // through as bare JS identifiers → ReferenceError.
                    // Shadowing (params, locals, imports, user defs) wins.
                    // DX-B1: use is_declared_in_any_scope, not is_declared —
                    // a binding named like a builtin (`set`/`list`/`dict`/`str`/
                    // …) in an ENCLOSING function scope must shadow the builtin
                    // when referenced from a nested fn/lambda/method. The
                    // innermost-only is_declared missed it and mis-lowered
                    // zustand's canonical `def store(set, get): def inc(): set(…)`
                    // to `pySetOf` — a silent miscompile (TypeError, zero
                    // actions). This mirrors the call-form guard below.
                    // #452 ROOT: never in a WRITE/binding position. A for /
                    // comprehension target (or any assignment target) named
                    // `list`/`dict`/… is a USER binding by construction — the
                    // old unguarded mapping emitted the builtin value as the
                    // loop variable (`for (const __pyTypeList of …)`) while
                    // body reads stayed `list` → ReferenceError. Every
                    // binding position funnels through here with
                    // `in_lhs_target` set, so this one guard closes the class.
                    if !self.in_lhs_target && !self.is_declared_in_any_scope(name) {
                        if let Some((js, deps)) = crate::builtins::builtin_value_mapping(name) {
                            for d in deps {
                                self.need_runtime(d);
                            }
                            self.write(js);
                            return;
                        }
                    }
                    // public #3: a KNOWN but unimplemented builtin referenced
                    // as a VALUE (`f = open`, `print(id)`, `key=hash`) — the
                    // same compile-error gate as the call form (see
                    // emit_call). Guards mirror the mapping path above; a
                    // WRITE target (in_lhs_target) is a user binding, and
                    // inside a @component an HTML-element-named reference
                    // (input/map/object) is left to the PSX machinery.
                    if !self.in_lhs_target
                        && !self.is_declared_in_any_scope(name)
                        && !self.star_import_bindings.contains_key(name)
                        && !self.known_functions.contains(name)
                        && !self.known_classes.contains(name)
                        && (!self.in_component || !react::is_html_element(name))
                        && crate::builtins::unsupported_builtin(name)
                    {
                        let diag = crate::builtins::unsupported_builtin_message(name);
                        self.emit_expr_error(&diag);
                        return;
                    }
                    // #448 (value position): `import_module` lowers to the ES
                    // dynamic `import(...)` KEYWORD form — it is not a first-class
                    // JS value, so `f = import_module` / passing it as an arg
                    // cannot work. The old code emitted a bare `import_module`
                    // identifier (undefined at runtime, no diagnostic). Covers
                    // both the bare builtin and a `from importlib import
                    // import_module [as X]` binding used in value position.
                    if !self.in_lhs_target
                        && !self.is_declared_in_any_scope(name)
                        && (self.import_module_fns.contains(name)
                            || (name == "import_module"
                                && matches!(
                                    crate::builtins::builtin_func_mapping(name),
                                    Some(crate::builtins::BuiltinMapping::NativeCall(_))
                                )))
                    {
                        self.emit_expr_error(
                            "`import_module` lowers to the native dynamic `import(...)` form \
                             and cannot be used as a value — only called directly, e.g. \
                             `await import_module(\"./mod.js\")`.",
                        );
                        return;
                    }
                    // #448 CLASS rule: ANY reference to a tracked `import
                    // importlib [.sub] [as X]` namespace — bare `importlib`,
                    // `importlib.reload(x)`, `importlib.util.find_spec(...)`,
                    // `f = importlib.import_module` — is diagnosed. The
                    // namespace deliberately emits no binding (importlib is not
                    // a real module in the compiled output), so the old code
                    // emitted a bare unbound identifier → ReferenceError with
                    // no diagnostic (worse than pre-fix). The original fix
                    // diagnosed only the `.import_module` member-call shape;
                    // this one rule at the NAME, the root of every reference
                    // form, closes the whole class. A user binding that shadows
                    // the name (declared) wins, as does a write target.
                    if !self.in_lhs_target
                        && self.importlib_namespaces.contains(name)
                        && !self.is_declared_in_any_scope(name)
                    {
                        self.emit_expr_error(&format!(
                            "`{name}` (importlib) has no runtime binding in the compiled \
                             output — only `import_module` is supported: use `from importlib \
                             import import_module` (or the bare `import_module(...)` builtin) \
                             and call `await import_module(\"./mod.js\")`. Other importlib \
                             APIs (reload, util, machinery, …) cannot be lowered \
                             (pythscribe-v3.x).",
                        ));
                        return;
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
                    // DX-B2 alias-and-rewrite: a module-scope import whose JS
                    // binding collided with an earlier import's (snake→camel
                    // convergence) was hoisted under a unique name — rewrite
                    // its reference sites to that name. Checked BEFORE the
                    // react_imports camelCase match (the camel form may be
                    // exactly the colliding name). A FUNCTION-scope binding of
                    // the same name shadows the module import (Python scoping)
                    // and skips the rewrite — the pre-pass `scope_bindings`
                    // makes that check order-independent.
                    if !self.scope_bindings.iter().skip(1).any(|s| s.contains(name)) {
                        if let Some(u) = self.import_ref_renames.get(name).cloned() {
                            self.write(&u);
                            return;
                        }
                    }
                    // Names imported from React-like modules without an
                    // alias were camelCased on the import line. Match
                    // that on the reference side so the JS binding
                    // resolves cleanly. Without this, `from foo import
                    // use_query; use_query()` emits `import { useQuery }
                    // from "foo"` followed by `use_query()` — the local
                    // binding mismatches and crashes at runtime.
                    //
                    // B8(a) CLASS rule: the import→camel rename applies ONLY
                    // to references that actually resolve to the import. A
                    // param/local of the SAME Python name in any enclosing
                    // function scope shadows it (Python scoping), so the
                    // reference must stay on the Python name — the old
                    // unguarded rename silently rewrote `def f(create_store):
                    // return create_store` to return the IMPORT. Same
                    // predicate as the import_ref_renames guard above.
                    if self.react_imports.contains(name)
                        && !self.scope_bindings.iter().skip(1).any(|s| s.contains(name))
                    {
                        self.write(&react::snake_to_camel(name));
                        return;
                    }
                    // PBT-2 / #452: READ of a sentinel-initialized for-target
                    // — guard it so an unbound read raises like CPython, with
                    // the guard chosen by which SCOPE OWNS the variable (see
                    // sentinel_read). Writes (in_lhs_target) stay bare so
                    // assignments and the loop binding itself overwrite the
                    // sentinel.
                    let sentinel = if self.in_lhs_target {
                        None
                    } else {
                        self.sentinel_read(name)
                    };
                    match sentinel {
                        Some(SentinelRead::Global) => {
                            // #452: GLOBAL lookup is CPython's dynamic
                            // globals → builtins chain — an unbound
                            // builtin-named global (`for list in list(xs)`
                            // before/without an iteration, read at module
                            // scope or from a nested function) resolves to
                            // the BUILTIN value, it does not raise. Other
                            // names raise NameError.
                            if let Some((js, deps)) =
                                crate::builtins::builtin_value_mapping(name)
                            {
                                for d in deps {
                                    self.need_runtime(d);
                                }
                                self.need_runtime("__UNBOUND");
                                let id = Self::sanitize_ident(name);
                                self.write(&format!("({id} === __UNBOUND ? {js} : {id})"));
                            } else {
                                self.need_runtime("__pyChkGlobal");
                                self.write(&format!(
                                    "__pyChkGlobal({}, \"{}\")",
                                    Self::sanitize_ident(name),
                                    name
                                ));
                            }
                        }
                        Some(SentinelRead::Local) => {
                            self.need_runtime("__pyChkLocal");
                            self.write(&format!(
                                "__pyChkLocal({}, \"{}\")",
                                Self::sanitize_ident(name),
                                name
                            ));
                        }
                        Some(SentinelRead::Free) => {
                            // #452 blocker 2: a closure read of an unbound
                            // ENCLOSING-function local is CPython's
                            // free-variable NameError — never the raw
                            // sentinel value, and not UnboundLocalError
                            // (that belongs to the owning scope itself).
                            self.need_runtime("__pyChkFree");
                            self.write(&format!(
                                "__pyChkFree({}, \"{}\")",
                                Self::sanitize_ident(name),
                                name
                            ));
                        }
                        None => self.write(&Self::sanitize_ident(name)),
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
                // autotester arguments (#203): `x.__class__` is type(x) — the
                // compiled model has no __class__ property on primitives or
                // plain-object dicts, so route through the value-aware
                // runtime type() (whose result carries __name__).
                if attr == "__class__" && !*optional {
                    self.need_runtime("pyType");
                    self.write("pyType(");
                    self.emit_expr(value);
                    self.write(")");
                    return;
                }
                // 0.2.2 member-call class fix (VALUE position): a member READ
                // on a core-React namespace alias (`f = react.create_element`,
                // `g = react_dom.create_portal`) routes through the SAME
                // `react::route_namespace_member` rule as the call form. The
                // old lowering wrapped the raw snake member in pyBoundMethod
                // (`pyBoundMethod(react, "create_element")` — a dead reference:
                // the namespace exports only camelCase). ESM namespace members
                // are plain functions, so no bound-method wrap either: emit the
                // routed camelCase export, or the removed / wrong-module
                // compile diagnostic. Write position (in_lhs_target) falls
                // through — assigning into a frozen ESM namespace is its own
                // loud runtime TypeError, and an inline throw-expression is not
                // a valid assignment target.
                if !self.in_lhs_target {
                    if let ExprKind::Name(base) = &value.kind {
                        if let Some(&src) = self.react_namespace_alias_modules.get(base) {
                            match react::route_namespace_member(src, attr) {
                                react::MemberRoute::Removed(msg) => {
                                    self.emit_expr_error(msg);
                                    return;
                                }
                                react::MemberRoute::WrongModule {
                                    js_name,
                                    exports_from,
                                } => {
                                    self.emit_expr_error(&format!(
                                        "`{base}.{attr}` — `{js_name}` is exported by \
                                         \"{}\", not \"{}\": the member access would \
                                         be `undefined` at runtime. Import it from \
                                         the right module, or use `from pyths.react \
                                         import {attr}` (auto-routes to the correct \
                                         package).",
                                        exports_from.module(),
                                        src.module(),
                                    ));
                                    return;
                                }
                                react::MemberRoute::Routed(js_name) => {
                                    self.emit_expr(value);
                                    self.write(if *optional { "?." } else { "." });
                                    self.write(&js_name);
                                    return;
                                }
                            }
                        }
                        // Broader react-ecosystem namespace aliases: same
                        // snake→camel member transform as the from-import path
                        // (and as the call form above); plain functions, so no
                        // pyBoundMethod wrap.
                        if self.react_lib_module_aliases.contains(base) && attr.contains('_') {
                            self.emit_expr(value);
                            self.write(if *optional { "?." } else { "." });
                            self.write(&react::snake_to_camel(attr));
                            return;
                        }
                    }
                }
                // #266 → autotester simple_and_augmented_assignment (root
                // fix): ANY attribute read in VALUE position (`g = obj.m`,
                // `key=d.get`, arg positions, container elements) is Python
                // attribute access — a function attribute is a BOUND method
                // carrying its receiver (`g = a.f; g()` must keep self).
                // pyBoundMethod passes data attributes straight through (one
                // typeof check), synthesizes the dict-method closures, and
                // raises AttributeError on a None receiver. Excluded:
                //   - assignment targets (in_lhs_target — write position),
                //   - method CALLS (emit_call writes the callee itself, so
                //     `a.m(x)` never reaches this arm),
                //   - stdlib module namespaces (plain functions),
                //   - dunder-protocol reads (`__name__` above).
                if !*optional
                    && !self.in_lhs_target
                    && !self.in_call_callee
                    && !matches!(&value.kind, ExprKind::Name(n) if self.module_namespaces.contains(n))
                    && !matches!(&value.kind, ExprKind::Name(n) if self.asyncio_namespaces.contains(n))
                    && !attr.starts_with("__")
                {
                    self.need_runtime("pyBoundMethod");
                    self.write("pyBoundMethod(");
                    self.emit_expr(value);
                    // Error-kind round 3: a receiver statically proven DICT
                    // gets the strict flag — at runtime a plain-object dict
                    // is indistinguishable from a JS-interop object (React
                    // props), so pyBoundMethod raises AttributeError on an
                    // absent attribute only when the codegen vouches the
                    // receiver is a Python dict. Brand-carrying containers
                    // (Array/Map/Set) raise without the flag.
                    if matches!(self.infer_type(value), JsInferredType::Dict) {
                        self.write(&format!(", {:?}, 1)", attr));
                    } else {
                        self.write(&format!(", {:?})", attr));
                    }
                    return;
                }
                // `123.foo` is a JS syntax error (the lexer eats `123.`
                // as a numeric literal). Wrap int-literal receivers in
                // parens. Floats already have a `.` and are unaffected.
                let needs_paren = matches!(&value.kind, ExprKind::IntLiteral(_));
                if needs_paren {
                    self.write("(");
                }
                // #452 review blocker 1: the RECEIVER of an attribute STORE
                // (`obj.attr = v` reaches here with in_lhs_target set) is a
                // READ context — only the stored attribute name is the write
                // target. Reset the flag for the receiver and all its
                // subexpressions (mirrors the Subscript arm), so e.g.
                // `wrap(list).attr = v` still lowers the builtin-named value
                // arg / star-import / sentinel reads inside the receiver.
                let was_lhs = self.in_lhs_target;
                self.in_lhs_target = false;
                self.emit_expr(value);
                self.in_lhs_target = was_lhs;
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
            ExprKind::Slice { lower, upper, step } => {
                // A slice in VALUE position (an element of a subscript
                // tuple — `a[1:2:3, 4:5:6]` — or a bare slice object):
                // a real PySlice with CPython's .indices(len). The direct
                // `a[i:j:k]` form stays on the pySlice fast path above.
                self.need_runtime("__pySliceObj");
                self.write("__pySliceObj(");
                for (i, part) in [lower, upper, step].iter().enumerate() {
                    if i > 0 {
                        self.write(", ");
                    }
                    match part {
                        Some(e) => self.emit_expr(e),
                        None => self.write("null"),
                    }
                }
                self.write(")");
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
                // Issue #438 (case E): a comprehension is its own scope; its
                // for-targets shadow builtins inside the element/conditions.
                self.push_scope(Self::comprehension_target_names(generators));
                self.emit_collect_comprehension(&CompAccum::Element(elt), generators);
                self.pop_scope();
            }
            ExprKind::DictComp {
                key,
                value,
                generators,
            } => {
                self.push_scope(Self::comprehension_target_names(generators));
                self.emit_dict_comprehension(key, value, generators);
                self.pop_scope();
            }
            ExprKind::SetComp { elt, generators } => {
                // #297: canonicalizing PySet (see ExprKind::Set). The set
                // container init is this wrap — the element stream is the
                // same unified lowering as list comps.
                self.need_runtime("PySet");
                self.write("new PySet(");
                self.push_scope(Self::comprehension_target_names(generators));
                self.emit_collect_comprehension(&CompAccum::Element(elt), generators);
                self.pop_scope();
                self.write(")");
            }
            ExprKind::GeneratorExp { elt, generators } => {
                // #155: genexps lower to REAL lazy JS generators (iterator
                // protocol), not eager arrays — `next(genexp, default)`,
                // laziness side-effect ordering, and iter() identity all
                // depend on it.
                self.push_scope(Self::comprehension_target_names(generators));
                self.emit_generator_exp(elt, generators);
                self.pop_scope();
            }
            ExprKind::Lambda { params, body } => {
                // autotester lambda_functions: a lambda whose signature needs
                // the varargs keyword channel (`lambda *a, **kw: …`) emits a
                // BLOCK-body arrow so the prologue has somewhere to run.
                let needs_kw_block = Self::varargs_kw_split(params).is_some();
                // A varargs lambda needs calling-convention metadata so
                // __pyKwArgs routes keywords through the marked carrier —
                // a lambda has no declaration to hang the post-assignments
                // on, so wrap the expression in __pyFnMeta.
                let star = params.iter().position(|p| p.is_args);
                // S2: ANY varargs lambda needs a block body so the
                // `*args`-is-a-tuple marker prologue has somewhere to run.
                let needs_block = needs_kw_block || star.is_some();
                let needs_meta = star.is_some();
                let meta_names: String = params
                    .iter()
                    .enumerate()
                    .filter(|(i, p)| !p.is_kwargs && !p.is_args && star.is_none_or(|s| *i < s))
                    .map(|(_, p)| format!("\"{}\"", p.name))
                    .collect::<Vec<_>>()
                    .join(", ");
                // WB-14: Python evaluates default args ONCE at def-time (frozen);
                // a bare JS default param `(p = expr) => …` re-evaluates at
                // CALL-time. Two divergences: (1) a default reading a loop/outer
                // variable reassigned after the lambda is defined sees the LAST
                // value, not the def-time snapshot; (2) `lambda x=x` lowers to
                // `(x = x) =>` whose RHS resolves to the PARAMETER `x` in its own
                // temporal dead zone → ReferenceError. Fix: hoist each default to
                // a def-time const via an outer IIFE that captures the
                // enclosing-scope values once — `((__ld0) => (p = __ld0) => …)(expr)`
                // — so the value is frozen at def-time and the `x=x`
                // self-reference is broken (the const captures the OUTER `x`).
                // `*args`/`**kwargs` never carry defaults, so they're excluded.
                // An immutable CONSTANT-literal default (`None`, a number, a
                // string, a bool) is identical whether evaluated at def-time or
                // call-time and needs no snapshot — leave it a bare JS default
                // param (keeps `lambda e=None:` event handlers as `(e = null)`).
                // Only a default that could observe def-time state (a Name that
                // may be reassigned, a mutable literal, a call, …) is hoisted.
                let default_params: Vec<&Param> = params
                    .iter()
                    .filter(|p| {
                        !p.is_args
                            && !p.is_kwargs
                            && p.default.as_ref().is_some_and(|d| !is_const_literal(d))
                    })
                    .collect();
                let has_defaults = !default_params.is_empty();
                let saved_default_hoists = self.param_default_hoists.clone();
                if has_defaults {
                    self.write("((");
                    for (k, p) in default_params.iter().enumerate() {
                        if k > 0 {
                            self.write(", ");
                        }
                        let c = format!("__ld${}", self.default_hoist_counter);
                        self.default_hoist_counter += 1;
                        self.write(&c);
                        // emit_params references this const for the param default
                        // (instead of re-emitting the call-time expression).
                        self.param_default_hoists.insert(p.name.clone(), c);
                    }
                    self.write(") => ");
                }
                if needs_meta {
                    self.need_runtime("__pyFnMeta");
                    self.write("__pyFnMeta(");
                }
                self.write("(");
                self.emit_params(params);
                self.write(") => ");
                // Round-4 sweep: a plain arrow can't await.
                let prev_await_ok = self.await_ok;
                self.await_ok = false;
                // Declare the lambda's params in a fresh scope so name-resolution
                // inside the body sees them — in particular so a param that
                // shadows a builtin (`lambda set: set(…)`) calls the PARAM, not
                // the `set()` builtin. Issue #438: the binding set is the params
                // plus any walrus targets in the (expression) body.
                let mut binds: HashSet<String> = params.iter().map(|p| p.name.clone()).collect();
                Self::collect_walrus_targets(body, &mut binds);
                self.push_scope(binds);
                for p in params {
                    self.declare(&p.name);
                }
                // WB-15 (S4): a lambda param named `self` shadows any enclosing
                // instance-method receiver — inside this arrow `self` is that
                // ordinary param, not `this`/`__self`. (Arrows otherwise inherit
                // the enclosing `this`, so a non-`self`-param lambda keeps the
                // receiver lowering — `lambda x: x + self.k` stays `this.k`.)
                let prev_self_lowering_lambda = self.self_lowering;
                if params.iter().any(|p| p.name == "self") {
                    self.self_lowering = SelfLowering::Ordinary;
                }
                if needs_block {
                    self.write("{\n");
                    self.indent += 1;
                    self.emit_varargs_kw_prologue(params, "<lambda>");
                    self.write_indent();
                    self.write("return ");
                    self.emit_expr(body);
                    self.write(";\n");
                    self.indent -= 1;
                    self.write_indent();
                    self.write("}");
                } else {
                    self.emit_expr(body);
                }
                if needs_meta {
                    self.write(&format!(
                        ", [{}], {}, true)",
                        meta_names,
                        if needs_kw_block { "true" } else { "false" }
                    ));
                }
                // Restore before the popped scope / the def-time default IIFE
                // args (emitted in the ENCLOSING scope below).
                self.self_lowering = prev_self_lowering_lambda;
                self.pop_scope();
                self.await_ok = prev_await_ok;
                // WB-14: close the def-time IIFE — its arguments are the default
                // expressions, evaluated ONCE, now in the ENCLOSING scope (the
                // lambda's param scope has been popped, so `lambda x=x` reads the
                // OUTER `x`). Restore the hoist map first so the defaults resolve
                // with the enclosing function's mappings, not the lambda's.
                if has_defaults {
                    self.param_default_hoists = saved_default_hoists;
                    self.write(")(");
                    for (k, p) in default_params.iter().enumerate() {
                        if k > 0 {
                            self.write(", ");
                        }
                        self.emit_expr(p.default.as_ref().unwrap());
                    }
                    self.write(")");
                } else {
                    self.param_default_hoists = saved_default_hoists;
                }
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
                // autotester iterators_and_generators: yield-as-EXPRESSION —
                // always parenthesized. JS rejects a bare `yield` in most
                // nested expression positions (`pyAdd(r, yield r)` is a
                // SyntaxError; `(yield r)` is valid everywhere in a
                // generator), and `r + (yield r)` is exactly the send()
                // protocol shape the testlets exercise.
                self.write("(yield");
                if let Some(v) = value {
                    self.write(" ");
                    self.emit_expr(v);
                }
                self.write(")");
            }
            ExprKind::YieldFrom(inner) => {
                self.write("(yield* ");
                self.emit_expr(inner);
                self.write(")");
            }
            ExprKind::NamedExpr { target, value } => {
                // Walrus operator: (target = value)
                // PBT-2: the target is a WRITE position — a sentinel-guarded
                // name must emit bare (`(i = v)`), not as a __pyChkLocal read
                // (which would be an invalid JS assignment target).
                // #443: a walrus target is a non-import rebind — forget any
                // import identity this scope cached for the name, so a later
                // re-import re-emits instead of deduping to the walrus value
                // (`from math import floor; (floor := f); from math import
                // floor` must restore math.floor).
                if let ExprKind::Name(n) = &target.kind {
                    self.invalidate_import_decl(n);
                }
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

    /// Option-B spike: bare float op with box-unwrap (`+`) on both operands
    /// and a re-box of the result — the float fast path under boxed floats.
    /// Option B: emit `value` into a NATIVE JS sink position (a JS built-in
    /// constructor argument, a React `style` value). A boxed (integer-valued)
    /// float must arrive as a native `Number` there — these sinks dispatch on
    /// `typeof === "number"` (`Array(n)` length-vs-single-element, React's
    /// px-append), and `valueOf()` coercion never runs for a typeof check.
    /// A float LITERAL emits bare (statically known); a statically-`Float`
    /// expression unwraps through `__pyJs`; an `Unknown` expression is
    /// wrapped too when `wrap_unknown` (it may hold a runtime-boxed float);
    /// every other static type can never be boxed and emits plain.
    fn emit_native_sink_value(&mut self, value: &Expr, wrap_unknown: bool) {
        if let ExprKind::FloatLiteral(n) = &value.kind {
            self.write(&format!("({})", n));
            return;
        }
        let t = self.infer_type(value);
        let needs_unbox = matches!(t, JsInferredType::Float)
            || (wrap_unknown && matches!(t, JsInferredType::Unknown));
        if needs_unbox {
            self.need_runtime("__pyJs");
            self.write("__pyJs(");
            self.emit_expr(value);
            self.write(")");
        } else {
            self.emit_expr(value);
        }
    }

    /// Option B: a JSX CHILD — a `createElement(tag, props, ...children)`
    /// positional — is a native React sink: React THROWS on an object child
    /// ("Objects are not valid as a React child (found: [object Number])"),
    /// so a boxed integer-valued float must cross as a primitive. ONE rule
    /// for every child surface (PSX element/fragment children, both factory
    /// forms): float literals emit bare, statically-Float and Unknown
    /// expressions unwrap through __pyJs (a non-box passes through
    /// untouched). The React oracle renders a native number child via JS
    /// toString (8.0 → "8") — that IS the parity target, so no pyStr here.
    /// Spread children (`*xs` → `...xs`) pass through verbatim — wrapping a
    /// spread would be invalid JS; boxed floats INSIDE spread/list children
    /// remain a documented container-boundary residual.
    fn emit_jsx_child(&mut self, child: &Expr) {
        if matches!(child.kind, ExprKind::Starred(_)) {
            self.emit_expr(child);
        } else {
            self.emit_native_sink_value(child, true);
        }
    }

    fn emit_binop_bare_float(&mut self, left: &Expr, op: &str, right: &Expr) {
        self.need_runtime("__pyF");
        self.write("__pyF(");
        self.emit_float_operand_unboxed(left);
        self.write(" ");
        self.write(op);
        self.write(" ");
        self.emit_float_operand_unboxed(right);
        self.write(")");
    }

    /// Argument to a `pyFormatFloat(...)` preformat call: the formatter only
    /// needs the numeric VALUE, so a float literal (or negated float
    /// literal) emits bare instead of boxing via __pyF just for
    /// pyFormatFloat to unwrap it again. Non-literal expressions emit
    /// normally — pyFormatFloat unwraps a boxed argument itself.
    fn emit_format_float_arg(&mut self, e: &Expr) {
        match &e.kind {
            ExprKind::FloatLiteral(n) => self.write(&format!("{}", n)),
            ExprKind::UnaryOp {
                op: UnaryOp::Neg,
                operand,
            } if matches!(&operand.kind, ExprKind::FloatLiteral(_)) => {
                if let ExprKind::FloatLiteral(n) = &operand.kind {
                    self.write(&format!("(-{})", n));
                }
            }
            _ => self.emit_expr(e),
        }
    }

    /// A float-typed operand in unboxed (native Number) position: a float
    /// LITERAL is emitted bare (boxing it just to unwrap it again would
    /// waste an allocation — the value is statically known); anything else
    /// unwraps through the value-boundary authority's `__reqNum` — a no-op
    /// on a native Number, `valueOf()` on a boxed float, and the exact
    /// int→float coercion on a BigInt (a large int flowing through a
    /// float-inferred binding made the old bare `(+x)` throw "Cannot
    /// convert a BigInt value to a number" — the #461 class).
    fn emit_float_operand_unboxed(&mut self, operand: &Expr) {
        if let ExprKind::FloatLiteral(n) = &operand.kind {
            self.write(&format!("({})", n));
        } else {
            self.need_runtime("__reqNum");
            self.write("__reqNum(");
            self.emit_expr(operand);
            self.write(")");
        }
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
                        self.emit_binop_bare_float(left, "+", right)
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
                if self.both_float(left, right) {
                    self.emit_binop_bare_float(left, "-", right);
                } else if self.int_arith_provably_safe(left, BinOp::Sub, right) {
                    self.emit_binop_bare(left, "-", right);
                } else {
                    self.emit_binop_helper("pySub", left, right);
                }
            }
            // PEP 465 `a @ b`: pure dunder dispatch (__matmul__/__rmatmul__)
            // — no builtin operand support, like CPython without numpy.
            BinOp::MatMul => self.emit_binop_helper("pyMatMul", left, right),
            BinOp::Mul => {
                if self.both_float(left, right) {
                    self.emit_binop_bare_float(left, "*", right);
                } else if self.int_arith_provably_safe(left, BinOp::Mul, right) {
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
                    self.emit_binop_bare_float(left, "**", right);
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
                self.need_runtime("pyContains");
                self.write("pyContains(");
                self.emit_expr(right);
                self.write(", ");
                self.emit_expr(left);
                self.write(")");
            }
            BinOp::NotIn => {
                self.need_runtime("pyContains");
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
                    // Option B: a comparison is VALUE-only — a float literal
                    // operand emits bare (boxing it just for `<`'s ToPrimitive
                    // to unwrap would allocate per evaluation, e.g. every
                    // loop iteration of `if x > 1e6:`). A boxed non-literal
                    // operand still compares correctly via valueOf.
                    self.write("(");
                    self.emit_format_float_arg(left);
                    self.write(&format!(" {} ", op_str));
                    self.emit_format_float_arg(right);
                    self.write(")");
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
                // Option-B spike: a boxed float never `===` anything — route
                // float-involved equality through pyEq (which unwraps).
                let float_side =
                    matches!(lt, JsInferredType::Float) || matches!(rt, JsInferredType::Float);
                if !(lt.is_scalar() && rt.is_scalar()) || bool_lit || float_side {
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
                let float_side =
                    matches!(lt, JsInferredType::Float) || matches!(rt, JsInferredType::Float);
                if !(lt.is_scalar() && rt.is_scalar()) || bool_lit || float_side {
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
                        let was_callee = self.in_call_callee;
                        self.in_call_callee = true;
                        self.emit_expr(func);
                        self.in_call_callee = was_callee;
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
                if matches!(self.infer_type(operand), JsInferredType::Float) {
                    // Option-B spike: bare `-` would unwrap the box via
                    // valueOf and lose the float tag — negate + re-box.
                    // Authority unwrap (__reqNum): bare `(+x)` threw on a
                    // BigInt leaking through a float-inferred binding.
                    self.need_runtime("__pyF");
                    self.need_runtime("__reqNum");
                    self.write("__pyF(-__reqNum(");
                    self.emit_expr(operand);
                    self.write("))");
                } else if matches!(self.infer_type(operand), JsInferredType::Primitive) {
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
                if matches!(self.infer_type(operand), JsInferredType::Float) {
                    // Option-B spike: keep the box through unary plus.
                    // Authority unwrap (__reqNum) — see UnaryOp::Neg.
                    self.need_runtime("__pyF");
                    self.need_runtime("__reqNum");
                    self.write("__pyF(__reqNum(");
                    self.emit_expr(operand);
                    self.write("))");
                } else {
                    // Authority: `+int` is the identity at ANY magnitude —
                    // the old bare `(+x)` threw "Cannot convert a BigInt
                    // value to a number" on a large int (#38 class).
                    self.need_runtime("pyPos");
                    self.write("pyPos(");
                    self.emit_expr(operand);
                    self.write(")");
                }
            }
            UnaryOp::Not => {
                // #211: `not x` must use Python truthiness. For a scalar
                // (int/float/bool/str/None) JS `!` already matches Python, so
                // keep the bare fast path. For a collection or Unknown operand
                // an empty list/dict/set is FALSY in Python but TRUTHY in JS
                // (`![]` === false), so wrap in pyBool — same conservative
                // choice as `if x:` / `while x:`. This is why `if not strings:`
                // guards silently failed on empty inputs (HumanEval /5 /12).
                if matches!(self.infer_type(operand), JsInferredType::Primitive) {
                    // Option-B spike: Float excluded — a boxed 0.0 is a JS
                    // object (always truthy), so `not x` must use pyBool.
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

        // B4 → 0.2.2 member-call CLASS rule: a member call on a CORE-React
        // namespace alias (`import react [as R]` / `import react_dom [as D]` /
        // `import react_dom.client as C`). The star-namespace import binds the
        // camelCase exports, so a raw snake member is a silent `undefined`.
        // The original fix special-cased only `create_element` with ≥2 args —
        // validating its own shape and leaving every ADJACENT member
        // (`react.use_state`, `react.clone_element`, `react_dom.create_portal`,
        // single-arg `create_element`, …) silently dead. Now EVERY member is
        // routed through ONE rule (`react::route_namespace_member`): removed
        // check first, then camel-case + module check against the audited
        // table, or a compile diagnostic. `createElement` additionally gets the
        // factory props/kwargs transform, identical to the name-bound form.
        if let ExprKind::Attribute {
            value,
            attr,
            optional: attr_opt,
        } = &func.kind
        {
            if let ExprKind::Name(base) = &value.kind {
                if let Some(&src) = self.react_namespace_alias_modules.get(base) {
                    match react::route_namespace_member(src, attr) {
                        react::MemberRoute::Removed(msg) => {
                            self.emit_expr_error(msg);
                            return;
                        }
                        react::MemberRoute::WrongModule {
                            js_name,
                            exports_from,
                        } => {
                            self.emit_expr_error(&format!(
                                "`{base}.{attr}` — `{js_name}` is exported by \
                                 \"{}\", not \"{}\": the member access would be \
                                 `undefined` at runtime. Import it from the right \
                                 module, or use `from pyths.react import {attr}` \
                                 (auto-routes to the correct package).",
                                exports_from.module(),
                                src.module(),
                            ));
                            return;
                        }
                        react::MemberRoute::Routed(js_name) => {
                            if js_name == "createElement"
                                && !kwargs.is_empty()
                                && self.react_factory_kwargs_misuse(args, kwargs)
                            {
                                return; // diagnostic already emitted
                            }
                            self.emit_expr(value);
                            self.write(if *attr_opt { "?." } else { "." });
                            self.write(&js_name);
                            self.write(open_paren);
                            if js_name == "createElement" {
                                self.emit_react_factory_args(args, kwargs);
                            } else {
                                self.emit_call_args(args, kwargs);
                            }
                            self.write(")");
                            return;
                        }
                    }
                }
                // Broader react-ecosystem namespace aliases (react_router_dom,
                // framer_motion, …): no export table to check against, but the
                // member transform must still MATCH the from-import path
                // (snake→camel) — `rrd.create_browser_router` binds
                // `createBrowserRouter`, exactly what `from react_router_dom
                // import create_browser_router` emits. Identity on
                // underscore-free names, so camelCase spellings pass through.
                if self.react_lib_module_aliases.contains(base) && attr.contains('_') {
                    self.emit_expr(value);
                    self.write(if *attr_opt { "?." } else { "." });
                    self.write(&react::snake_to_camel(attr));
                    self.write(open_paren);
                    self.emit_call_args(args, kwargs);
                    self.write(")");
                    return;
                }
            }
        }

        // #448 (member form): `importlib.import_module(...)` — a member call on
        // a tracked `import importlib` namespace. importlib is not a real module
        // here (the namespace emits nothing), so this used to lower to a broken
        // `importlib.import_module(...)` with no diagnostic. Steer to the
        // supported forms.
        if let ExprKind::Attribute {
            value,
            attr,
            optional: false,
        } = &func.kind
        {
            if attr == "import_module" {
                if let ExprKind::Name(base) = &value.kind {
                    if self.importlib_namespaces.contains(base)
                        && !self.is_declared_in_any_scope(base)
                    {
                        self.emit_expr_error(
                            "`importlib.import_module(...)` (member form) is not supported. \
                             Use `from importlib import import_module` then call \
                             `import_module(\"./mod.js\")`, or the bare `import_module(...)` \
                             builtin — both lower to native dynamic `import()`.",
                        );
                        return;
                    }
                }
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
            // autotester builtin_super: the EXPLICIT two-arg form
            // `super(C, obj)` maps exactly onto the same MRO helper —
            // dispatch to the class after C in obj's MRO. Previously fell
            // through to the reserved-word rename (`super$(...)` →
            // ReferenceError).
            if n == "super" && args.len() == 2 && kwargs.is_empty() {
                self.need_runtime("__pySuper");
                self.write("__pySuper(");
                self.emit_expr(&args[0]);
                self.write(", ");
                self.emit_expr(&args[1]);
                self.write(")");
                return;
            }
        }

        // TB-1: a DIRECT call to React's createElement factory
        // (`h("button", {"on_click": f}, "-")` where `h`/`create_element`/
        // `createElement` is bound to it). The 2nd positional argument is the
        // props object — PSX-prop position — so a dict-literal there gets the
        // snake→camel/kebab prop-name transform (on_click→onClick,
        // aria_label→aria-label). This is the ONLY dict-literal position the
        // transform reaches; general dict literals stay verbatim.
        //
        // 0.2.2 kwargs class fix: the KEYWORD form (`h("div", on_click=f)`,
        // PSX-flat-style) used to fall through to generic call emission —
        // kwargs became a VERBATIM trailing object (`{on_click: f}`, a dead
        // handler) landing in whatever argument slot came next. Now kwargs are
        // the props (static keys transformed via the same single
        // `write_react_prop_key` rule, `**spread` verbatim — the genuine TB-1
        // dynamic boundary) and positionals after the tag are children, exactly
        // like flat PSX. Ambiguous/malformed kwarg forms are diagnosed.
        if !optional {
            if let ExprKind::Name(name) = &func.kind {
                if self.react_create_element_fns.contains(name)
                    && (args.len() >= 2 || !kwargs.is_empty())
                {
                    if !kwargs.is_empty() && self.react_factory_kwargs_misuse(args, kwargs) {
                        return; // diagnostic already emitted
                    }
                    let callee = self.resolve_name_ref(name);
                    self.write(&callee);
                    self.write("(");
                    self.emit_react_factory_args(args, kwargs);
                    self.write(")");
                    return;
                }
            }
        }

        // NB-1: an UNBOUND HTML/SVG intrinsic-tag name (`div(...)`, `pre(...)`)
        // used as a CALL OUTSIDE any @psx/@component compiled to a bare
        // `div(...)` reference — `pyths check` passed clean, then a runtime
        // ReferenceError. #306's is_unbound_psx_tag only RESCUES the same name
        // INSIDE a component (→ createElement); OUTSIDE, there is no element
        // context, so this is a hard compile diagnostic instead of a silent
        // miscompile. Gated on is_unbound_psx_tag, so a legitimately-BOUND user
        // symbol named `div` (import/def/local) used outside a component stays a
        // valid call, and Python builtins / JS globals that happen to be tag
        // names (`map`/`input`/`object`) keep their normal lowering.
        if !self.in_component {
            if let ExprKind::Name(name) = &func.kind {
                if react::is_html_element(name) && self.is_unbound_psx_tag(name) {
                    self.emit_expr_error(&format!(
                        "`{name}` is an intrinsic HTML/SVG element tag, only available inside \
                         @component/@psx — decorate this function with @psx, or `{name}` is \
                         undefined here"
                    ));
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
                    // NB-2: a user BINDING (module-level `def`/`class`, a
                    // local/param, or an import) whose name is a lowercase HTML
                    // intrinsic tag is SILENTLY shadowed by the intrinsic here —
                    // the allowlist check ignores bindings, so `div(...)` always
                    // lowers to createElement("div") and the user's `div` is
                    // unreachable, with no error. Intrinsic-wins is correct and
                    // React-consistent, so we KEEP the lowering — but make the
                    // shadow LOUD: a hard compile diagnostic. Fires ONLY for an
                    // allowlist tag with a real user binding; an unbound tag
                    // (the #306 rescue) and a bound NON-allowlist tag-shaped
                    // name are both untouched.
                    if react::is_html_element(name) && self.has_user_binding(name) {
                        self.record_codegen_error(&format!(
                            "`{name}` collides with the HTML intrinsic element tag inside \
                             @component/@psx; your binding of `{name}` is shadowed — rename it \
                             (or use a Capitalized component name)"
                        ));
                    }
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
            // DX-B1: a binding named `len` in any enclosing scope shadows the
            // builtin — do NOT take the `.length` fast path for it.
            if func_name == "len"
                && args.len() == 1
                && kwargs.is_empty()
                && !self.is_declared_in_any_scope("len")
            {
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
                self.emit_format_float_arg(&args[0]);
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
                        self.emit_format_float_arg(a);
                        self.write(")");
                    } else {
                        self.emit_expr(a);
                    }
                }
                self.write(")");
                return;
            }
        }

        // #225 (folded into the public-#3 unified gate below): `eval`/`exec`/
        // `compile` are intentionally unsupported — they now flow through the
        // same unimplemented-builtin diagnostic as `open`/`input`/`hash`/...,
        // which also means a USER binding named `eval` correctly shadows
        // (the old dedicated branch fired unconditionally).
        //
        // public #3: `vars()` with NO arguments is `locals()` — no compiled
        // equivalent exists. Reject at compile time; the 1-arg instance form
        // lowers through the pyVars mapping below. A user binding named
        // `vars` shadows and is untouched.
        if let ExprKind::Name(name) = &func.kind {
            if name == "vars"
                && args.is_empty()
                && kwargs.is_empty()
                && !self.is_declared_in_any_scope(name)
            {
                self.emit_expr_error(
                    "vars() with no arguments is locals(), which is not \
                     supported yet (pythscribe-v3.x)",
                );
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
            // Star-import call form: a bound export CALLED bare resolves to
            // the module's function/class (classes construct with `new`) —
            // and suppresses the builtin lowering below (`from math import *`
            // makes `pow(3, 4.5)` math.pow, like CPython's rebinding).
            // #448: a name bound via `from importlib import import_module [as X]`
            // lowers to native dynamic `import(spec)` (unless shadowed by a
            // local). The bare `import_module` builtin is handled below via
            // builtin_func_mapping; this covers the aliased form.
            if !self.is_declared_in_any_scope(name) && self.import_module_fns.contains(name) {
                self.emit_import_module_call(args, kwargs);
                return;
            }
            if !self.is_declared_in_any_scope(name) {
                if let Some((ns, is_class)) = self.star_import_bindings.get(name).cloned() {
                    if is_class {
                        self.write("new ");
                    }
                    self.write(&format!("{}.{}", ns, name));
                    self.write(open_paren);
                    self.emit_call_args(args, kwargs);
                    self.write(")");
                    return;
                }
            }
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
                    BuiltinMapping::NativeCall(kw) => {
                        // #448: native language call form (e.g. dynamic
                        // `import(spec)`). No runtime helper. The `?.()`
                        // optional-call form is meaningless for a keyword head,
                        // so always use a plain `(`.
                        if name == "import_module" {
                            self.emit_import_module_call(args, kwargs);
                            return;
                        }
                        self.write(kw);
                        self.write("(");
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
                                    | "bool" | "dict" | "set"
                                    | "bytes" | "bytearray"))
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
                // WF-1 root fix — the cleanup-effect hooks (useEffect /
                // useLayoutEffect / useInsertionEffect) store the callback's
                // return as the effect cleanup and invoke it. React accepts
                // only `undefined` or a function there; a Python effect ending
                // in `return None` emits `return null`, which React calls →
                // "destroy is not a function". Wrap the callback in
                // `__pyEffect`, which coerces ANY non-function return (null /
                // None / a number / …) to `undefined`. This is more general
                // than a codegen `return None` rewrite: it neutralizes every
                // null/non-function-returning effect body regardless of shape.
                // Only the FIRST arg (the effect fn) is wrapped; the deps array
                // is passed through. kwargs never apply to these hooks.
                if react::is_cleanup_effect_hook(name) && !args.is_empty() && kwargs.is_empty() {
                    // WF-1 round 2 (spread form): `use_effect(*args)` — the
                    // compile-time wrap can't reach inside a spread; the old
                    // emission wrapped the WHOLE spread (`useEffect(
                    // __pyEffect(...args))`), swallowing the deps array so
                    // the effect re-ran every render. Route through the
                    // runtime splitter, which wraps ONLY the resolved first
                    // argument: `useEffect(...__pyEffectArgs(...args))`.
                    // Applies whenever ANY positional arg is a spread (the
                    // first slot's identity is unknowable at compile time).
                    if args
                        .iter()
                        .any(|a| matches!(a.kind, ExprKind::Starred(_)))
                    {
                        self.need_runtime("__pyEffectArgs");
                        self.write(js_name);
                        self.write(open_paren);
                        self.write("...__pyEffectArgs(");
                        self.emit_call_args(args, kwargs);
                        self.write("))");
                        return;
                    }
                    self.need_runtime("__pyEffect");
                    self.write(js_name);
                    self.write(open_paren);
                    self.write("__pyEffect(");
                    self.emit_expr(&args[0]);
                    self.write(")");
                    for a in &args[1..] {
                        self.write(", ");
                        self.emit_expr(a);
                    }
                    self.write(")");
                    return;
                }
                self.write(js_name);
                self.write(open_paren);
                self.emit_call_args(args, kwargs);
                self.write(")");
                return;
            }

            // public #3 DEEP FIX — the unimplemented-builtin gate. At this
            // point `name` is called bare and is NOT a local/param/enclosing
            // binding, NOT a star-import binding, NOT a runtime-mapped
            // builtin, and NOT a react hook. If it is a KNOWN CPython builtin
            // with no implementation (open/input/eval/hash/id/...), emitting
            // it verbatim would reproduce the silent compile-then-
            // ReferenceError class of public issue #3 — emit a compile error
            // that fails `pyths compile` AND `pyths check` instead. No false
            // positives: user bindings/imports are caught by
            // is_declared_in_any_scope, forward-referenced top-level defs by
            // known_functions/known_classes (module-wide pre-scan),
            // star-import rebinds by star_import_bindings, implemented
            // builtins matched the mapping tables above, and inside a
            // @component the HTML-element collisions (input/map/object) were
            // already claimed by the PSX element dispatch.
            if !self.is_declared_in_any_scope(name)
                && !self.star_import_bindings.contains_key(name)
                && !self.known_functions.contains(name)
                && !self.known_classes.contains(name)
                && crate::builtins::unsupported_builtin(name)
            {
                let diag = crate::builtins::unsupported_builtin_message(name);
                self.emit_expr_error(&diag);
                return;
            }
        }

        // WB-17: the ES primitive-wrapper globals (Boolean/Number/String/
        // Symbol/BigInt) are TYPE-CONVERSION functions when called BARE, but
        // the capitalized-name → `new` heuristic below would box them into
        // truthy wrapper OBJECTS (`new Boolean("")` is truthy, not `false`;
        // `typeof new Number("3")` is `"object"`) — and `new Symbol()`/
        // `new BigInt()` THROW. Emit a BARE call so they coerce to primitives.
        // Guarded on not-shadowed: a user `class String` (known_classes) or any
        // local/param binding keeps normal handling. Python's `bool()/int()/
        // str()` are lowercase and already routed through runtime helpers
        // (pyBool/pyInt/pyStr) above, so they are unaffected.
        if let ExprKind::Name(name) = &func.kind {
            if react::is_js_primitive_wrapper(name)
                && !self.known_classes.contains(name)
                && !self.is_declared_in_any_scope(name)
            {
                self.write(&format!("{}{}", name, open_paren));
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
                // Option B: a JS BUILT-IN constructor (never a compiled
                // Python class — those are in known_classes, checked first)
                // is a native sink: unbox float args (`Array(3.0)` must get
                // a native 3, or the typeof-dispatch builds `[box]` of
                // length 1 instead of a 3-slot array).
                if !self.known_classes.contains(name)
                    && is_js_builtin_ctor(name)
                    && kwargs.is_empty()
                {
                    self.write(&format!("new {}{}", Self::sanitize_ident(name), open_paren));
                    for (i, arg) in args.iter().enumerate() {
                        if i > 0 {
                            self.write(", ");
                        }
                        self.emit_native_sink_value(arg, true);
                    }
                    self.write(")");
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
                        let js_cls = Self::sanitize_ident(cls_name).into_owned();
                        if kwargs.is_empty() {
                            self.write(&format!(
                                "__pyClassCall({}, \"{}\", [",
                                js_cls, attr
                            ));
                            self.emit_call_args(args, kwargs);
                            self.write("])");
                        } else {
                            // Keyword binding for unbound Cls.method(...)
                            // calls: __pyClassCallKw consults the resolved
                            // method's __pyparams__/__pykw__ metadata and
                            // offsets the leading positional self for
                            // prototype methods (autotester arguments:
                            // A.__init__(self, y, x, *args, m=n, ...)).
                            self.need_runtime("__pyClassCallKw");
                            self.write(&format!(
                                "__pyClassCallKw({}, \"{}\", [",
                                js_cls, attr
                            ));
                            for (i, a) in args.iter().enumerate() {
                                if i > 0 {
                                    self.write(", ");
                                }
                                self.emit_expr(a);
                            }
                            self.write("], ");
                            self.emit_kwargs_value(kwargs);
                            self.write(")");
                        }
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
            let dt_class_call = matches!(&value.kind, ExprKind::Name(n)
                    if self.datetime_namespaces.contains(n))
                && matches!(
                    attr.as_str(),
                    "datetime" | "date" | "time" | "timedelta" | "timezone"
                );
            if (matches!(&value.kind, ExprKind::Name(n) if self.module_namespaces.contains(n))
                && attr.chars().next().is_some_and(|c| c.is_uppercase()))
                || dt_class_call
            {
                self.write("new ");
                self.emit_expr(value);
                self.write(&format!(".{}(", attr));
                self.emit_call_args(args, kwargs);
                self.write(")");
                return;
            }
        }

        if let ExprKind::Attribute {
            value,
            attr,
            optional: attr_opt,
        } = &func.kind
        {
            // #221: a call on a stdlib module namespace (`re.split`, `os.count`)
            // is a module function, not the string/list method with the same
            // name — skip the lowering table and emit it verbatim.
            let is_module_call =
                matches!(&value.kind, ExprKind::Name(n) if self.module_namespaces.contains(n));
            if !is_module_call {
                if let Some(lowering) = method_lowering(attr) {
                    // WB-9 CLASS rule: the ATTRIBUTE-level `?.` (`l?.remove(1)`,
                    // `s?.upper()`) was DISCARDED here — only the call-level
                    // `optional` (`l.remove?.(1)`) flowed through, so helper
                    // lowerings ran on a None receiver and inline/rename
                    // lowerings dropped the short-circuit. Fold both flags into
                    // ONE `optional` that every lowering path (Rename / Inline /
                    // Hybrid / Runtime) must honor uniformly.
                    let opt = optional || *attr_opt;
                    // F2 root fix: receiver-context + arity dispatch for
                    // collision-prone container methods. When the receiver is
                    // proven foreign, or the positional arity is impossible for
                    // the Python container method (so it cannot BE that method),
                    // skip the container lowering — which would silently drop or
                    // corrupt arguments — and fall through to verbatim emission
                    // that preserves every argument.
                    if !self.container_dispatch_prefers_verbatim(value, attr, args)
                        && self
                            .try_emit_method_lowering(value, attr, args, kwargs, lowering, opt)
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
        if !kwargs.is_empty()
            && !optional
            && matches!(&func.kind, ExprKind::Name(_) | ExprKind::Lambda { .. })
        {
            // Lambda IIFEs too (autotester arguments): the __pyFnMeta wrapper
            // carries the lambda's __pyparams__, so keyword binding works.
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

        // autotester local_classes: an attribute call whose ATTRIBUTE NAME is
        // a known class (`a.B(9)` — a nested class reached through an
        // instance) needs `new` at runtime; route through __pyAttrCall, which
        // class-detects the attribute value and otherwise applies it with the
        // receiver. Gated on the attr being a known class name so ordinary
        // method calls stay on the raw fast path.
        if let ExprKind::Attribute {
            value,
            attr,
            optional: false,
        } = &func.kind
        {
            if kwargs.is_empty()
                && self.known_classes.contains(attr)
                && !matches!(&value.kind, ExprKind::Name(n) if self.known_classes.contains(n)
                    || self.local_module_imports.contains(n)
                    || self.module_namespaces.contains(n)
                    || self.asyncio_namespaces.contains(n))
            {
                self.need_runtime("__pyAttrCall");
                self.write("__pyAttrCall(");
                self.emit_expr(value);
                self.write(&format!(", \"{}\", [", attr));
                for (i, a) in args.iter().enumerate() {
                    if i > 0 {
                        self.write(", ");
                    }
                    self.emit_expr(a);
                }
                self.write("])");
                return;
            }
        }

        // autotester callable_test: a call through a plain VARIABLE may hit
        // an instance of a class defining __call__ (CPython callable
        // objects). Route Name callees that are LOCAL VARIABLES (declared,
        // but not a known def/class) through __pyCall — real functions take
        // its one-typeof fast path; direct calls to known defs stay raw.
        // Gated on the module DEFINING a __call__ method somewhere: a
        // callable-free module (the overwhelmingly common case) keeps raw
        // calls with zero overhead, and a cross-module callable instance
        // fails exactly as loudly as before (TypeError), never silently.
        // #472: a callee whose STATIC type is provably non-callable (a dict/
        // list/set/tuple/str/int/float local — `d = {"a": 1}; d()`) used to
        // inline a raw `d(...)` and leak the native JS "d is not a function"
        // TypeError. Route it through the same __pyCall guard, which raises
        // CPython's "'dict' object is not callable" via __pyTypeName. The
        // wrap is semantics-preserving even if inference is stale (a real
        // function/class takes __pyCall's fast path), and Unknown-typed
        // callees keep the zero-overhead raw call unless the module defines
        // a __call__ somewhere (the original gate).
        let statically_non_callable = matches!(
            self.infer_type(func),
            JsInferredType::List
                | JsInferredType::Dict
                | JsInferredType::Set
                | JsInferredType::Tuple
                | JsInferredType::Primitive
                | JsInferredType::Float
        );
        if let ExprKind::Name(name) = &func.kind {
            if (self.module_has_dunder_call || statically_non_callable)
                && self.is_declared_in_any_scope(name)
                && !self.known_functions.contains(name)
                && !self.known_classes.contains(name)
            {
                self.need_runtime("__pyCall");
                self.write("__pyCall(");
                self.emit_expr(func);
                self.write(", [");
                self.emit_call_args(args, kwargs);
                self.write("])");
                return;
            }
        } else if statically_non_callable && !optional {
            // Error-kind round 3 (corpus deep-close): the same guard for
            // EXPRESSION callees — `(5)()`, `([1])()`, `({'a': 1})()`,
            // `d["k"]()` when the subscript's static type is known — which
            // otherwise emit a raw JS call and leak the native
            // "... is not a function" TypeError instead of CPython's
            // "'int' object is not callable". Unknown-typed callees keep the
            // zero-overhead raw call (documented limitation: an Unknown
            // non-callable still leaks the JS error); provably-callable
            // paths (defs, lambdas, attribute methods) never infer as a
            // container/primitive, so the fast path is untouched.
            self.need_runtime("__pyCall");
            self.write("__pyCall(");
            self.emit_expr(func);
            self.write(", [");
            self.emit_call_args(args, kwargs);
            self.write("])");
            return;
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
        let was_callee = self.in_call_callee;
        self.in_call_callee = true;
        self.emit_expr(func);
        self.in_call_callee = was_callee;
        if needs_parens {
            self.write(")");
        }
        self.write(open_paren);
        self.emit_call_args(args, kwargs);
        self.write(")");
    }

    /// F2 root fix — receiver-context + arity dispatch for the
    /// collision-prone container methods (`append`, `extend`, `insert`,
    /// `remove`, `pop`, `get`, `keys`/`values`/`items`, …). Returns `true`
    /// when the call MUST be emitted verbatim (skip the container lowering)
    /// because the receiver cannot be the Python container this method
    /// belongs to.
    ///
    /// The rule is UNIFORM across every arity-gated method (one table in
    /// `method_table::container_method_arity`), not per-method patches:
    ///
    /// 1. Receiver **provably the matching container** (List/Dict/Set/Tuple)
    ///    → keep the container lowering (current behavior; correct + fast).
    /// 2. Positional **arity impossible** for the Python container method
    ///    → verbatim. This is the load-bearing backstop: `FormData().append("k", v)`
    ///    (2 args) can never be 1-arg `list.append`, so lowering it as a list
    ///    op silently drops `v`. Fires even when type inference can't identify
    ///    the receiver.
    /// 3. Receiver **provably foreign** (a JS/DOM global constructor result:
    ///    `FormData()`, `Headers()`, `URLSearchParams()`, `Map()`, …) with a
    ///    valid arity → verbatim (the foreign method wins the name collision).
    /// 4. Otherwise (unknown receiver, valid arity) → keep the container
    ///    lowering (backward-compatible for untyped lists; the runtime helper
    ///    dispatches on receiver shape).
    fn container_dispatch_prefers_verbatim(
        &self,
        receiver: &Expr,
        attr: &str,
        args: &[Expr],
    ) -> bool {
        let argc = args.len();
        // WB-3 root fix — `list.sort` is KEYWORD-ONLY in Python:
        // `sort(*, key=None, reverse=False)`. It takes NO positional
        // arguments, so `xs.sort(anything_positional)` can never be Python
        // `list.sort` (`[].sort(f)` is a `TypeError` in CPython). A positional
        // arg therefore PROVES a JS Array comparator (functools.cmp_to_key
        // semantics: `cmp(a,b) < 0 → a before b`). Emit it verbatim —
        // `recv.sort(cmp)` is `Array.prototype.sort(compareFn)`, and Python
        // lists are backed by JS arrays, so the ordering is correct for lists;
        // it is equally correct for a user/foreign receiver with its own
        // `.sort(cmp)`. This overrides the provable-container short-circuit
        // below (a real list WITH a positional arg is still non-Python), and
        // it depends only on the positional-arity SIGNAL — no receiver-type
        // inference, hence no variable-flow residual. The keyword forms
        // (`key=`, `reverse=`) and the no-arg `xs.sort()` carry zero
        // positional args and keep the `pyListSort` lowering unchanged.
        if attr == "sort" && argc >= 1 {
            return true;
        }
        // WB-10 root fix — `str.replace` is an ARG-TYPE-discriminated collision
        // (same family as WB-3's positional-arity signal). Python `str.replace`
        // has signature `(old: str, new: str[, count: int])` — it takes ONLY a
        // string pattern and a string replacement. So a call whose **first arg
        // is a regex** (`RegExp(...)`) or whose **second arg is a function**
        // (lambda / a known `def`) can NEVER be Python `str.replace` — in
        // CPython both raise `TypeError`. Such a call is JS
        // `String.prototype.replace` (regex + capture groups / `$1` backrefs /
        // function replacer), so emit it verbatim; `pyStrReplace` would drop the
        // groups and stringify a function replacer. A plain `s.replace(str, str)`
        // carries neither signal and keeps the `pyStrReplace` lowering (Python
        // replace-all semantics — unchanged). Like the `sort` case, this rides
        // only on the ARGUMENT shape, so it fires regardless of receiver type
        // (a real Python `str` with a regex arg is still non-Python) and needs
        // no variable-flow inference.
        if attr == "replace" && self.replace_args_are_js_only(args) {
            return true;
        }
        let Some(rule) = container_method_arity(attr) else {
            return false;
        };
        // (1) Provably the container this method belongs to → keep lowering.
        let rt = self.infer_type(receiver);
        if Self::infer_matches_container(rt, rule.containers) {
            return false;
        }
        // (2) Arity backstop (mandatory): an arg count impossible for the
        //     Python container method proves the receiver is not it.
        if !rule.accepts(argc) {
            return true;
        }
        // (3) Provably foreign receiver, valid arity → verbatim.
        self.is_foreign_receiver(receiver)
    }

    /// WB-10 — do the arguments of a `.replace(...)` call carry a signal that
    /// PROVES it is JS `String.prototype.replace`, not Python `str.replace`?
    ///
    /// Python `str.replace(old, new[, count])` accepts only string `old`/`new`.
    /// Two argument shapes are impossible for it (both `TypeError` in CPython),
    /// so their presence proves the call is the JS regex/callback form:
    ///   1. the **first arg is a regex** — a `RegExp(...)` constructor call, or
    ///   2. the **second arg is a function** — a lambda literal, or a bare name
    ///      that resolves to a known `def`.
    ///
    /// Emitted verbatim, these become real `String.prototype.replace` calls that
    /// honor capture groups / `$1` backrefs / function replacers.
    fn replace_args_are_js_only(&self, args: &[Expr]) -> bool {
        // (1) regex first arg: `RegExp(...)` (compiles to `new RegExp(...)`).
        let regex_first = args.first().is_some_and(|a| {
            matches!(&a.kind, ExprKind::Call { func, .. }
                if matches!(&func.kind, ExprKind::Name(n) if n == "RegExp"))
        });
        // (2) function second arg: a lambda, or a name that is a known `def`.
        let fn_second = args.get(1).is_some_and(|a| match &a.kind {
            ExprKind::Lambda { .. } => true,
            ExprKind::Name(n) => self.known_functions.contains(n),
            _ => false,
        });
        regex_first || fn_second
    }

    /// Does the inferred receiver type match any of the container kinds the
    /// method belongs to? (List↔List, Dict↔Dict, Set↔Set, Tuple↔Tuple.)
    fn infer_matches_container(rt: JsInferredType, kinds: &[ReceiverKind]) -> bool {
        kinds.iter().any(|k| {
            matches!(
                (k, rt),
                (ReceiverKind::List, JsInferredType::List)
                    | (ReceiverKind::Dict, JsInferredType::Dict)
                    | (ReceiverKind::Set, JsInferredType::Set)
                    | (ReceiverKind::Tuple, JsInferredType::Tuple)
            )
        })
    }

    /// Is the receiver PROVABLY a foreign (non-Python) object — so a
    /// colliding container-method name is really the foreign method and must
    /// be emitted verbatim? Conservative and sound: only a direct call to a
    /// recognized JS/DOM global constructor (`FormData()`, `Headers()`,
    /// `URLSearchParams()`, `Map()`, `URL()`, …; see
    /// `react::is_builtin_constructor`) qualifies. A user `class` shadowing
    /// such a name is a real class instance, handled elsewhere — excluded.
    fn is_foreign_receiver(&self, receiver: &Expr) -> bool {
        match &receiver.kind {
            ExprKind::Call { func, .. } => matches!(
                &func.kind,
                ExprKind::Name(n)
                    if react::is_builtin_constructor(n)
                        && !self.known_classes.contains(n)
            ),
            _ => false,
        }
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
                // WB-9: an optional-chained receiver must short-circuit the
                // WHOLE inline form (`s?.strip()` → undefined when s is
                // None), exactly like emit_runtime_method's guard. The spec
                // is emitted against a temp binding inside the same guard
                // arrow; emission is buffered first so an arity-rejecting
                // spec can still fall back cleanly to verbatim.
                if optional || Self::receiver_may_short_circuit(receiver) {
                    let n = self.default_hoist_counter;
                    self.default_hoist_counter += 1;
                    let t = format!("__optrecv{}", n);
                    let tmp_recv = Expr {
                        kind: ExprKind::Name(t.clone()),
                        span: receiver.span,
                    };
                    let saved = std::mem::take(&mut self.output);
                    let ok = self.emit_inline_spec(&tmp_recv, args, spec);
                    let inner = std::mem::replace(&mut self.output, saved);
                    if !ok {
                        return false;
                    }
                    self.write(&format!("(({t}) => {t} == null ? undefined : "));
                    self.write(&inner);
                    self.write(")(");
                    self.emit_expr(receiver);
                    self.write(")");
                    return true;
                }
                self.emit_inline_spec(receiver, args, spec)
            }
            MethodLowering::Hybrid { inline, runtime } => {
                // WB-9: with an optional-chained receiver, skip the inline
                // form — the runtime path below carries the uniform
                // null-guard (emit_runtime_method).
                if !optional
                    && is_simple_receiver(receiver)
                    && self.hybrid_inline_applies(inline, receiver)
                    && self.emit_inline_spec(receiver, args, inline)
                {
                    return true;
                }
                // Complex receiver, type-inapplicable inline, OR inline form
                // rejected the args (e.g., wrong arity) — delegate to the
                // runtime helper.
                self.emit_runtime_method(runtime, receiver, args, kwargs, optional)
            }
            MethodLowering::Runtime { helper, .. } => {
                self.emit_runtime_method(helper, receiver, args, kwargs, optional)
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
    ///
    /// WB-9 root fix: when the method is reached via optional chaining, the
    /// helper must NOT be invoked on a null/undefined receiver. Native
    /// method calls (`el?.classList.add(x)`) short-circuit to `undefined`;
    /// but a helper lowering (`el?.classList.remove(x)` → `pyRemove(...)`)
    /// only guards the *receiver argument* — `pyRemove(el?.classList, x)`
    /// still calls `pyRemove(undefined, x)` unconditionally and throws.
    /// This is the ONE place every container-method shim (pyRemove/pyAppend/
    /// pyPop/pyIndex/pyUpdate/…) is emitted for the Runtime + Hybrid-runtime
    /// paths, so guarding here fixes the whole class in one site. The guard
    /// temp-binds the emitted receiver once (via an arrow IIFE — strict-mode
    /// safe, unlike a bare `(__t = …)`) and short-circuits the whole call:
    /// `((__t) => __t == null ? undefined : H(__t, args))(<recv>)`.
    ///
    /// `optional` covers a method-level `?.` (`el?.remove(x)`); the spine
    /// check covers a receiver that itself short-circuits (`el?.classList`,
    /// where the `.remove` call node carries `optional: false`).
    fn emit_runtime_method(
        &mut self,
        helper: &str,
        receiver: &Expr,
        args: &[Expr],
        kwargs: &[Keyword],
        optional: bool,
    ) -> bool {
        self.need_runtime(helper);
        if optional || Self::receiver_may_short_circuit(receiver) {
            let n = self.default_hoist_counter;
            self.default_hoist_counter += 1;
            let t = format!("__optrecv{}", n);
            self.write(&format!("(({t}) => {t} == null ? undefined : {helper}({t}"));
            if !args.is_empty() || !kwargs.is_empty() {
                self.write(", ");
            }
            self.emit_call_args(args, kwargs);
            self.write("))(");
            self.emit_expr(receiver);
            self.write(")");
            return true;
        }
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

    /// True iff `expr`, when emitted, may evaluate to `undefined` via an
    /// optional-chaining (`?.`) short-circuit somewhere in its member/call
    /// spine. JS propagates a `?.` short-circuit to the end of the chain, so
    /// a helper-lowered method whose receiver is such a chain
    /// (`el?.classList.remove(x)`) must be guarded rather than called on the
    /// short-circuited `undefined`. Only the spine (member/subscript/call
    /// left-hand side) is inspected — arguments (`foo(a?.b).remove(x)`) do
    /// not make the *receiver* short-circuit, since `foo` runs regardless.
    fn receiver_may_short_circuit(expr: &Expr) -> bool {
        match &expr.kind {
            ExprKind::Attribute { value, optional, .. }
            | ExprKind::Subscript { value, optional, .. } => {
                *optional || Self::receiver_may_short_circuit(value)
            }
            ExprKind::Call { func, optional, .. } => {
                *optional || Self::receiver_may_short_circuit(func)
            }
            _ => false,
        }
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

    /// R6: render an object-literal KEY for a kwarg/prop name proto-safely.
    /// The literal `__proto__: v` syntax invokes the inherited prototype setter
    /// (reparenting the object before any helper can copy the key); a COMPUTED
    /// key `["__proto__"]: v` creates a real own data property instead. Every
    /// object-literal that is built from a source-controlled name must route
    /// its key through here.
    fn obj_key(name: &str) -> String {
        if name == "__proto__" {
            "[\"__proto__\"]".to_string()
        } else {
            name.to_string()
        }
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
                    self.write(&format!("{}: ", Self::obj_key(name)));
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
                    self.write(&format!("{}: ", Self::obj_key(name)));
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
                    self.write(&format!("{}: ", Self::obj_key(name)));
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
                    if js_prop == "__proto__" {
                        // R6: even a QUOTED `"__proto__":` key triggers the
                        // prototype setter in an object literal — only a
                        // computed key creates a real own prop.
                        self.write("[\"__proto__\"]: ");
                    } else if is_valid_js_identifier(&js_prop) {
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
                        // Item 4 (0.2.2 hold): ONE style-value rule shared
                        // with the createElement-factory paths.
                        self.emit_react_style_value(&kw.value);
                        continue;
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
                    // Option B: EVERY prop value on an HTML/library tag is a
                    // native React sink — float-typed AND Unknown-typed
                    // values unwrap through __pyJs (uniform with style and
                    // JSX children; a non-box passes through untouched, so
                    // handlers/strings/objects are unaffected). A USER
                    // @component (convert_props == false) is Python land:
                    // its float props keep the box for fidelity.
                    if convert_props {
                        self.emit_native_sink_value(&kw.value, true);
                    } else {
                        self.emit_expr(&kw.value);
                    }
                } else {
                    // **kwargs spread
                    self.write("...");
                    self.emit_expr(&kw.value);
                }
            }
            self.write("}");
        }

        // Children from positional args — every child is a native React
        // sink (Option B: boxed floats unbox via emit_jsx_child).
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
                    self.emit_jsx_child(arg);
                }
                _ => {
                    self.emit_jsx_child(arg);
                }
            }
        }

        self.write(")");
    }

    /// Emit the VALUE of a `style` prop — ONE rule for EVERY surface that
    /// carries React props (PSX kwargs on HTML/library tags, the
    /// createElement-factory keyword form, and the factory's 2nd-positional
    /// props dict). Item 4 of the 0.2.2 hold: only the PSX kwargs path used
    /// to apply it, so `create_element("div", {"style": {"font_size": 12}})`
    /// kept `font_size` and React silently dropped the property.
    ///
    ///   - Dict literal → snake→camel each CSS key at compile time
    ///     (`emit_style_dict`; free, readable JS).
    ///   - An existing `pyNormalizeStyle(...)` call → pass through
    ///     (idempotent).
    ///   - Anything dynamic (variable, function call, …) → wrap in
    ///     `pyNormalizeStyle()` so the runtime converts the keys.
    fn emit_react_style_value(&mut self, value: &Expr) {
        match &value.kind {
            ExprKind::Dict { items } => self.emit_style_dict(items),
            ExprKind::Call { func, .. }
                if matches!(&func.kind, ExprKind::Name(n) if n == "pyNormalizeStyle") =>
            {
                self.emit_expr(value);
            }
            _ => {
                self.need_runtime("pyNormalizeStyle");
                self.write("pyNormalizeStyle(");
                self.emit_expr(value);
                self.write(")");
            }
        }
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
                    // Option B: style values feed React's px-append typeof
                    // check — a boxed 10.0 must arrive as a native 10.
                    self.emit_native_sink_value(value, true);
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
                    self.emit_jsx_child(child);
                }
                _ => {
                    self.emit_jsx_child(child);
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

    /// UNIFIED comprehension lowering for the COLLECTING forms (list / set /
    /// dict), modeled on CPython's desugaring: every comprehension form —
    /// list/set/dict/genexp, sync AND async — compiles to the SAME
    /// nested-loop scope function, differing ONLY in the accumulate op
    /// (`CompAccum`), the container init (applied by the caller: `new
    /// PySet(...)` / `new PyDict(...)` / `Object.fromEntries(...)`), and
    /// whether each level's iteration is awaited (`gen.is_async`, decided
    /// per level inside `emit_comp_loops`). Genexps share the SAME loop
    /// emitter via `emit_generator_exp` (accumulate op = `yield`).
    ///
    /// This is the class-level fix for the recurring "feature bolted onto
    /// some per-form emitters but not others" bug (#454: the dict emitter
    /// had no async arm; #463: only the genexp emitter needed eager
    /// iter-timing and drifted): there are no per-form emitters left to
    /// drift — a form CANNOT miss an arm because the arms live in exactly
    /// one place.
    ///
    /// Fast path: `[expr for x in xs if cond]` → `xs.filter(x => cond)
    /// .map(x => expr)` for the sync, single-generator, ≤1-condition,
    /// no-walrus case (dict comps map to `[k, v]` pairs). `.filter().map()`
    /// doesn't support async iteration, so ANY async level forces the loop
    /// path; a walrus anywhere forces it too (see expr_contains_walrus).
    fn emit_collect_comprehension(&mut self, accum: &CompAccum, generators: &[Comprehension]) {
        let any_async = generators.iter().any(|g| g.is_async);
        let has_walrus = accum.exprs().iter().any(|e| Self::expr_contains_walrus(e))
            || generators
                .iter()
                .any(|g| g.ifs.iter().any(Self::expr_contains_walrus));
        if !any_async && !has_walrus && generators.len() == 1 && generators[0].ifs.len() <= 1 {
            let gen = &generators[0];
            // Review edge: the leftmost iterable is evaluated in the ENCLOSING
            // scope (before the target binds), so emit it with the
            // comprehension's target scope temporarily lifted — `[list for list
            // in list([[1],[2]])]`'s receiver `list(...)` must lower to the
            // builtin, not resolve to the not-yet-bound target. The caller
            // pushed the (empty-but-for-targets) comprehension scope and nothing
            // has been emitted into it yet, so pop it, emit the iterable in the
            // enclosing scope, then re-push it for the `.filter`/`.map` arrows.
            self.pop_scope();
            self.emit_iterable_as_array(&gen.iter);
            self.push_scope(Self::comprehension_target_names(generators));
            // WB-15 (S5): iterable above stayed enclosing; target+cond+elt below
            // treat a `self` for-target as the ordinary comprehension variable.
            let cprev = self.enter_comp_self_shadow(generators);
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
            match accum {
                CompAccum::Element(e) => self.emit_expr(e),
                CompAccum::Pair(k, v) => {
                    self.write("[");
                    self.emit_expr(k);
                    self.write(", ");
                    self.emit_expr(v);
                    self.write("]");
                }
                CompAccum::Yield(_) => {
                    unreachable!("genexps lower through emit_generator_exp")
                }
            }
            self.write(")");
            self.self_lowering = cprev;
        } else {
            // Loop path — IIFE with the unified nested loops. ANY async
            // level requires `async` on the IIFE so `for await` works.
            // Round-4 sweep: the async IIFE returns a Promise — await it
            // in place when awaiting is legal here (async def body /
            // module top level), otherwise the caller gets the Promise
            // (pre-existing documented limit).
            let wrap_await = any_async && self.await_ok;
            if wrap_await {
                self.write("(await ");
            }
            // #453: mint guaranteed-fresh names for the IIFE's internal
            // temporaries — a user binding named `__comp_it`/`__result`
            // referenced in the element/conditions must keep resolving to
            // the USER name (see fresh_temp).
            let it = self.fresh_temp("__comp_it");
            let res = self.fresh_temp("__result");
            // B3: the OUTERMOST iterable is passed in as the IIFE arg
            // (emitted below in the ENCLOSING scope, after the shadow restores),
            // so a receiver-reading outer iterable keeps `this`. `idx == 0`
            // consumes the param instead of inlining `generators[0].iter`.
            if any_async {
                self.write(&format!("(async ({it}) => {{ const {res} = []; "));
            } else {
                self.write(&format!("(({it}) => {{ const {res} = []; "));
            }
            let cprev = self.enter_comp_self_shadow(generators);
            self.emit_comp_loops(accum, generators, 0, &it, Some(&res));
            self.self_lowering = cprev;
            self.write(&format!(" return {res}; }})("));
            // #452: the outermost iterable is evaluated in the ENCLOSING scope
            // — lift the comprehension's target scope (same as the fast path)
            // so `[x for list in list(xs) …]`'s `list(...)` lowers to the
            // builtin, never to the not-yet-bound target.
            self.pop_scope();
            self.emit_outer_comp_iterable(generators);
            self.push_scope(Self::comprehension_target_names(generators));
            self.write(")");
            if wrap_await {
                self.write(")");
            }
        }
    }

    /// #155: generator expressions compile to a lazy JS generator IIFE:
    ///
    ///   (function* (__gen_it) { for (const x of __gen_it) { if (c) { yield e; } } })
    ///       .call(this, __pyEagerIter(XS))
    ///
    /// Design notes:
    /// - #463: CPython acquires `iter(outermost)` when the genexp object is
    ///   CREATED — dis shows GET_ITER (GET_AITER for async) running before
    ///   the genexp function is even called — which is observable with a
    ///   side-effecting or throwing `__iter__`. `__pyEagerIter` /
    ///   `__pyEagerAIter` perform exactly that creation-time acquisition;
    ///   the generator body consumes the already-acquired iterator. Inner
    ///   iterables/conditions/element stay lazy (evaluated during
    ///   consumption), also matching CPython.
    /// - `.call(this, ...)` instead of a plain call: `self` inside method
    ///   bodies is rewritten to `this`, and a bare `function*` would
    ///   shadow it. At module top level `this` is undefined in ESM, which
    ///   is harmless.
    /// - Async genexps become `async function*` (an async-generator
    ///   object, consumable with `async for` / for-await).
    /// - Walrus targets keep working: they're hoisted as `let` in the
    ///   enclosing function scope (PEP 572) and simply assigned from
    ///   inside the generator body on consumption — which is also
    ///   CPython's (lazy) binding timing.
    ///
    /// The loop nest is the SAME `emit_comp_loops` all other forms use —
    /// only the accumulate op (`yield`) and the container (a generator
    /// object instead of an array) differ. See emit_collect_comprehension.
    fn emit_generator_exp(&mut self, elt: &Expr, generators: &[Comprehension]) {
        let any_async = generators.iter().any(|g| g.is_async);
        // #453: fresh internal iterator-parameter name (see fresh_temp) — a
        // user binding named `__gen_it` read in the element/conditions must
        // not be shadowed by the IIFE parameter.
        let git = self.fresh_temp("__gen_it");
        if any_async {
            self.write(&format!("(async function* ({git}) {{ "));
        } else {
            self.write(&format!("(function* ({git}) {{ "));
        }
        // WB-15 (S5): a `self` for-target shadows the receiver inside the loop
        // body; the OUTERMOST iterable (the `.call(this, …)` arg below) stays in
        // the enclosing scope. (A non-shadowing genexp keeps `self`→`this`, which
        // resolves through the explicit `.call(this, …)` receiver.)
        let cprev = self.enter_comp_self_shadow(generators);
        self.emit_comp_loops(&CompAccum::Yield(elt), generators, 0, &git, None);
        self.self_lowering = cprev;
        self.write("}).call(this, ");
        // #452: the outermost iterable is evaluated in the ENCLOSING scope —
        // lift the genexp's target scope so a target name referenced there
        // (`(x for list in list(xs))`) resolves to the enclosing binding /
        // builtin, never to the not-yet-bound loop variable.
        self.pop_scope();
        // #463: eager creation-time iterator acquisition (see doc above).
        let first = &generators[0];
        if first.is_async {
            self.need_runtime("__pyEagerAIter");
            self.write("__pyEagerAIter(");
        } else {
            self.need_runtime("__pyEagerIter");
            self.write("__pyEagerIter(");
        }
        self.emit_expr(&first.iter);
        self.write(")");
        self.push_scope(Self::comprehension_target_names(generators));
        self.write(")");
    }

    /// THE ONE comprehension loop-nest emitter — list, set, dict, AND
    /// genexp, sync AND async, all flow through here (the CPython-desugaring
    /// model: identical nested loops, parameterized accumulate op). Each
    /// level independently decides `for` vs `for await` (PEP 530 mixed
    /// levels work); the innermost statement is `accum`'s op.
    ///
    /// A new comprehension feature lands HERE — once — and every form ×
    /// async-ness gets it; there is no second copy to forget.
    fn emit_comp_loops(
        &mut self,
        accum: &CompAccum,
        generators: &[Comprehension],
        idx: usize,
        outer_it: &str,
        result: Option<&str>,
    ) {
        if idx >= generators.len() {
            match accum {
                CompAccum::Element(e) => {
                    self.write(result.expect("collect forms carry a result array"));
                    self.write(".push(");
                    self.emit_expr(e);
                    self.write("); ");
                }
                CompAccum::Pair(k, v) => {
                    self.write(result.expect("collect forms carry a result array"));
                    self.write(".push([");
                    self.emit_expr(k);
                    self.write(", ");
                    self.emit_expr(v);
                    self.write("]); ");
                }
                CompAccum::Yield(e) => {
                    self.write("yield ");
                    self.emit_expr(e);
                    self.write("; ");
                }
            }
            return;
        }

        let gen = &generators[idx];
        // `async for x in xs` (PEP 530) lowers to `for await (const x
        // of xs)`. The surrounding context must itself be async — the
        // collect IIFE / genexp wrapper is emitted `async` whenever any
        // level is async.
        if gen.is_async {
            self.write("for await (const ");
        } else {
            self.write("for (const ");
        }
        self.emit_for_target(&gen.target);
        self.write(" of ");
        // B3: the OUTERMOST iterable (`idx == 0`) was evaluated in the ENCLOSING
        // scope and passed in as `__comp_it`/`__gen_it` (already
        // protocol-bridged); consume it here so a receiver-reading outer
        // iterable is not lowered under the `self`-shadow. Inner iterables
        // reference the comprehension variables and stay inline (correctly
        // under the shadow).
        if idx == 0 {
            self.write(outer_it);
        } else if gen.is_async {
            // #239 / matrix fix: an async iterable is bridged by
            // __pyAsyncIter on the RAW expression — same policy as
            // emit_for's async arm. It is never a dict, so it must NOT go
            // through emit_iterable's sync pyForIter/pyDictKeys wrapping
            // (that wrap turned a Python-protocol `__aiter__` class into
            // its attribute keys).
            self.need_runtime("__pyAsyncIter");
            self.write("__pyAsyncIter(");
            self.emit_expr(&gen.iter);
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

        self.emit_comp_loops(accum, generators, idx + 1, outer_it, result);

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
                            // TB-1 root fix: a PLAIN (non-PSX) dict literal emits
                            // its keys VERBATIM, always. The snake→camel/kebab PSX
                            // prop-name transform (item_id→itemId, on_click→onClick,
                            // aria_label→aria-label) is correct ONLY when emitting a
                            // createElement/PSX prop NAME — gated on `convert_props`
                            // in emit_psx_element (and emit_style_dict for `style`).
                            // Leaking it into general dict-literal keys mangled the
                            // stored key while the subscript READ (`d["item_id"]`)
                            // stayed verbatim — an asymmetric silent KeyError. Same
                            // naming-soundness family as the intrinsic-tag rule / F2
                            // / #420: the transform is scoped to PSX-prop position by
                            // construction, never to dict keys.
                            self.write(&format!("\"{}\": ", escape_js_string(s)));
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

    /// TB-1: emit a DIRECT React createElement-factory call
    /// (`h(tag, props, ...children)`). Only the props argument (index 1) is in
    /// PSX-prop position, so a dict literal there gets the prop-name transform;
    /// the tag and every child are emitted verbatim. The caller has checked
    /// `args.len() >= 2`, no kwargs, and not optional.
    /// Resolve a Name in callee position to the JS binding it was imported
    /// under — the SAME resolution the bare-Name reference path uses
    /// (emit_expr): a DX-B2 alias-and-rewrite rename wins first, then an
    /// unaliased React-module import is snake→camel'd (`create_element` →
    /// `createElement`), otherwise the sanitized identifier. Factored out so
    /// `emit_create_element_call` writes the REAL binding rather than the raw
    /// Python name — B3: `from react import create_element` binds
    /// `createElement`, so a `create_element(...)` call must emit
    /// `createElement(...)`, not a load-time-undefined `create_element(...)`.
    fn resolve_name_ref(&self, name: &str) -> String {
        if !self.scope_bindings.iter().skip(1).any(|s| s.contains(name)) {
            if let Some(u) = self.import_ref_renames.get(name) {
                return u.clone();
            }
        }
        // B8(a): same binding-aware guard as emit_expr's Name branch — the
        // camel rename never captures a name shadowed by a param/local.
        if self.react_imports.contains(name)
            && !self.scope_bindings.iter().skip(1).any(|s| s.contains(name))
        {
            return react::snake_to_camel(name);
        }
        Self::sanitize_ident(name).into_owned()
    }

    /// #448: emit the native dynamic `import(spec)` for a bare / aliased
    /// `import_module(...)` call. The `package=` kwarg (CPython's relative-import
    /// anchor) has no native `import()` equivalent — `import()`'s optional 2nd
    /// argument is an import-attributes object, NOT a package anchor — so a
    /// `package=` (or any) kwarg is diagnosed rather than silently mis-emitted
    /// as `import(spec, {package: ...})`.
    fn emit_import_module_call(&mut self, args: &[Expr], kwargs: &[Keyword]) {
        if !kwargs.is_empty() {
            self.emit_expr_error(
                "`import_module(...)` does not support keyword arguments (e.g. `package=`): \
                 it lowers to the native dynamic `import()`, which has no relative-import \
                 anchor. Pass one fully-resolved specifier as the only argument.",
            );
            return;
        }
        self.write("import(");
        self.emit_call_args(args, kwargs);
        self.write(")");
    }

    /// THE single prop-key transform for the createElement factory-call
    /// surface (0.2.2 kwargs class fix): every statically-known props key —
    /// a string-literal dict key in the positional form OR a keyword name in
    /// the kwargs form — goes through this ONE rule (`react_prop_mapping`,
    /// verbatim fallback, `__proto__` computed-key guard). Two emission
    /// surfaces, one transform: the forms cannot drift apart.
    fn write_react_prop_key(&mut self, key: &str) {
        if key == "__proto__" {
            // R6: even a QUOTED `"__proto__":` key triggers the prototype
            // setter in an object literal — only a computed key creates a
            // real own prop.
            self.write("[\"__proto__\"]: ");
        } else {
            // Quote every key (matches the prior props emission
            // byte-for-byte; kebab keys like `aria-label` require quoting
            // regardless).
            let js_key = react::react_prop_mapping(key)
                .map(|m| m.to_string())
                .unwrap_or_else(|| escape_js_string(key));
            self.write(&format!("\"{}\": ", js_key));
        }
    }

    /// Diagnose the malformed/ambiguous KEYWORD forms of a createElement
    /// factory call BEFORE any output is written. Returns true when a
    /// diagnostic was emitted (the caller must return).
    ///
    /// * no positional tag at all (`h(on_click=1)`) — createElement needs a
    ///   tag/component first argument;
    /// * a positional dict literal alongside kwargs (`h("div", {...},
    ///   on_click=1)`) — two competing props objects; silently picking one
    ///   would drop the other.
    fn react_factory_kwargs_misuse(&mut self, args: &[Expr], kwargs: &[Keyword]) -> bool {
        debug_assert!(!kwargs.is_empty());
        if args.is_empty() {
            self.emit_expr_error(
                "createElement needs a tag or component as its first positional \
                 argument — keyword props alone (`create_element(on_click=...)`) \
                 have nothing to attach to.",
            );
            return true;
        }
        if args[1..]
            .iter()
            .any(|a| matches!(a.kind, ExprKind::Dict { .. }))
        {
            self.emit_expr_error(
                "ambiguous createElement call: pass props EITHER as the 2nd \
                 positional dict (`create_element(tag, {\"on_click\": f}, \
                 *children)`) OR as keywords (`create_element(tag, *children, \
                 on_click=f)`), not both — a dict literal is not a valid React \
                 child, so mixing the forms would silently drop one props set.",
            );
            return true;
        }
        false
    }

    /// Emit the ARGUMENT LIST (no surrounding parens) of a createElement
    /// factory call — ONE uniform rule for every surface that reaches the
    /// factory (name-bound `h(...)` / `create_element(...)`, and the
    /// namespace-member `react.create_element(...)` route):
    ///
    /// * positional form (no kwargs): args[1] is the props slot →
    ///   `emit_react_props_arg`;
    /// * keyword form (PSX-flat-style): kwargs are the props — static keys
    ///   transformed via `write_react_prop_key`, `**spread` entries verbatim
    ///   (the genuine TB-1 dynamic boundary) — and positionals after the tag
    ///   are children, emitted after the props object (React's
    ///   `createElement(type, props, ...children)` shape).
    ///
    /// Callers must run `react_factory_kwargs_misuse` first when kwargs are
    /// present (it diagnoses before any output is written).
    fn emit_react_factory_args(&mut self, args: &[Expr], kwargs: &[Keyword]) {
        if kwargs.is_empty() {
            for (i, arg) in args.iter().enumerate() {
                if i > 0 {
                    self.write(", ");
                }
                if i == 1 {
                    self.emit_react_props_arg(arg);
                } else if i >= 2 {
                    // Option B: factory children are native React sinks.
                    self.emit_jsx_child(arg);
                } else {
                    self.emit_expr(arg);
                }
            }
            return;
        }
        // tag
        self.emit_expr(&args[0]);
        // props from kwargs
        self.write(", {");
        for (i, kw) in kwargs.iter().enumerate() {
            if i > 0 {
                self.write(", ");
            }
            match &kw.name {
                Some(name) => {
                    self.write_react_prop_key(name);
                    if name == "style" {
                        // Item 4: same style-value rule as PSX props.
                        self.emit_react_style_value(&kw.value);
                    } else {
                        // Option B: same native-sink unbox as PSX props.
                        self.emit_native_sink_value(&kw.value, true);
                    }
                }
                None => {
                    // `**spread` — dynamic, stays verbatim (TB-1 boundary).
                    self.write("...");
                    self.emit_expr(&kw.value);
                }
            }
        }
        self.write("}");
        // children: the remaining positionals (native React sinks).
        for child in &args[1..] {
            self.write(", ");
            self.emit_jsx_child(child);
        }
    }

    /// Emit a React createElement props argument (the 2nd POSITIONAL). A dict
    /// literal whose non-spread keys are all provably strings is PSX-prop
    /// position: its string-LITERAL keys get the snake→camel/kebab prop-name
    /// transform (`react_prop_mapping`: on_click→onClick,
    /// aria_label→aria-label) via the single `write_react_prop_key` rule.
    /// Dynamic keys (f-strings / `str(...)`) stay computed, and `**spread`
    /// entries are emitted verbatim as object spreads — 0.2.2 class fix: a
    /// spread BESIDE literal keys (`{"on_click": f, **base}`) no longer
    /// disables the transform for the static keys (that shape used to emit
    /// the whole dict verbatim → dead handler). A dict with NO static
    /// string-literal keys at all — spread-only, computed-only, a variable,
    /// `None` — has nothing statically transformable and is emitted verbatim
    /// (the genuine TB-1 dynamic boundary).
    fn emit_react_props_arg(&mut self, arg: &Expr) {
        if let ExprKind::Dict { items } = &arg.kind {
            let has_static_key = items.iter().any(|i| {
                matches!(i, DictItem::KeyValue { key, .. } if Self::key_provably_string(key))
            });
            let all_props_shaped = items.iter().all(|i| match i {
                DictItem::Spread(_) => true,
                DictItem::KeyValue { key, .. } => Self::key_provably_string(key),
            });
            if has_static_key && all_props_shaped {
                self.write("({");
                for (i, item) in items.iter().enumerate() {
                    if i > 0 {
                        self.write(", ");
                    }
                    match item {
                        DictItem::Spread(value) => {
                            self.write("...");
                            self.emit_expr(value);
                        }
                        DictItem::KeyValue { key, value } => {
                            let mut is_style = false;
                            match &key.kind {
                                ExprKind::StringLiteral(s) => {
                                    let s = s.clone();
                                    is_style = s == "style";
                                    self.write_react_prop_key(&s);
                                }
                                _ => {
                                    self.write("[");
                                    self.emit_expr(key);
                                    self.write("]: ");
                                }
                            }
                            if is_style {
                                // Item 4: the factory's 2nd-positional props
                                // dict gets the same style-value rule as PSX
                                // props (nested CSS keys snake→camel'd).
                                self.emit_react_style_value(value);
                            } else {
                                // Option B: same native-sink unbox as PSX
                                // props (boxed floats never reach React).
                                self.emit_native_sink_value(value, true);
                            }
                        }
                    }
                }
                self.write("})");
                return;
            }
        }
        self.emit_expr(arg);
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
            // Provably-iterable statics keep the raw for..of fast path.
            JsInferredType::List | JsInferredType::Set | JsInferredType::Tuple => {
                self.emit_expr(iter)
            }
            // #239: an UNKNOWN operand (e.g. an untyped dict parameter) may be a
            // plain-object dict (not JS-iterable) or a Map/Counter (whose for..of
            // yields entries, not keys). Route through pyForIter, which iterates
            // dict keys and passes lists/tuples/sets/strings/generators through.
            //
            // #473: Primitive/Float take the SAME guard — a statically-numeric
            // local (`n = 5; for y in n:`) used to inline a raw `for..of` and
            // leak the native JS "n is not iterable" TypeError; pyForIter
            // raises CPython's "'int' object is not iterable" instead. The
            // one iterable Primitive (str) passes through pyForIter untouched
            // (one call at loop setup, not per iteration).
            JsInferredType::Unknown | JsInferredType::Primitive | JsInferredType::Float => {
                self.need_runtime("pyForIter");
                self.write("pyForIter(");
                self.emit_expr(iter);
                self.write(")");
            }
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
        // same [key, value] pair stream. The container init is the ONLY
        // dict-specific part — the pair stream itself comes from the
        // unified lowering (CompAccum::Pair), so async arms / eval-order
        // / naming hygiene can never diverge from the other forms (#454).
        if Self::key_provably_string(key) {
            self.write("Object.fromEntries(");
        } else {
            self.need_runtime("PyDict");
            self.write("new PyDict(");
        }
        self.emit_collect_comprehension(&CompAccum::Pair(key, value), generators);
        self.write(")");
    }
}

/// CPython desugars EVERY comprehension form — list/set/dict/genexp, sync
/// AND async — to the SAME nested-loop scope function; the forms differ
/// ONLY in the accumulate op (append / add / `__setitem__` / yield), the
/// container init, and per-level awaited-ness. This enum IS the accumulate
/// parameterization; `emit_comp_loops` is the single consumer. The
/// per-form-emitter era repeatedly left a feature off one form (#454: no
/// async arm on the dict path; #463: genexp iter-timing) — with the ops
/// centralized here, a left-out arm is structurally impossible.
enum CompAccum<'a> {
    /// list/set comprehensions: `__result.push(elt)` (set callers wrap the
    /// finished array in `new PySet(...)`).
    Element(&'a Expr),
    /// dict comprehensions: `__result.push([key, value])` — a pair stream
    /// the caller feeds to `new PyDict(...)` / `Object.fromEntries(...)`.
    Pair(&'a Expr, &'a Expr),
    /// generator expressions: `yield elt` (no result array).
    Yield(&'a Expr),
}

impl<'a> CompAccum<'a> {
    /// The value expressions the op accumulates — the walrus-detection
    /// surface (a `:=` in any of them forces the loop path).
    fn exprs(&self) -> Vec<&'a Expr> {
        match self {
            CompAccum::Element(e) | CompAccum::Yield(e) => vec![e],
            CompAccum::Pair(k, v) => vec![k, v],
        }
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
        AugAssignOp::MatMul => "/* @= */",
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
    // public #3: derived from the ONE canonical builtin-name list (plus the
    // mapping tables and the site-customization aliases exit/quit) instead
    // of a third hand-maintained enumeration that could drift.
    crate::builtins::builtin_func_mapping(name).is_some()
        || crate::builtins::builtin_value_mapping(name).is_some()
        || crate::builtins::CPYTHON_BUILTIN_FUNCTIONS.contains(&name)
        || matches!(name, "exit" | "quit")
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
/// Option B: JS BUILT-IN constructors reachable through the capitalized-name
/// `new` heuristic. Calls to these are JS-interop by definition (they are
/// never compiled Python classes), so float arguments unbox to native
/// Numbers at the call boundary — several of them dispatch on
/// `typeof === "number"` (`Array(n)`, the TypedArray length-vs-iterable
/// overloads). A user class with one of these names would shadow the global
/// anyway and lands in `known_classes`, which is checked FIRST.
fn is_js_builtin_ctor(name: &str) -> bool {
    matches!(
        name,
        "Array"
            | "Date"
            | "RegExp"
            | "Error"
            | "Map"
            | "Set"
            | "WeakMap"
            | "WeakSet"
            | "Promise"
            | "Proxy"
            | "DataView"
            | "ArrayBuffer"
            | "SharedArrayBuffer"
            | "Int8Array"
            | "Uint8Array"
            | "Uint8ClampedArray"
            | "Int16Array"
            | "Uint16Array"
            | "Int32Array"
            | "Uint32Array"
            | "Float32Array"
            | "Float64Array"
            | "BigInt64Array"
            | "BigUint64Array"
            | "URL"
            | "URLSearchParams"
            | "Blob"
            | "File"
            | "FormData"
            | "Headers"
            | "Request"
            | "Response"
            | "WebSocket"
            | "XMLHttpRequest"
            | "EventSource"
            | "Event"
            | "CustomEvent"
            | "AbortController"
            | "TextEncoder"
            | "TextDecoder"
            | "Image"
            | "Audio"
            | "Option"
            | "Worker"
            | "MessageChannel"
            | "IntersectionObserver"
            | "MutationObserver"
            | "ResizeObserver"
            | "Notification"
            | "Intl"
    )
}

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
    // autotester iterators_and_generators: recursive — `r = r + (yield r)`
    // buries the Yield inside a BinOp, and the old top-level-only match left
    // the emitted function a NON-generator (bare `yield` → SyntaxError).
    // Lambdas/comprehensions are their own scopes and are not descended into.
    match &expr.kind {
        ExprKind::Yield(_) | ExprKind::YieldFrom(_) => true,
        ExprKind::BinOp { left, right, .. } => {
            expr_contains_yield(left) || expr_contains_yield(right)
        }
        ExprKind::UnaryOp { operand, .. } => expr_contains_yield(operand),
        ExprKind::Compare { left, comparisons } => {
            expr_contains_yield(left) || comparisons.iter().any(|(_, e)| expr_contains_yield(e))
        }
        ExprKind::Call {
            func, args, kwargs, ..
        } => {
            expr_contains_yield(func)
                || args.iter().any(expr_contains_yield)
                || kwargs.iter().any(|k| expr_contains_yield(&k.value))
        }
        ExprKind::IfExpr {
            test,
            body,
            else_body,
        } => {
            expr_contains_yield(test)
                || expr_contains_yield(body)
                || expr_contains_yield(else_body)
        }
        ExprKind::Tuple(elts) | ExprKind::List(elts) | ExprKind::Set(elts) => {
            elts.iter().any(expr_contains_yield)
        }
        ExprKind::Dict { items } => items.iter().any(|item| match item {
            DictItem::KeyValue { key, value } => {
                expr_contains_yield(key) || expr_contains_yield(value)
            }
            DictItem::Spread(e) => expr_contains_yield(e),
        }),
        ExprKind::Subscript { value, index, .. } => {
            expr_contains_yield(value) || expr_contains_yield(index)
        }
        ExprKind::Attribute { value, .. } => expr_contains_yield(value),
        ExprKind::Starred(e) | ExprKind::Await(e) => expr_contains_yield(e),
        ExprKind::NamedExpr { value, .. } => expr_contains_yield(value),
        ExprKind::FString { parts } => parts.iter().any(|p| match p {
            FStringPart::Expr(e) => expr_contains_yield(e),
            _ => false,
        }),
        ExprKind::Slice { lower, upper, step } => [lower, upper, step]
            .into_iter()
            .flatten()
            .any(|e| expr_contains_yield(e)),
        _ => false,
    }
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
    "cmath",
    "unicodedata",
];

/// Embedded stdlib shim sources: the build-time export surface for
/// star-import binding (`from math import *` binds every export). Parsed
/// from the SAME canonical files the runtime package ships, so the bound
/// name set can never drift from what the module actually exports.
const STDLIB_JS_SOURCES: &[(&str, &str)] = &[
    ("math", include_str!("../../../runtime/src/stdlib/math.js")),
    ("json", include_str!("../../../runtime/src/stdlib/json.js")),
    ("itertools", include_str!("../../../runtime/src/stdlib/itertools.js")),
    ("functools", include_str!("../../../runtime/src/stdlib/functools.js")),
    ("collections", include_str!("../../../runtime/src/stdlib/collections.js")),
    ("random", include_str!("../../../runtime/src/stdlib/random.js")),
    ("datetime", include_str!("../../../runtime/src/stdlib/datetime.js")),
    ("re", include_str!("../../../runtime/src/stdlib/re.js")),
    ("decimal", include_str!("../../../runtime/src/stdlib/decimal.js")),
    ("fractions", include_str!("../../../runtime/src/stdlib/fractions.js")),
    ("operator", include_str!("../../../runtime/src/stdlib/operator.js")),
    ("copy", include_str!("../../../runtime/src/stdlib/copy.js")),
    ("string", include_str!("../../../runtime/src/stdlib/string.js")),
    ("heapq", include_str!("../../../runtime/src/stdlib/heapq.js")),
    ("bisect", include_str!("../../../runtime/src/stdlib/bisect.js")),
    ("sys", include_str!("../../../runtime/src/stdlib/sys.js")),
    ("cmath", include_str!("../../../runtime/src/stdlib/cmath.js")),
    ("unicodedata", include_str!("../../../runtime/src/stdlib/unicodedata.js")),
];

/// Export names of a stdlib shim: `(name, is_class)` for every column-0
/// `export function`/`function*`/`class`/`const`/`let` declaration. Private
/// helpers (leading `_`) are not exported by these shims, but filter them
/// anyway — CPython's `import *` skips underscore names.
fn stdlib_export_names(module: &str) -> Option<Vec<(String, bool)>> {
    let src = STDLIB_JS_SOURCES
        .iter()
        .find(|(m, _)| *m == module)
        .map(|(_, s)| *s)?;
    let mut out = Vec::new();
    for line in src.lines() {
        let Some(rest) = line.strip_prefix("export ") else {
            continue;
        };
        let (rest, is_class) = match rest.strip_prefix("class ") {
            Some(r) => (r, true),
            None => {
                let r = rest
                    .strip_prefix("function* ")
                    .or_else(|| rest.strip_prefix("function "))
                    .or_else(|| rest.strip_prefix("const "))
                    .or_else(|| rest.strip_prefix("let "));
                match r {
                    Some(r) => (r, false),
                    None => continue, // `export { … }` re-exports etc.
                }
            }
        };
        let name: String = rest
            .chars()
            .take_while(|c| c.is_ascii_alphanumeric() || *c == '_' || *c == '$')
            .collect();
        if !name.is_empty() && !name.starts_with('_') {
            out.push((name, is_class));
        }
    }
    Some(out)
}

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
        "AssertionError" => &["AssertionError"],
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

/// delta4 — the CHECKED MANIFEST of every runtime symbol the codegen can
/// emit an `import { ... } from "pyths-runtime"` / `"pyths-runtime/core"`
/// line for. Single source of truth for the export-surface drift guard:
///
///   * `need_runtime` (the SOLE write path into `runtime_imports`)
///     debug_asserts that every registered name is listed here — so any new
///     emitter helper that is not added to this list panics across the debug
///     test suite.
///   * cli_test.rs `runtime_export_surface_covers_all_emittable_symbols`
///     imports BOTH package entry points (runtime/src/index.js AND the
///     `--target worker` entry runtime/src/core.js) under node and fails if
///     any name here is missing from either — so a manifest entry cannot
///     land until both entries actually export it.
///
/// Keep the list sorted. It may be a small SUPERSET of what the emitter can
/// currently produce (supersets only cost an extra export check); it must
/// never be a subset.
/// Test-only surface for the inline-vs-package runtime PARITY GATE
/// (`tests/inline_runtime_parity.rs`): build the inline runtime text for an
/// arbitrary needed set, exactly as `pyths run`/`bundle` would embed it.
#[doc(hidden)]
pub fn inline_runtime_for_test(names: &[&str]) -> String {
    let needed: HashSet<String> = names.iter().map(|s| s.to_string()).collect();
    JsCodegen::emit_inline_runtime(&needed)
}

pub const EMITTABLE_RUNTIME_SYMBOLS: &[&str] = &[
    "ArithmeticError",
    "AssertionError",
    "AttributeError",
    "BaseException",
    "Exception",
    "IndexError",
    "KeyError",
    "LookupError",
    "NameError",
    "NotImplementedError",
    "OverflowError",
    "PyDict",
    "PyObject",
    "PySet",
    "RuntimeError",
    "StopAsyncIteration",
    "StopIteration",
    "TypeError",
    "UnboundLocalError",
    "ValueError",
    "ZeroDivisionError",
    "__UNBOUND",
    "__pyAsyncIter",
    "__pyAttrCall",
    "__pyCall",
    "__pyCallKw",
    "__pyChkFree",
    "__pyChkGlobal",
    "__pyChkLocal",
    "__pyClass",
    "__pyClassAttr",
    "__pyClassCall",
    "__pyClassCallKw",
    "__pyDecorateClassMethod",
    "__pyDecorateMethod",
    "__pyEagerAIter",
    "__pyEagerIter",
    "__pyEffect",
    "__pyEffectArgs",
    "__pyF",
    "__pyFnMeta",
    "__pyIsInstance",
    "__pyIsSubclass",
    "__pyJs",
    "__pyKwArgs",
    "__pyKwPop",
    "__pyMarkTuple",
    "__pyNoExtraKw",
    "__pyRangeIter",
    "__pySliceObj",
    "__pySuper",
    "__pyTakeKw",
    "__pyTypeBool",
    "__pyTypeBytearray",
    "__pyTypeBytes",
    "__pyTypeDict",
    "__pyTypeFloat",
    "__pyTypeFrozenset",
    "__pyTypeInt",
    "__pyTypeList",
    "__pyTypeObject",
    "__pyTypeSet",
    "__pyTypeStr",
    "__pyTypeTuple",
    "__reqNum",
    "pyAbs",
    "pyAdd",
    "pyAll",
    "pyAnd",
    "pyAny",
    "pyAppend",
    "pyAscii",
    "pyBin",
    "pyBitAnd",
    "pyBitNot",
    "pyBitOr",
    "pyBitXor",
    "pyBool",
    "pyBoundMethod",
    "pyBytearrayOf",
    "pyBytes",
    "pyBytesOf",
    "pyCallable",
    "pyChr",
    "pyClear",
    "pyComplex",
    "pyComplexOf",
    "pyConjugate",
    "pyContains",
    "pyCopy",
    "pyCount",
    "pyDelItem",
    "pyDelSlice",
    "pyDelattr",
    "pyDict",
    "pyDictFromkeys",
    "pyDictGet",
    "pyDictItems",
    "pyDictKeys",
    "pyDictMerge",
    "pyDictPopitem",
    "pyDictSetdefault",
    "pyDictValues",
    "pyDir",
    "pyDiscard",
    "pyDiv",
    "pyDivmod",
    "pyEnumerate",
    "pyEq",
    "pyExtend",
    "pyFind",
    "pyFixed",
    "pyFloat",
    "pyFloorDiv",
    "pyForIter",
    "pyFormat",
    "pyFormatDynamic",
    "pyFormatFloat",
    "pyFormatSpec",
    "pyFrozensetOf",
    "pyGe",
    "pyGenClose",
    "pyGenSend",
    "pyGenThrow",
    "pyGetItem",
    "pyGetattr",
    "pyGt",
    "pyHasattr",
    "pyHex",
    "pyIAdd",
    "pyIBitAnd",
    "pyIBitOr",
    "pyIBitXor",
    "pyIMatMul",
    "pyIMul",
    "pyISub",
    "pyIndex",
    "pyInsert",
    "pyInt",
    "pyIter",
    "pyLe",
    "pyLen",
    "pyListOf",
    "pyListSort",
    "pyLt",
    "pyMap",
    "pyMatMul",
    "pyMax",
    "pyMin",
    "pyMod",
    "pyMul",
    "pyNe",
    "pyNeg",
    "pyNext",
    "pyNormalizeStyle",
    "pyOct",
    "pyOr",
    "pyOrd",
    "pyPop",
    "pyPos",
    "pyPow",
    "pyPowBuiltin",
    "pyPrint",
    "pyProperty",
    "pyRange",
    "pyRemove",
    "pyRepr",
    "pyReversed",
    "pyRound",
    "pySeq",
    "pySetDifference",
    "pySetDifferenceUpdate",
    "pySetIntersection",
    "pySetIntersectionUpdate",
    "pySetIsdisjoint",
    "pySetIssubset",
    "pySetIssuperset",
    "pySetItem",
    "pySetOf",
    "pySetSlice",
    "pySetSymmetricDifference",
    "pySetSymmetricDifferenceUpdate",
    "pySetUnion",
    "pySetattr",
    "pyShiftLeft",
    "pyShiftRight",
    "pySlice",
    "pySliceOf",
    "pySorted",
    "pyStr",
    "pyStrCapitalize",
    "pyStrCenter",
    "pyStrEndswith",
    "pyStrExpandtabs",
    "pyStrFormat",
    "pyStrIsidentifier",
    "pyStrIslower",
    "pyStrIsprintable",
    "pyStrIstitle",
    "pyStrIsupper",
    "pyStrJoin",
    "pyStrLjust",
    "pyStrLstrip",
    "pyStrPartition",
    "pyStrReplace",
    "pyStrReplaceSmart",
    "pyStrRfind",
    "pyStrRindex",
    "pyStrRjust",
    "pyStrRpartition",
    "pyStrRsplit",
    "pyStrRstrip",
    "pyStrSplit",
    "pyStrSplitlines",
    "pyStrStartswith",
    "pyStrStrip",
    "pyStrSwapcase",
    "pyStrTitle",
    "pyStrTranslate",
    "pySub",
    "pySum",
    "pyTuple",
    "pyTupleOf",
    "pyType",
    "pyUpdate",
    "pyVars",
    "pyZip",
];

/// Every Python builtin exception class the codegen understands — the ONE
/// list behind `is_builtin_exception`. Round-6 delta: the class-base path
/// (`class E(TypeError)`) auto-imports ANY of these names, TypeError
/// included (only the raise-/except-site paths skip TypeError, to avoid
/// shadowing the JS global for no gain), so EVERY name here must be in
/// EMITTABLE_RUNTIME_SYMBOLS — enforced by
/// `tests::manifest_covers_all_builtin_exceptions` below, which makes the
/// exception-base drift (TypeError was emittable but unmanifested)
/// structurally unrepeatable.
const BUILTIN_EXCEPTIONS: &[&str] = &[
    // autotester exceptions: BaseException is subclassable/raisable as the
    // real hierarchy root (`class Table(BaseException)`); `except
    // BaseException` stays the unconditional catch-all path.
    "AssertionError",
    "BaseException",
    "Exception",
    "ValueError",
    "IndexError",
    "KeyError",
    "AttributeError",
    "StopIteration",
    "ZeroDivisionError",
    // Batch G: TypeError (thrown by pyOrd et al. as a name-tagged
    // Error) and OverflowError (pyInt on float('inf')) need
    // name-based except matching too.
    "TypeError",
    "OverflowError",
    // Round-4 sweep: the runtime grew CPython's hierarchy classes
    // (LookupError/ArithmeticError bases, RuntimeError,
    // NotImplementedError, StopAsyncIteration) — raise/except sites
    // need their auto-imports too.
    "RuntimeError",
    "NotImplementedError",
    "LookupError",
    "ArithmeticError",
    "StopAsyncIteration",
    // PBT-2: zero-iteration for-loop target reads raise these; the
    // runtime grew the classes, so raise/except sites auto-import.
    "NameError",
    "UnboundLocalError",
];

fn is_builtin_exception(name: &str) -> bool {
    BUILTIN_EXCEPTIONS.contains(&name)
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
/// Whether a BARE (level-0) `from <module> import ...` names a recognized
/// EXTERNAL package rather than a sibling project `.ps` module. External =
/// the React/Next ecosystem, a PythScribe stdlib module, asyncio, the
/// `pyths`/`pyths.*` meta modules, a scoped `at_<org>.<pkg>` npm package, or
/// a name explicitly mapped in `NPM_MODULE_MAPPINGS`. Everything else that is
/// imported bare is assumed to be a local sibling module (WB-8): its classes
/// take the cooperative PyObject/`__pyClass` path so cross-module `super()`
/// works, exactly as a relative import already does (#300). External bases
/// (React.Component, stdlib containers, npm classes) keep native `extends` +
/// a native constructor.
fn is_external_pkg_module(module: &str) -> bool {
    is_react_or_next_module(module)
        || STDLIB_MODULES.contains(&module)
        || module == "asyncio"
        || module == "pyths"
        || module.starts_with("pyths.")
        || module.starts_with("at_")
        || NPM_MODULE_MAPPINGS.iter().any(|(py, _)| *py == module)
}

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

    /// Round-6 delta drift guard: EVERY builtin exception class is emittable
    /// as a class BASE (`class E(TypeError)` auto-imports it), so the checked
    /// manifest must contain the WHOLE list — TypeError was emittable but
    /// unmanifested, panicking the debug drift-assert. One list, one test:
    /// adding an exception to BUILTIN_EXCEPTIONS without adding it to
    /// EMITTABLE_RUNTIME_SYMBOLS (and thus to both package entry points, via
    /// the cli export-surface gate) fails right here.
    #[test]
    fn manifest_covers_all_builtin_exceptions() {
        let missing: Vec<&&str> = BUILTIN_EXCEPTIONS
            .iter()
            .filter(|n| !EMITTABLE_RUNTIME_SYMBOLS.contains(*n))
            .collect();
        assert!(
            missing.is_empty(),
            "builtin exception classes emittable as class bases but missing              from EMITTABLE_RUNTIME_SYMBOLS: {missing:?}"
        );
    }

    /// The manifest must stay sorted (binary-searchable, mergeable, and the
    /// doc comment promises it).
    #[test]
    fn manifest_is_sorted_and_deduped() {
        let mut sorted = EMITTABLE_RUNTIME_SYMBOLS.to_vec();
        sorted.sort_unstable();
        sorted.dedup();
        assert_eq!(EMITTABLE_RUNTIME_SYMBOLS, &sorted[..], "manifest must be sorted + deduped");
    }

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

    // #473: a statically-NON-iterable local (`n = 5; for y in n:`) must take
    // the pyForIter guard too — the raw `for..of` leaked the native JS
    // "n is not iterable" TypeError instead of CPython's
    // "'int' object is not iterable".
    #[test]
    fn test_for_over_primitive_local_uses_pyforiter() {
        let ji = compile("n = 5\nfor y in n:\n    print(y)");
        assert!(
            ji.contains("of pyForIter(n)"),
            "numeric-local loop must route through pyForIter:\n{ji}"
        );
        let jf = compile("x = 2.5\nfor y in x:\n    print(y)");
        assert!(
            jf.contains("of pyForIter(x)"),
            "float-local loop must route through pyForIter:\n{jf}"
        );
    }

    // #472: calling a local whose static type is provably non-callable
    // (`d = {"a": 1}; d()`) must route through __pyCall so the failure is
    // CPython's "'dict' object is not callable", not the native JS
    // "d is not a function". A known def keeps the raw direct call.
    #[test]
    fn test_call_on_non_callable_local_uses_pycall() {
        let jd = compile("d = {\"a\": 1}\nd()");
        assert!(
            jd.contains("__pyCall(d, ["),
            "dict-local call must route through __pyCall:\n{jd}"
        );
        let jn = compile("n = 5\nn()");
        assert!(
            jn.contains("__pyCall(n, ["),
            "int-local call must route through __pyCall:\n{jn}"
        );
        // a known def stays a raw direct call (no wrap)
        let jf = compile("def f():\n    return 1\nf()");
        assert!(
            !jf.contains("__pyCall("),
            "known-def call must stay raw:\n{jf}"
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
    // function produced a Node `SyntaxError`. Round-4 (findings 2 & 3): a
    // function-local import is genuinely function-local, so the module-top
    // `import` is hoisted under a UNIQUE name and the local name is bound in
    // the body via `let <name> = <unique>` — never leaked as a bare module
    // binding.
    #[test]
    fn test_function_local_import_hoisted_to_module_scope() {
        let js =
            compile("def f():\n    import random\n    return random.randint(0, 5)\nprint(f())");
        // The actual `import` is hoisted to the module top under a unique name.
        assert!(
            js.contains("import * as __pyimp_random_0 from \"pyths-runtime/stdlib/random\";"),
            "hoisted unique import missing:\n{}",
            js,
        );
        let import_at = js.find("import * as __pyimp_random_0").expect("no random import");
        let fn_at = js.find("function f(").expect("no function f");
        assert!(
            import_at < fn_at,
            "function-local import was not hoisted above `function f`:\n{}",
            js,
        );
        // The local name is bound INSIDE the function body (not at module top).
        assert!(
            js.contains("let random = __pyimp_random_0;"),
            "function-local `random` binding missing:\n{}",
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
        // Option B: the float literal operand carries its box (__pyF(10) —
        // the runtime discriminates float-ness by brand) AND the static
        // float-context flag is still passed (belt: a BigInt right operand
        // must coerce-or-overflow on the float path).
        assert!(
            js.contains("pyPow(__pyF(10), 400, true)"),
            "float-ctx flag missing on pyPow:\n{}",
            js
        );
        assert!(
            js.contains("__reqNum = (x)"),
            "inline __reqNum missing:\n{}",
            js
        );
        assert!(
            js.contains("(typeof x === \"number\" && !Number.isInteger(x))"),
            "inline __isFloat must be the authority classifier (brand or non-integer Number):\n{}",
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
    // raise TypeError, not coerce to NaN. E2 (#466): the None guard is now
    // subsumed by the binary-op operand-type authority (__binOpTypeError) —
    // the inline arithmetic block carries the same authority the package
    // operators.js does (both runtimes fixed), and pyAdd's fall-through
    // terminates in it instead of a raw JS `a + b`.
    #[test]
    fn test_inline_arith_none_guard_present() {
        let js = compile_inline("v = {}\nprint(repr(v.get(5) + 1))");
        assert!(
            js.contains("function __binOpTypeError("),
            "operand-type authority missing:\n{}",
            js
        );
        assert!(
            js.contains("unsupported operand type(s) for"),
            "CPython TypeError message missing:\n{}",
            js
        );
        assert!(
            js.contains("__binOpTypeError(\"+\", a, b)"),
            "pyAdd authority call missing"
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

    #[test]
    fn round3_range_for_uses_shared_lazy_iter() {
        // ROOT FIX: the optimized for-range lowering iterates the SHARED lazy
        // __pyRangeIter (same guards as pyRange, BigInt/2**53-safe), NOT a
        // hand-rolled value-controlled `i += step` counter.
        let js = compile("step = 0\nfor i in range(1, 0, step):\n    print(i)");
        assert!(js.contains("of __pyRangeIter("), "range-for must iterate __pyRangeIter:\n{js}");
        assert!(!js.contains("+= step") && !js.contains("__pyRangeArgs"),
            "no hand-rolled counter / old guard:\n{js}");
        // inline mode must also inline the shared iterator + normalizer.
        let ji = compile_inline("for i in range(3):\n    print(i)");
        assert!(ji.contains("function* __pyRangeIter("), "inlined iterator missing:\n{ji}");
        assert!(ji.contains("function __pyRangeNorm("), "shared normalizer missing:\n{ji}");
    }

    #[test]
    fn round2_kwargs_proto_uses_computed_key() {
        // R6: a `__proto__` kwarg must emit a COMPUTED key, never the literal
        // `__proto__:` / `"__proto__":` (both invoke the prototype setter).
        let js = compile("def f(**kw):\n    return kw\nf(__proto__=1)");
        assert!(js.contains("[\"__proto__\"]:"), "computed proto key missing:\n{js}");
        assert!(
            !js.contains("{__proto__:") && !js.contains(" __proto__: "),
            "unsafe literal proto key emitted:\n{js}"
        );
    }

    // ── Multi-file BUG #1: `from . import <submodule>` ─────────────────────
    // The old lowering emitted `import { a } from "./"` — asking the package
    // index to provide ITSELF a named export `a` (guaranteed ESM link error)
    // — and `a.X` mis-lowered through pyBoundMethod. Root fix: a MODULE-
    // NAMESPACE import of the submodule file, with the binding tracked in
    // `module_namespaces` so member access is a direct property read.
    #[test]
    fn test_from_dot_submodule_is_namespace_import() {
        let js = compile("from . import a\nprint(a.X)\nprint(a.f())\ng = a.X");
        assert!(
            js.contains("import * as a from \"./a\";"),
            "must namespace-import the submodule file:\n{js}"
        );
        assert!(
            !js.contains("from \"./\""),
            "self-referential package-index import must be gone:\n{js}"
        );
        assert!(
            js.contains("pyPrint(a.X)") && js.contains("pyPrint(a.f())"),
            "member access on the module namespace must be direct:\n{js}"
        );
        assert!(
            !js.contains("pyBoundMethod(a,"),
            "module-namespace member reads must not pyBoundMethod-wrap:\n{js}"
        );
    }

    #[test]
    fn test_from_dot_submodule_alias_capitalized_and_levels() {
        // Aliased form binds the alias.
        let js = compile("from . import a as mod_a\nprint(mod_a.X)");
        assert!(
            js.contains("import * as mod_a from \"./a\";"),
            "aliased submodule import:\n{js}"
        );
        // A capitalized member CALL on the namespace is a cross-module class
        // instantiation → `new` (same rule as stdlib module namespaces).
        let jc = compile("from . import shapes\ns = shapes.Shape(1)");
        assert!(
            jc.contains("new shapes.Shape(1)"),
            "capitalized member call must `new`:\n{jc}"
        );
        // `from .. import x` climbs one package level.
        let j2 = compile("from .. import util\nprint(util.V)");
        assert!(
            j2.contains("import * as util from \"./../util\";"),
            "level-2 submodule specifier:\n{j2}"
        );
    }

    #[test]
    fn test_from_pkg_named_symbol_import_unchanged() {
        // The WORKING named-reexport form (`from .impl import work` — a
        // SYMBOL of impl) must stay a named import, not become a namespace.
        let js = compile("from .impl import work\nwork()");
        assert!(
            js.contains("import { work } from \"./impl\";"),
            "named relative symbol import must stay named:\n{js}"
        );
    }

    // ── FIX 2: package-index SYMBOL import (sentinel module ".") ───────────
    // `from . import CONST` where CONST is a symbol of the package __init__
    // (no submodule file) is rewritten by the CLI pre-pass to the sentinel
    // module "." — which must lower to a NAMED import from the index
    // specifier, the correct pre-BUG#1 behavior for this half of the
    // ambiguous form. The sentinel is unreachable from the parser (leading
    // dots parse into `level`), so the AST is built by hand here.
    #[test]
    fn test_index_symbol_sentinel_lowers_to_named_index_import() {
        use pyths_syntax::ast::{ImportAlias, Module, Stmt, StmtKind};
        use pyths_syntax::span::Span;
        let module = Module {
            body: vec![Stmt::new(
                StmtKind::ImportFrom {
                    module: ".".to_string(),
                    names: vec![ImportAlias {
                        name: "CONST".to_string(),
                        alias: None,
                    }],
                    level: 1,
                },
                Span::new(0, 0),
            )],
            span: Span::new(0, 0),
        };
        let mut gen = JsCodegen::new();
        gen.emit_module(&module);
        let js = gen.finish();
        assert!(
            js.contains("import { CONST } from \"./\";"),
            "sentinel \".\" must be a named import from the package index:\n{js}"
        );
        assert!(
            !js.contains("import * as CONST"),
            "index SYMBOL must not become a submodule namespace import:\n{js}"
        );
    }

    // ── FIX 1(b): module-level tuple/list-unpack targets EXPORT ────────────
    // `x, y = 1, 2` at module level binds ordinary Python module globals —
    // they must export exactly like plain assignments (B-015) and AnnAssign.
    // This was the one binding form the export model missed: per-module
    // consumers link-failed and bundles silently bound `undefined`.
    #[test]
    fn test_module_level_unpack_targets_export() {
        let js = compile("x, y = 1, 2\n[p, q] = [3, 4]");
        for n in ["x", "y", "p", "q"] {
            assert!(
                js.contains(&format!("export let {};", n)),
                "module-level unpack target `{n}` must export:\n{js}"
            );
        }
        // Function-local unpack stays local — no export inside a body.
        let jf = compile("def g():\n    a, b = 1, 2\n    return a + b");
        assert!(
            !jf.contains("export let a"),
            "function-local unpack must NOT export:\n{jf}"
        );
    }

    // ── Multi-file BUG #2 backstop: unexpanded relative star is LOUD ───────
    // The CLI expands `from .mod import *` from the sibling source
    // (commands::relstar). If a caller bypasses that pass, codegen must be a
    // hard error — the old behavior emitted NOTHING (silent miscompile:
    // clean compile, bare ReferenceError at runtime).
    #[test]
    fn test_relative_star_unexpanded_is_hard_error() {
        let module = pyths_parser::parse("from .impl import *\nprint(Y)").expect("Parse failed");
        let mut gen = JsCodegen::new();
        gen.emit_module(&module);
        let errors = gen.take_errors();
        let js = gen.finish();
        assert!(
            !errors.is_empty(),
            "unexpanded relative star must record a codegen error"
        );
        assert!(
            errors[0].contains("from .impl import *"),
            "error must name the import: {}",
            errors[0]
        );
        assert!(
            js.contains("throw new Error"),
            "emitted artifact must fail loud, not silently drop the import:\n{js}"
        );
    }
}
