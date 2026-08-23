use std::cell::Cell;
use std::collections::{BTreeMap, BTreeSet, HashMap};

use pyths_hir::{class_registry, exception_code, WasmAnalysis, WasmFuncInfo};
use pyths_syntax::ast::{ExceptHandler, Expr, ExprKind, FStringPart, Module, Stmt, StmtKind};
use pyths_syntax::operators::{AugAssignOp, BinOp, UnaryOp};
use pyths_types::types::{resolve_type, Type};
use wasm_encoder::{
    CodeSection, ConstExpr, DataCountSection, DataSection, ElementSection, Elements, ExportKind,
    ExportSection, Function, FunctionSection, GlobalSection, GlobalType, ImportSection,
    Instruction, MemArg, MemorySection, MemoryType, Module as WasmModule, RefType, TableSection,
    TableType, TypeSection, ValType,
};

use crate::types::{to_wasm_type, WasmType};

/// Math.* functions that map to JS imports.
/// Tuple: (python_name, arity). All take and return f64.
pub const MATH_FUNCTIONS: &[(&str, u32)] = &[
    ("sqrt", 1),
    ("sin", 1),
    ("cos", 1),
    ("tan", 1),
    ("asin", 1),
    ("acos", 1),
    ("atan", 1),
    ("log", 1),
    ("log2", 1),
    ("log10", 1),
    ("exp", 1),
    ("ceil", 1),
    ("floor", 1),
    ("fabs", 1),
    ("atan2", 2),
    ("pow", 2),
];

/// Math.* constants that compile to inline f64.const.
pub const MATH_CONSTANTS: &[(&str, f64)] = &[
    ("pi", std::f64::consts::PI),
    ("e", std::f64::consts::E),
    ("tau", std::f64::consts::TAU),
    ("inf", f64::INFINITY),
];

/// Look up arity of a math function. Returns None if not supported.
pub fn math_function_arity(name: &str) -> Option<u32> {
    MATH_FUNCTIONS
        .iter()
        .find(|(n, _)| *n == name)
        .map(|(_, a)| *a)
}

/// Look up value of a math constant. Returns None if not supported.
pub fn math_constant_value(name: &str) -> Option<f64> {
    MATH_CONSTANTS
        .iter()
        .find(|(n, _)| *n == name)
        .map(|(_, v)| *v)
}

/// Emits WASM binary from PythScribe AST.
pub struct WasmEmitter {
    /// Map function name â†’ WASM function index (accounting for imports)
    func_indices: HashMap<String, u32>,
    /// Info about each eligible function
    func_info: HashMap<String, WasmFuncInfo>,
    /// Sorted set of math.* functions to import (e.g. "pow", "sqrt", "atan2").
    /// Replaces the old needs_pow boolean â€” pow is just one entry here.
    math_imports: BTreeSet<String>,
    /// Names bound by `from math import X [as Y]` → canonical math function
    /// name (only entries for which `math_function_arity` is Some). Lets a bare
    /// `sqrt(x)` call dispatch to the `math.sqrt` import the same way the
    /// `math.sqrt(x)` attribute form does. Populated during emit_module.
    math_aliases: HashMap<String, String>,
    /// Map import name â†’ WASM function index. Populated during emit_module.
    import_indices: HashMap<String, u32>,
    // === String support ===
    /// Pooled string literals: (value, offset_in_data_section)
    string_pool: Vec<(String, u32)>,
    /// Deduplication map: string value â†’ data section offset
    string_dedup: HashMap<String, u32>,
    /// Total size of the data section (bytes)
    data_section_size: u32,
    /// Whether any eligible function uses string types
    needs_strings: bool,
    /// Function index of the internal __alloc function
    alloc_func_idx: u32,
    /// Whether any eligible function uses raise/assert/try (Tier 7).
    /// When true, emit `__err_code: i32` global and check it in the bridge.
    needs_errors: bool,
    /// Index of the `__err_code` global. Only meaningful when needs_errors=true.
    /// Globals are: 0 = __heap_ptr (if needs_strings), then __err_code (if needs_errors).
    err_code_global_idx: u32,
    /// Index of the `__err_msg` global — i32 pointer to a string in linear
    /// memory holding the exception message. 0 = no message. Only emitted
    /// when both needs_errors and needs_strings are true (messages need the
    /// string allocator).
    err_msg_global_idx: u32,
    /// #358: index of the `__ovf` global — i32 exactness flag, set to 1 by
    /// any checked integer operation whose exact result cannot be
    /// represented in i64 (or whose WASM lowering would be inexact, e.g. a
    /// negative shift count). Always emitted and exported; the JS glue
    /// checks it after every call and transparently re-runs the call on the
    /// exact JS (BigInt) twin — or throws where no twin is available.
    ovf_global_idx: u32,
    /// User-defined exception classes → assigned error codes (Step 5: custom
    /// exceptions). Codes start at 100; built-ins occupy 1-7. The bridge
    /// surfaces these as `Error.name` exactly matching the class name.
    custom_exceptions: BTreeMap<String, i32>,
    /// Whether any compiled function uses dict operations. When true, the
    /// bridge generates a `__dict` import namespace with `new`, `get_*`,
    /// `set_*`, `has_*`, `del_*`, `len`, etc.
    needs_dicts: bool,
    /// Lambdas synthesized during emit_module. Each lambda becomes a
    /// top-level WASM function added to a `funcref` table for `call_indirect`.
    lambdas: Vec<LambdaInfo>,
    /// Map from closure signature to its type-section index. Used by
    /// `call_indirect` to declare the expected signature.
    closure_type_indices: HashMap<ClosureSig, u32>,
    /// Counter incremented each time we emit a `Lambda` expression. Walks
    /// the AST in the same order as the lambda-collection pass, so the
    /// counter values match `self.lambdas` indices.
    next_lambda_emit_idx: Cell<u32>,
}

/// Compile-time info about a synthesized lambda function.
#[derive(Debug, Clone)]
struct LambdaInfo {
    /// Parameter (name, type) pairs (user-visible — env_ptr is implicit).
    params: Vec<(String, WasmType)>,
    /// Return type. None = void.
    return_type: Option<WasmType>,
    /// Body — a single expression.
    body: Expr,
    /// WASM function index assigned to this lambda (set during emit_module).
    func_idx: u32,
    /// User-visible closure signature for `call_indirect` type lookup.
    sig: ClosureSig,
    /// Free-variable captures: (name, type, offset-in-env-bytes). Populated
    /// by `collect_captures_for_lambda` during emit_module.
    captures: Vec<CaptureInfo>,
}

/// One captured free-variable in a lambda. The lambda body loads `name`
/// from `env_ptr + offset` with a load instruction matching `ty`.
#[derive(Debug, Clone)]
struct CaptureInfo {
    name: String,
    ty: WasmType,
    offset: u32,
}

/// Closure signature: (params, return). Used as a HashMap key to dedup
/// type-section entries for `call_indirect`.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct ClosureSig {
    params: Vec<WasmType>,
    ret: Option<WasmType>,
}

/// B1: the branch targets of ONE enclosing loop, recorded as ABSOLUTE label
/// indices (the value of `FuncContext::block_depth` at the instant the label
/// was opened). `break`/`continue` compute their `br` RELATIVE depth from the
/// current `block_depth` at the branch site, so they are correct under any
/// number of intervening structured blocks (`if`, `try`, nested loops) — the
/// old hardcoded `Br(1)`/`Br(0)` were only correct directly in the loop body.
#[derive(Debug, Clone, Copy)]
struct LoopLabels {
    /// The `block` wrapping the loop — `break` branches to its end.
    break_abs: u32,
    /// The label `continue` branches to: the `loop` header for `while`
    /// (re-tests the condition), or the body-wrapping `block` for `for`
    /// (falls through to the increment, then back to the header).
    continue_abs: u32,
}

/// Per-function compilation context.
struct FuncContext {
    /// Local variables: name â†’ (local index, WASM type)
    locals: HashMap<String, (u32, WasmType)>,
    /// Next available local index
    next_local: u32,
    /// Return type (None for void functions)
    return_type: Option<WasmType>,
    /// B1: number of currently-open structured labels (block/loop/if) that
    /// enclose the current emission point INSIDE the function body. Every
    /// statement-level structured-block emitter must bracket its labels with
    /// `push_label`/`pop_label` so branch depths computed against this are
    /// correct by construction. (Expression-internal blocks that open and
    /// close without emitting statements inside need not be tracked.)
    block_depth: u32,
    /// B1: enclosing-loop label stack (innermost last) for break/continue.
    loop_labels: Vec<LoopLabels>,
    /// Pre-allocated i32 temp locals for string operation internals (8 locals)
    str_temps: Vec<u32>,
    /// Pre-allocated i32 save locals for nested string expression results (4 locals)
    str_saves: Vec<u32>,
    /// Current nesting depth for string save slots
    str_depth: usize,
    /// Pre-allocated i32 scratch PAIRS (list ptr, index) for list
    /// subscript reads, indexed by `sub_depth` — nested reads
    /// (`rh[ir[k]]`) each take their own pair. A single fixed pair
    /// was the Livermore k14 silent-miscompile (clobbered container).
    sub_scratch: Vec<(u32, u32)>,
    /// CVE-2026-15903 fix (F3/F4): pre-allocated i64 scratch PAIRS
    /// (normalized-index, length) for the UNCONDITIONAL list-subscript bounds
    /// check, indexed by `sub_depth` in lockstep with `sub_scratch`. The check
    /// must test the FULL i64 index BEFORE the `i32.wrap_i64` narrowing, so it
    /// needs an i64 home for the (negative-normalized) index and the length;
    /// nested reads each take their own pair for the same reason `sub_scratch`
    /// is per-depth.
    sub_scratch_i64: Vec<(u32, u32)>,
    /// Current list-subscript-read nesting depth.
    sub_depth: usize,
    /// Nesting depth of `try` blocks. When > 0, `raise` only sets __err_code
    /// (it does not return) so that the surrounding try's dispatch can catch.
    try_depth: u32,
    /// Tier 6: when emitting a lambda function body, holds the captures so
    /// `emit_expr` for a Name pointing to a captured variable knows to load
    /// it from `env_ptr` (implicit local 0) at the recorded offset.
    /// Empty for non-lambda functions.
    captures: Vec<CaptureInfo>,
    /// #358: pre-allocated i64 scratch locals (4) for overflow-checked
    /// integer arithmetic (add/sub/mul/shl/shr/floordiv/mod). Only used
    /// AFTER both operands are fully on the stack, so nesting is safe.
    ck_i64: Vec<u32>,
    /// #358: pre-allocated i64 scratch locals (3: base/exp/acc) for the
    /// exact integer-pow loop. Separate from `ck_i64` because the pow loop
    /// invokes the checked-mul sequence (which uses `ck_i64`) internally.
    pw_i64: Vec<u32>,
    /// #358: pre-allocated f64 scratch locals (2) for on-stack float
    /// mod/floordiv (removes the old re-emit-the-operand double-eval).
    ck_f64: Vec<u32>,
}

impl FuncContext {
    /// B1: record that a structured label (block/loop/if) is being opened.
    /// Returns the label's ABSOLUTE index, to be stored and later resolved
    /// against the current depth via `br_depth_to`.
    fn push_label(&mut self) -> u32 {
        let abs = self.block_depth;
        self.block_depth += 1;
        abs
    }

    /// B1: record that the most recently opened structured label is closed
    /// (its `end` was emitted).
    fn pop_label(&mut self) {
        debug_assert!(self.block_depth > 0, "pop_label underflow");
        self.block_depth -= 1;
    }

    /// B1: relative `br` depth from the current emission point to the label
    /// with absolute index `abs` (which must still be open).
    fn br_depth_to(&self, abs: u32) -> u32 {
        debug_assert!(
            abs < self.block_depth,
            "br target label {} is not open (depth {})",
            abs,
            self.block_depth
        );
        self.block_depth - 1 - abs
    }

    fn get_or_alloc_local(&mut self, name: &str, ty: WasmType) -> u32 {
        if let Some(&(idx, _)) = self.locals.get(name) {
            return idx;
        }
        let idx = self.next_local;
        self.next_local += 1;
        self.locals.insert(name.to_string(), (idx, ty));
        idx
    }

    fn get_local(&self, name: &str) -> Option<(u32, WasmType)> {
        self.locals.get(name).cloned()
    }

    /// Every scratch-pool local index (str temps/saves, subscript pairs,
    /// overflow-check i64/f64), flattened.
    fn scratch_pool_indices(&self) -> Vec<u32> {
        let mut s: Vec<u32> = Vec::new();
        s.extend(self.str_temps.iter().copied());
        s.extend(self.str_saves.iter().copied());
        for (l, r) in &self.sub_scratch {
            s.push(*l);
            s.push(*r);
        }
        for (n, l) in &self.sub_scratch_i64 {
            s.push(*n);
            s.push(*l);
        }
        s.extend(self.ck_i64.iter().copied());
        s.extend(self.pw_i64.iter().copied());
        s.extend(self.ck_f64.iter().copied());
        s
    }

    /// **Scratch non-interference check** (validate-a-posteriori, CompCert
    /// CACM §4.3 applied to scratch allocation). The name→index map must be
    /// INJECTIVE: two names sharing a local index means one write clobbers a
    /// value the other is still live on — the Livermore k14 silent-miscompile
    /// class (nested `rh[ir[k]]` reads sharing a scratch pair) and its
    /// `env_ptr`-clobber variant (a lambda's scratch falling back to local 0,
    /// the old `unwrap_or(0)` default). Depth-indexed pools already make this
    /// hold by construction; this VALIDATES it so a future refactor cannot
    /// silently reintroduce aliasing. Returns human-readable violations
    /// (empty = non-interfering).
    fn scratch_non_interference_violations(&self, is_lambda: bool) -> Vec<String> {
        scratch_interference_violations(&self.locals, &self.scratch_pool_indices(), is_lambda)
    }
}

/// Free core of the non-interference check (extracted for direct unit testing
/// without building a whole `FuncContext`). See
/// [`FuncContext::scratch_non_interference_violations`].
fn scratch_interference_violations(
    locals: &HashMap<String, (u32, WasmType)>,
    scratch: &[u32],
    is_lambda: bool,
) -> Vec<String> {
    let mut violations = Vec::new();
    // Injectivity: no local index may be bound to two names.
    let mut seen: HashMap<u32, &str> = HashMap::new();
    for (name, (idx, _)) in locals {
        if let Some(prev) = seen.insert(*idx, name.as_str()) {
            violations.push(format!(
                "local index {idx} aliased by `{prev}` and `{name}` — scratch interference"
            ));
        }
    }
    // The env_ptr variant: in a lambda body local 0 is the implicit env
    // pointer; no scratch slot may land on it.
    if is_lambda && scratch.contains(&0) {
        violations.push("a scratch local aliases env_ptr (local 0) in a lambda body".to_string());
    }
    violations
}

impl Default for WasmEmitter {
    fn default() -> Self {
        Self::new()
    }
}

impl WasmEmitter {
    pub fn new() -> Self {
        Self {
            func_indices: HashMap::new(),
            func_info: HashMap::new(),
            math_imports: BTreeSet::new(),
            math_aliases: HashMap::new(),
            import_indices: HashMap::new(),
            string_pool: Vec::new(),
            string_dedup: HashMap::new(),
            data_section_size: 0,
            needs_strings: false,
            alloc_func_idx: 0,
            needs_errors: false,
            err_code_global_idx: 0,
            err_msg_global_idx: 0,
            ovf_global_idx: 0,
            custom_exceptions: BTreeMap::new(),
            needs_dicts: false,
            lambdas: Vec::new(),
            closure_type_indices: HashMap::new(),
            next_lambda_emit_idx: Cell::new(0),
        }
    }

    /// True if any lambda was collected during emit_module (used by lib.rs).
    pub fn has_lambdas(&self) -> bool {
        !self.lambdas.is_empty()
    }

    /// Whether the module uses dict operations.
    pub fn needs_dicts(&self) -> bool {
        self.needs_dicts
    }

    /// Whether the module emitted the heap (bump allocator + `__heap_ptr`
    /// global + `__alloc`/`memory` exports). True for string OR collection
    /// params/returns/bodies. The bridge uses this to know the heap exists
    /// (for the arena reset around each call — B-034) and to emit marshallers.
    pub fn needs_strings(&self) -> bool {
        self.needs_strings
    }

    /// Custom exception classes detected during emit_module, sorted by name.
    pub fn custom_exceptions(&self) -> &BTreeMap<String, i32> {
        &self.custom_exceptions
    }

    /// Recursively walk a function body collecting `name → type` for any
    /// statically-typeable local. We pick up:
    ///   - AnnAssign with annotation
    ///   - Simple Assign where the RHS is a literal we can type
    fn collect_typed_locals(body: &[Stmt], scope: &mut HashMap<String, WasmType>) {
        for s in body {
            Self::collect_typed_locals_in_stmt(s, scope);
        }
    }

    fn collect_typed_locals_in_stmt(stmt: &Stmt, scope: &mut HashMap<String, WasmType>) {
        match &stmt.kind {
            StmtKind::AnnAssign {
                target, annotation, ..
            } => {
                if let ExprKind::Name(name) = &target.kind {
                    let ty = resolve_type(annotation);
                    if let Some(wt) = to_wasm_type(&ty) {
                        scope.insert(name.clone(), wt);
                    }
                }
            }
            StmtKind::Assign { targets, value } => {
                for t in targets {
                    if let ExprKind::Name(name) = &t.kind {
                        // Infer literal RHS. List/Tuple/Dict literals
                        // give the HoF-arg-from-context inference enough
                        // information to type lambda parameters at
                        // call sites like `reduce(lambda a, b: ..., nums, 0.0)`.
                        let inferred = match &value.kind {
                            ExprKind::IntLiteral(_) => Some(WasmType::I64),
                            ExprKind::FloatLiteral(_) => Some(WasmType::F64),
                            ExprKind::BoolLiteral(_) => Some(WasmType::I32),
                            ExprKind::StringLiteral(_) | ExprKind::FString { .. } => {
                                Some(WasmType::Ptr)
                            }
                            ExprKind::List(elts) => {
                                // Element type from the first literal element.
                                let inner = elts
                                    .first()
                                    .and_then(|e| match &e.kind {
                                        ExprKind::IntLiteral(_) => Some(WasmType::I64),
                                        ExprKind::FloatLiteral(_) => Some(WasmType::F64),
                                        ExprKind::BoolLiteral(_) => Some(WasmType::I32),
                                        ExprKind::StringLiteral(_) => Some(WasmType::Ptr),
                                        _ => None,
                                    })
                                    .unwrap_or(WasmType::I64);
                                Some(WasmType::PtrList(Box::new(inner)))
                            }
                            // `[0.0] * n` / `n * [0.0]` — preallocated
                            // list: element type from the literal side, so
                            // container names are in scope for the
                            // subscript-read inference below.
                            ExprKind::BinOp {
                                left,
                                op: BinOp::Mul,
                                right,
                            } => {
                                let lit = match (&left.kind, &right.kind) {
                                    (ExprKind::List(elts), _) => Some(elts),
                                    (_, ExprKind::List(elts)) => Some(elts),
                                    _ => None,
                                };
                                lit.and_then(|elts| {
                                    elts.first().and_then(|e| match &e.kind {
                                        ExprKind::IntLiteral(_) => Some(WasmType::I64),
                                        ExprKind::FloatLiteral(_) => Some(WasmType::F64),
                                        ExprKind::BoolLiteral(_) => Some(WasmType::I32),
                                        _ => None,
                                    })
                                })
                                .map(|inner| WasmType::PtrList(Box::new(inner)))
                            }
                            // Livermore finding (2026-07-10): a local whose
                            // first assignment is a list-subscript READ
                            // (`a = x[k]`) must take the ELEMENT type —
                            // previously untyped, it defaulted to i64 and
                            // produced invalid WASM ("local.set expected
                            // i64, found f64.load") in 8/24 LFK kernels.
                            ExprKind::Subscript {
                                value: container,
                                index,
                                ..
                            } if !matches!(index.kind, ExprKind::Slice { .. }) => {
                                match Self::infer_type_in_scope(container, scope) {
                                    WasmType::PtrList(inner) => Some((*inner).clone()),
                                    _ => None,
                                }
                            }
                            _ => None,
                        };
                        if let Some(wt) = inferred {
                            scope.insert(name.clone(), wt);
                        }
                    }
                }
            }
            StmtKind::If {
                body,
                elif_clauses,
                else_body,
                ..
            } => {
                Self::collect_typed_locals(body, scope);
                for (_, b) in elif_clauses {
                    Self::collect_typed_locals(b, scope);
                }
                if let Some(b) = else_body {
                    Self::collect_typed_locals(b, scope);
                }
            }
            // Loop-`else` (B1 family): the else body is real emitted code —
            // every walker must traverse it (a dropped else once meant
            // admitted-but-never-emitted statements).
            StmtKind::While {
                body, else_body, ..
            }
            | StmtKind::For {
                body, else_body, ..
            } => {
                Self::collect_typed_locals(body, scope);
                if let Some(b) = else_body {
                    Self::collect_typed_locals(b, scope);
                }
            }
            _ => {}
        }
    }

    /// Top-level lambda collection driver that knows the enclosing scope.
    /// For each Lambda found, walks its body for free variables and resolves
    /// their types from `scope`.
    fn collect_lambdas_with_scope(
        body: &[Stmt],
        scope: &HashMap<String, WasmType>,
        out: &mut Vec<LambdaInfo>,
    ) {
        for s in body {
            Self::collect_lambdas_in_stmt_scoped(s, scope, out);
        }
    }

    fn collect_lambdas_in_stmt_scoped(
        stmt: &Stmt,
        scope: &HashMap<String, WasmType>,
        out: &mut Vec<LambdaInfo>,
    ) {
        match &stmt.kind {
            StmtKind::Expr(e) => Self::collect_lambdas_in_expr_scoped(e, scope, out),
            StmtKind::Assign { value, .. } => {
                Self::collect_lambdas_in_expr_scoped(value, scope, out)
            }
            StmtKind::AugAssign { value, .. } => {
                Self::collect_lambdas_in_expr_scoped(value, scope, out)
            }
            StmtKind::AnnAssign { value: Some(v), .. } => {
                Self::collect_lambdas_in_expr_scoped(v, scope, out)
            }
            StmtKind::Return(Some(v)) => Self::collect_lambdas_in_expr_scoped(v, scope, out),
            StmtKind::If {
                test,
                body,
                elif_clauses,
                else_body,
            } => {
                Self::collect_lambdas_in_expr_scoped(test, scope, out);
                Self::collect_lambdas_with_scope(body, scope, out);
                for (t, b) in elif_clauses {
                    Self::collect_lambdas_in_expr_scoped(t, scope, out);
                    Self::collect_lambdas_with_scope(b, scope, out);
                }
                if let Some(b) = else_body {
                    Self::collect_lambdas_with_scope(b, scope, out);
                }
            }
            StmtKind::While {
                test,
                body,
                else_body,
            } => {
                Self::collect_lambdas_in_expr_scoped(test, scope, out);
                Self::collect_lambdas_with_scope(body, scope, out);
                if let Some(b) = else_body {
                    Self::collect_lambdas_with_scope(b, scope, out);
                }
            }
            StmtKind::For {
                iter,
                body,
                else_body,
                ..
            } => {
                Self::collect_lambdas_in_expr_scoped(iter, scope, out);
                Self::collect_lambdas_with_scope(body, scope, out);
                if let Some(b) = else_body {
                    Self::collect_lambdas_with_scope(b, scope, out);
                }
            }
            _ => {}
        }
    }

    /// Static type-of-expr helper for use inside `collect_lambdas_*`,
    /// which doesn't have access to `&self`. Handles literals and
    /// Names (looking up the enclosing scope). Falls back to `I64` for
    /// anything else — the caller uses this only to seed unannotated
    /// lambda parameters with a context-derived type, so a loose
    /// fallback is acceptable: the worst case is the existing default.
    fn infer_type_in_scope(expr: &Expr, scope: &HashMap<String, WasmType>) -> WasmType {
        match &expr.kind {
            ExprKind::IntLiteral(_) => WasmType::I64,
            ExprKind::FloatLiteral(_) => WasmType::F64,
            ExprKind::BoolLiteral(_) => WasmType::I32,
            ExprKind::StringLiteral(_) | ExprKind::FString { .. } => WasmType::Ptr,
            ExprKind::Name(n) => scope.get(n).cloned().unwrap_or(WasmType::I64),
            ExprKind::List(elts) => {
                let inner = elts
                    .first()
                    .map(|e| Self::infer_type_in_scope(e, scope))
                    .unwrap_or(WasmType::I64);
                WasmType::PtrList(Box::new(inner))
            }
            _ => WasmType::I64,
        }
    }

    /// Given an outer expression that may be a HoF call (`reduce`,
    /// `map`, `filter`, `sorted`), return per-position overrides for
    /// an inner Lambda's parameter types. The returned vector matches
    /// the lambda's parameter count; positions whose type can't be
    /// inferred map to `None` and fall back to the lambda's annotation
    /// (or `I64`).
    ///
    /// Examples:
    /// * `reduce(lambda a, b: ..., lst, init)` → `[Some(init_ty), Some(lst_elem_ty)]`
    /// * `map(lambda x: ..., lst)` → `[Some(lst_elem_ty)]`
    /// * `sorted(lst, key=lambda x: ...)` — `key=` is a kwarg, not yet
    ///   threaded through this path.
    fn hof_lambda_param_overrides(
        hof_call: &Expr,
        scope: &HashMap<String, WasmType>,
    ) -> Option<(usize, Vec<Option<WasmType>>)> {
        let ExprKind::Call { func, args, .. } = &hof_call.kind else {
            return None;
        };
        let ExprKind::Name(name) = &func.kind else {
            return None;
        };
        match name.as_str() {
            "reduce" if args.len() >= 3 => {
                // reduce(fn, lst, init): fn(acc, elem)
                // acc type = init type; elem type = lst element type.
                let init_ty = Self::infer_type_in_scope(&args[2], scope);
                let elem_ty = match Self::infer_type_in_scope(&args[1], scope) {
                    WasmType::PtrList(inner) => (*inner).clone(),
                    _ => return None,
                };
                Some((0, vec![Some(init_ty), Some(elem_ty)]))
            }
            "map" | "filter" if args.len() >= 2 => {
                let elem_ty = match Self::infer_type_in_scope(&args[1], scope) {
                    WasmType::PtrList(inner) => (*inner).clone(),
                    _ => return None,
                };
                Some((0, vec![Some(elem_ty)]))
            }
            _ => None,
        }
    }

    fn collect_lambdas_in_expr_scoped(
        expr: &Expr,
        scope: &HashMap<String, WasmType>,
        out: &mut Vec<LambdaInfo>,
    ) {
        Self::collect_lambdas_in_expr_scoped_with_overrides(expr, scope, out, &[]);
    }

    fn collect_lambdas_in_expr_scoped_with_overrides(
        expr: &Expr,
        scope: &HashMap<String, WasmType>,
        out: &mut Vec<LambdaInfo>,
        param_overrides: &[Option<WasmType>],
    ) {
        match &expr.kind {
            ExprKind::Lambda { params, body } => {
                let wparams: Vec<(String, WasmType)> = params
                    .iter()
                    .enumerate()
                    .map(|(i, p)| {
                        // Precedence: explicit annotation > HoF override > I64 fallback.
                        let annotated = p.annotation.as_ref().map(|a| resolve_type(a));
                        let ty = match (annotated, param_overrides.get(i).and_then(|o| o.clone())) {
                            (Some(ty), _) => to_wasm_type(&ty).unwrap_or(WasmType::I64),
                            (None, Some(override_ty)) => override_ty,
                            (None, None) => WasmType::I64,
                        };
                        (p.name.clone(), ty)
                    })
                    .collect();
                let ret_wty = Self::infer_lambda_return_type(body, &wparams);
                let sig = ClosureSig {
                    params: wparams.iter().map(|(_, t)| t.clone()).collect(),
                    ret: ret_wty.clone(),
                };
                // Find free variables: Names in body that aren't lambda params
                // and aren't top-level user functions (we approximate that
                // here by checking against the enclosing scope).
                let mut free_names: Vec<String> = Vec::new();
                let param_names: Vec<&str> = wparams.iter().map(|(n, _)| n.as_str()).collect();
                Self::collect_free_names(body, &param_names, &mut free_names);
                // Resolve captures by looking up types in the enclosing scope.
                let mut captures: Vec<CaptureInfo> = Vec::new();
                let mut offset: u32 = 0;
                for n in &free_names {
                    if let Some(ty) = scope.get(n) {
                        captures.push(CaptureInfo {
                            name: n.clone(),
                            ty: ty.clone(),
                            offset,
                        });
                        offset += ty.size_bytes();
                    }
                }
                out.push(LambdaInfo {
                    params: wparams,
                    return_type: ret_wty,
                    body: (**body).clone(),
                    func_idx: 0,
                    sig,
                    captures,
                });
                // The lambda body may itself contain HoF calls.
                Self::collect_lambdas_in_expr_scoped(body, scope, out);
            }
            ExprKind::BinOp { left, right, .. } => {
                Self::collect_lambdas_in_expr_scoped(left, scope, out);
                Self::collect_lambdas_in_expr_scoped(right, scope, out);
            }
            ExprKind::UnaryOp { operand, .. } => {
                Self::collect_lambdas_in_expr_scoped(operand, scope, out);
            }
            ExprKind::Call { func, args, .. } => {
                Self::collect_lambdas_in_expr_scoped(func, scope, out);
                // HoF arg-position override: when this call is a
                // recognized higher-order function (`reduce`/`map`/
                // `filter`), propagate inferred lambda-param types to
                // the lambda at the documented arg position.
                let hof_override = Self::hof_lambda_param_overrides(expr, scope);
                for (i, a) in args.iter().enumerate() {
                    let overrides = match &hof_override {
                        Some((pos, ovs)) if *pos == i => ovs.as_slice(),
                        _ => &[][..],
                    };
                    Self::collect_lambdas_in_expr_scoped_with_overrides(a, scope, out, overrides);
                }
            }
            ExprKind::IfExpr {
                test,
                body,
                else_body,
            } => {
                Self::collect_lambdas_in_expr_scoped(test, scope, out);
                Self::collect_lambdas_in_expr_scoped(body, scope, out);
                Self::collect_lambdas_in_expr_scoped(else_body, scope, out);
            }
            ExprKind::Compare { left, comparisons } => {
                Self::collect_lambdas_in_expr_scoped(left, scope, out);
                for (_, e) in comparisons {
                    Self::collect_lambdas_in_expr_scoped(e, scope, out);
                }
            }
            ExprKind::Subscript { value, index, .. } => {
                Self::collect_lambdas_in_expr_scoped(value, scope, out);
                Self::collect_lambdas_in_expr_scoped(index, scope, out);
            }
            ExprKind::Tuple(elts) | ExprKind::List(elts) => {
                for e in elts {
                    Self::collect_lambdas_in_expr_scoped(e, scope, out);
                }
            }
            _ => {}
        }
    }

    /// Walk an expression tree collecting Names that are NOT in `params` and
    /// that aren't well-known builtins. The result is best-effort: callers
    /// filter further based on enclosing-scope types.
    fn collect_free_names(expr: &Expr, params: &[&str], out: &mut Vec<String>) {
        match &expr.kind {
            ExprKind::Name(n) => {
                if !params.contains(&n.as_str()) && !out.contains(n) {
                    out.push(n.clone());
                }
            }
            ExprKind::BinOp { left, right, .. } => {
                Self::collect_free_names(left, params, out);
                Self::collect_free_names(right, params, out);
            }
            ExprKind::UnaryOp { operand, .. } => {
                Self::collect_free_names(operand, params, out);
            }
            ExprKind::Call { func, args, .. } => {
                Self::collect_free_names(func, params, out);
                for a in args {
                    Self::collect_free_names(a, params, out);
                }
            }
            ExprKind::Compare { left, comparisons } => {
                Self::collect_free_names(left, params, out);
                for (_, e) in comparisons {
                    Self::collect_free_names(e, params, out);
                }
            }
            ExprKind::IfExpr {
                test,
                body,
                else_body,
            } => {
                Self::collect_free_names(test, params, out);
                Self::collect_free_names(body, params, out);
                Self::collect_free_names(else_body, params, out);
            }
            ExprKind::Subscript { value, index, .. } => {
                Self::collect_free_names(value, params, out);
                Self::collect_free_names(index, params, out);
            }
            ExprKind::Tuple(elts) | ExprKind::List(elts) => {
                for e in elts {
                    Self::collect_free_names(e, params, out);
                }
            }
            ExprKind::Lambda {
                params: inner_params,
                body,
            } => {
                // Inner lambda — its params shadow ours.
                let mut combined: Vec<&str> = params.to_vec();
                for p in inner_params {
                    combined.push(p.name.as_str());
                }
                Self::collect_free_names(body, &combined, out);
            }
            _ => {}
        }
    }

    /// Recursively collect Lambda expressions from a statement body.
    /// Each lambda is appended to `out` with its inferred signature and a
    /// placeholder `func_idx: 0` (assigned later in emit_module).
    ///
    /// Kept as a no-scope reference variant; the active codegen path uses
    /// `collect_lambdas_in_stmt_scoped` (with capture analysis).
    #[allow(dead_code)]
    fn collect_lambdas_in_stmts(body: &[Stmt], out: &mut Vec<LambdaInfo>) {
        for s in body {
            Self::collect_lambdas_in_stmt(s, out);
        }
    }

    #[allow(dead_code)]
    fn collect_lambdas_in_stmt(stmt: &Stmt, out: &mut Vec<LambdaInfo>) {
        match &stmt.kind {
            StmtKind::Expr(e) => Self::collect_lambdas_in_expr(e, out),
            StmtKind::Assign { value, .. } => Self::collect_lambdas_in_expr(value, out),
            StmtKind::AugAssign { value, .. } => Self::collect_lambdas_in_expr(value, out),
            StmtKind::AnnAssign { value: Some(v), .. } => Self::collect_lambdas_in_expr(v, out),
            StmtKind::Return(Some(v)) => Self::collect_lambdas_in_expr(v, out),
            StmtKind::If {
                test,
                body,
                elif_clauses,
                else_body,
            } => {
                Self::collect_lambdas_in_expr(test, out);
                Self::collect_lambdas_in_stmts(body, out);
                for (t, b) in elif_clauses {
                    Self::collect_lambdas_in_expr(t, out);
                    Self::collect_lambdas_in_stmts(b, out);
                }
                if let Some(b) = else_body {
                    Self::collect_lambdas_in_stmts(b, out);
                }
            }
            StmtKind::While {
                test,
                body,
                else_body,
            } => {
                Self::collect_lambdas_in_expr(test, out);
                Self::collect_lambdas_in_stmts(body, out);
                if let Some(b) = else_body {
                    Self::collect_lambdas_in_stmts(b, out);
                }
            }
            StmtKind::For {
                iter,
                body,
                else_body,
                ..
            } => {
                Self::collect_lambdas_in_expr(iter, out);
                Self::collect_lambdas_in_stmts(body, out);
                if let Some(b) = else_body {
                    Self::collect_lambdas_in_stmts(b, out);
                }
            }
            _ => {}
        }
    }

    #[allow(dead_code)]
    fn collect_lambdas_in_expr(expr: &Expr, out: &mut Vec<LambdaInfo>) {
        match &expr.kind {
            ExprKind::Lambda { params, body } => {
                // Infer parameter types from annotations (default i64).
                let wparams: Vec<(String, WasmType)> = params
                    .iter()
                    .map(|p| {
                        let ty = p
                            .annotation
                            .as_ref()
                            .map(|a| resolve_type(a))
                            .unwrap_or(Type::Int);
                        (p.name.clone(), to_wasm_type(&ty).unwrap_or(WasmType::I64))
                    })
                    .collect();
                // Infer return type from body — best effort. We use a simple
                // walk with no scope info; for body expressions referencing
                // params, we look up their type from `wparams`.
                let ret_wty = Self::infer_lambda_return_type(body, &wparams);
                let sig = ClosureSig {
                    params: wparams.iter().map(|(_, t)| t.clone()).collect(),
                    ret: ret_wty.clone(),
                };
                out.push(LambdaInfo {
                    params: wparams,
                    return_type: ret_wty,
                    body: (**body).clone(),
                    func_idx: 0,
                    sig,
                    captures: Vec::new(),
                });
                // Recurse into body to catch nested lambdas (rare).
                Self::collect_lambdas_in_expr(body, out);
            }
            ExprKind::BinOp { left, right, .. } => {
                Self::collect_lambdas_in_expr(left, out);
                Self::collect_lambdas_in_expr(right, out);
            }
            ExprKind::UnaryOp { operand, .. } => {
                Self::collect_lambdas_in_expr(operand, out);
            }
            ExprKind::Call { func, args, .. } => {
                Self::collect_lambdas_in_expr(func, out);
                for a in args {
                    Self::collect_lambdas_in_expr(a, out);
                }
            }
            ExprKind::IfExpr {
                test,
                body,
                else_body,
            } => {
                Self::collect_lambdas_in_expr(test, out);
                Self::collect_lambdas_in_expr(body, out);
                Self::collect_lambdas_in_expr(else_body, out);
            }
            ExprKind::Compare { left, comparisons } => {
                Self::collect_lambdas_in_expr(left, out);
                for (_, e) in comparisons {
                    Self::collect_lambdas_in_expr(e, out);
                }
            }
            ExprKind::Subscript { value, index, .. } => {
                Self::collect_lambdas_in_expr(value, out);
                Self::collect_lambdas_in_expr(index, out);
            }
            ExprKind::Tuple(elts) | ExprKind::List(elts) => {
                for e in elts {
                    Self::collect_lambdas_in_expr(e, out);
                }
            }
            _ => {}
        }
    }

    /// Best-effort return-type inference for a lambda body.
    /// Knows about param references (looked up in `params`) and basic
    /// arithmetic. Falls back to `I64` for the unknown.
    fn infer_lambda_return_type(body: &Expr, params: &[(String, WasmType)]) -> Option<WasmType> {
        Some(match &body.kind {
            ExprKind::IntLiteral(_) => WasmType::I64,
            ExprKind::FloatLiteral(_) => WasmType::F64,
            ExprKind::BoolLiteral(_) => WasmType::I32,
            ExprKind::StringLiteral(_) | ExprKind::FString { .. } => WasmType::Ptr,
            ExprKind::Name(n) => params
                .iter()
                .find(|(p, _)| p == n)
                .map(|(_, t)| t.clone())
                .unwrap_or(WasmType::I64),
            ExprKind::BinOp { left, op, right } => {
                let lt = Self::infer_lambda_return_type(left, params).unwrap_or(WasmType::I64);
                let rt = Self::infer_lambda_return_type(right, params).unwrap_or(WasmType::I64);
                match op {
                    BinOp::Div => WasmType::F64,
                    // #358: int ** int is exact i64; float operands → f64.
                    BinOp::Pow => {
                        if lt == WasmType::F64 || rt == WasmType::F64 {
                            WasmType::F64
                        } else {
                            WasmType::I64
                        }
                    }
                    BinOp::Eq
                    | BinOp::NotEq
                    | BinOp::Lt
                    | BinOp::LtEq
                    | BinOp::Gt
                    | BinOp::GtEq
                    | BinOp::And
                    | BinOp::Or => WasmType::I32,
                    _ => {
                        if lt == WasmType::F64 || rt == WasmType::F64 {
                            WasmType::F64
                        } else {
                            lt
                        }
                    }
                }
            }
            ExprKind::Compare { .. } => WasmType::I32,
            ExprKind::IfExpr { body, .. } => {
                Self::infer_lambda_return_type(body, params).unwrap_or(WasmType::I64)
            }
            _ => WasmType::I64,
        })
    }

    /// Math imports detected during emit_module, sorted for deterministic output.
    pub fn math_imports(&self) -> &BTreeSet<String> {
        &self.math_imports
    }

    /// Whether any eligible function uses raise/assert/try.
    pub fn needs_errors(&self) -> bool {
        self.needs_errors
    }

    /// Emit a complete WASM module from eligible functions.
    pub fn emit_module(&mut self, module: &Module, analysis: &WasmAnalysis) -> Vec<u8> {
        self.func_info = analysis.eligible.clone();

        // Collect names bound by `from math import X [as Y]` so bare calls
        // (`sqrt(x)`) dispatch to the math import the same as `math.sqrt(x)`.
        // Module-scope imports only (we scan `module.body`, not function bodies):
        // a `from math import` nested inside a function body is not rebound here,
        // matching the `math.X` attribute path which likewise assumes a
        // module-level `import math`.
        self.math_aliases.clear();
        for stmt in &module.body {
            if let StmtKind::ImportFrom {
                module: m, names, ..
            } = &stmt.kind
            {
                if m == "math" {
                    for alias in names {
                        if math_function_arity(&alias.name).is_some() {
                            let bound = alias.alias.clone().unwrap_or_else(|| alias.name.clone());
                            self.math_aliases.insert(bound, alias.name.clone());
                        }
                    }
                }
            }
        }

        // Collect math imports across all eligible bodies.
        // ** (pow) operator is treated as a math.pow call here.
        self.math_imports.clear();
        self.import_indices.clear();
        for info in analysis.eligible.values() {
            if let StmtKind::FuncDef { body, .. } = &module.body[info.stmt_index].kind {
                collect_math_imports(body, &mut self.math_imports, &self.math_aliases);
            }
        }

        // Collect string literals and detect string usage
        self.string_pool.clear();
        self.string_dedup.clear();
        self.data_section_size = 0;
        self.collect_string_literals(module, analysis);

        // Detect needs_strings: any eligible function has str params/return or
        // body uses strings. The flag is really "needs the bump-allocator +
        // scratch temps" — collection params/returns (list/dict/tuple/closure)
        // need both, even when the body never constructs a collection literal:
        //   * the heap (`__alloc` + memory) so the JS glue can marshal a JS
        //     array/dict into linear memory (B-031), and
        //   * the per-function scratch temps used by list subscript (B-032 —
        //     without them `lst[i]` falls back to local 0, clobbering the list
        //     pointer and reading address 8 → wrong result / 0).
        // `to_wasm_type(..).is_any_ptr()` captures every heap/pointer boundary
        // type (string, list, set, dict, tuple, closure) in one predicate.
        let is_heap_boundary = |ty: &Type| to_wasm_type(ty).is_some_and(|w| w.is_any_ptr());
        self.needs_strings = !self.string_pool.is_empty()
            || analysis.eligible.values().any(|info| {
                info.params.iter().any(|(_, ty)| is_heap_boundary(ty))
                    || is_heap_boundary(&info.return_type)
                    || if let StmtKind::FuncDef { body, .. } = &module.body[info.stmt_index].kind {
                        body_uses_strings(body)
                    } else {
                        false
                    }
            });

        // Detect needs_errors: any eligible function uses raise/assert/try
        self.needs_errors = analysis.eligible.values().any(|info| {
            if let StmtKind::FuncDef { body, .. } = &module.body[info.stmt_index].kind {
                body_uses_errors(body)
            } else {
                false
            }
        });

        // Step 5: collect custom exception classes and assign codes 100+.
        // Built-ins occupy 1-7; we leave a gap for future built-ins.
        self.custom_exceptions.clear();
        if self.needs_errors {
            let registry = class_registry(module);
            let mut next_code: i32 = 100;
            for cls in &registry {
                if !self.custom_exceptions.contains_key(&cls.name) {
                    self.custom_exceptions.insert(cls.name.clone(), next_code);
                    next_code += 1;
                }
            }
        }

        // Detect dict usage -> bring up __dict.* import namespace.
        self.needs_dicts = analysis.eligible.values().any(|info| {
            if let StmtKind::FuncDef { body, .. } = &module.body[info.stmt_index].kind {
                body_uses_dicts(body)
            } else {
                false
            }
        });

        // Tier 6: collect lambdas across all eligible bodies and resolve
        // their free-variable captures. Each lambda becomes a synthetic
        // top-level function entered into a `funcref` table for
        // `call_indirect`. Captures are typed by walking the enclosing
        // FuncDef's params and AnnAssign locals.
        self.lambdas.clear();
        self.closure_type_indices.clear();
        for info in analysis.eligible.values() {
            if let StmtKind::FuncDef { body, params, .. } = &module.body[info.stmt_index].kind {
                // Build the enclosing scope's name → type map.
                let mut scope: HashMap<String, WasmType> = HashMap::new();
                for p in params {
                    let ty = p
                        .annotation
                        .as_ref()
                        .map(|a| resolve_type(a))
                        .unwrap_or(Type::Int);
                    if let Some(wt) = to_wasm_type(&ty) {
                        scope.insert(p.name.clone(), wt);
                    }
                }
                // Walk the function body to also pick up annotated locals.
                Self::collect_typed_locals(body, &mut scope);
                Self::collect_lambdas_with_scope(body, &scope, &mut self.lambdas);
            }
        }
        self.next_lambda_emit_idx.set(0);

        // Dict imports — fixed set, alphabetical for determinism.
        let dict_imports: Vec<(&str, Vec<ValType>, Vec<ValType>)> = if self.needs_dicts {
            vec![
                ("__dict_del_str", vec![ValType::I32, ValType::I32], vec![]),
                (
                    "__dict_get_str",
                    vec![ValType::I32, ValType::I32],
                    vec![ValType::I64],
                ),
                (
                    "__dict_has_str",
                    vec![ValType::I32, ValType::I32],
                    vec![ValType::I32],
                ),
                ("__dict_len", vec![ValType::I32], vec![ValType::I32]),
                ("__dict_new", vec![], vec![ValType::I32]),
                (
                    "__dict_set_str",
                    vec![ValType::I32, ValType::I32, ValType::I64],
                    vec![],
                ),
            ]
        } else {
            vec![]
        };

        // Assign function indices: math imports -> dict imports -> __alloc -> user funcs
        let math_count = self.math_imports.len() as u32;
        let dict_count = dict_imports.len() as u32;
        let import_count: u32 = math_count + dict_count;
        let alloc_offset: u32 = if self.needs_strings { 1 } else { 0 };
        self.alloc_func_idx = import_count;

        for (i, name) in self.math_imports.iter().enumerate() {
            self.import_indices.insert(name.clone(), i as u32);
        }
        for (i, (n, _, _)) in dict_imports.iter().enumerate() {
            self.import_indices
                .insert((*n).to_string(), math_count + i as u32);
        }

        let mut sorted_funcs: Vec<&WasmFuncInfo> = analysis.eligible.values().collect();
        sorted_funcs.sort_by_key(|f| f.stmt_index);

        let user_func_count = sorted_funcs.len() as u32;
        for (i, info) in sorted_funcs.iter().enumerate() {
            self.func_indices
                .insert(info.name.clone(), import_count + alloc_offset + i as u32);
        }

        // Tier 6: assign function indices to lambdas after user functions.
        for (i, lam) in self.lambdas.iter_mut().enumerate() {
            lam.func_idx = import_count + alloc_offset + user_func_count + i as u32;
        }

        let mut wasm_module = WasmModule::new();

        // === Type Section ===
        let mut types = TypeSection::new();
        let mut type_offset = 0u32;

        // Math import types
        let mut math_type_indices: HashMap<String, u32> = HashMap::new();
        for name in &self.math_imports {
            let arity = math_function_arity(name).unwrap_or(1);
            let params = vec![ValType::F64; arity as usize];
            types.ty().function(params, vec![ValType::F64]);
            math_type_indices.insert(name.clone(), type_offset);
            type_offset += 1;
        }

        // Dict import types — one per import.
        let mut dict_type_indices: Vec<u32> = Vec::new();
        for (_, params, results) in &dict_imports {
            types.ty().function(params.clone(), results.clone());
            dict_type_indices.push(type_offset);
            type_offset += 1;
        }

        let alloc_type_idx = type_offset;
        if self.needs_strings {
            types.ty().function(vec![ValType::I32], vec![ValType::I32]);
            type_offset += 1;
        }

        let mut type_indices: Vec<u32> = Vec::new();
        for (i, info) in sorted_funcs.iter().enumerate() {
            let params: Vec<ValType> = info
                .params
                .iter()
                .map(|(_, ty)| to_wasm_type(ty).unwrap().to_val_type())
                .collect();
            let results = return_to_val_types(&info.return_type);
            types.ty().function(params, results);
            type_indices.push(type_offset + i as u32);
        }
        type_offset += sorted_funcs.len() as u32;

        // Tier 6: lambda function types. All lambdas use the uniform calling
        // convention `(env_ptr: i32, ...user_params) -> ret`. The user-visible
        // signature (without env_ptr) is what `closure_type_indices` keys on,
        // since callers want to look up by their declared closure type.
        let mut lambda_type_indices: Vec<u32> = Vec::new();
        for lam in &self.lambdas {
            // Prepend env_ptr (i32) to the params.
            let mut params: Vec<ValType> = vec![ValType::I32];
            params.extend(lam.params.iter().map(|(_, t)| t.to_val_type()));
            let results: Vec<ValType> = match &lam.return_type {
                Some(t) => vec![t.to_val_type()],
                None => vec![],
            };
            types.ty().function(params, results);
            lambda_type_indices.push(type_offset);
            self.closure_type_indices
                .insert(lam.sig.clone(), type_offset);
            type_offset += 1;
        }

        wasm_module.section(&types);

        // === Import Section ===
        if !self.math_imports.is_empty() || !dict_imports.is_empty() {
            let mut imports = ImportSection::new();
            for name in &self.math_imports {
                let type_idx = math_type_indices[name];
                imports.import(
                    "math",
                    name.as_str(),
                    wasm_encoder::EntityType::Function(type_idx),
                );
            }
            for (i, (n, _, _)) in dict_imports.iter().enumerate() {
                imports.import(
                    "__dict",
                    n,
                    wasm_encoder::EntityType::Function(dict_type_indices[i]),
                );
            }
            wasm_module.section(&imports);
        }

        // === Function Section ===
        let mut functions = FunctionSection::new();
        if self.needs_strings {
            functions.function(alloc_type_idx);
        }
        for &type_idx in &type_indices {
            functions.function(type_idx);
        }
        // Tier 6: lambda functions follow user funcs.
        for &type_idx in &lambda_type_indices {
            functions.function(type_idx);
        }
        wasm_module.section(&functions);

        // === Table Section (Tier 6: closures) ===
        if !self.lambdas.is_empty() {
            let n = self.lambdas.len() as u64;
            let mut tables = TableSection::new();
            tables.table(TableType {
                element_type: RefType::FUNCREF,
                table64: false,
                minimum: n,
                maximum: Some(n),
                shared: false,
            });
            wasm_module.section(&tables);
        }

        // === Memory Section ===
        let mut memory = MemorySection::new();
        memory.memory(MemoryType {
            minimum: 1,
            maximum: None,
            memory64: false,
            shared: false,
            page_size_log2: None,
        });
        wasm_module.section(&memory);

        // === Global Section ===
        // Globals: 0 = __heap_ptr (if needs_strings), then __err_code (if
        // needs_errors), then __err_msg (if also strings), then __ovf
        // (always — #358 i64-exactness flag; see field doc).
        {
            let mut globals = GlobalSection::new();
            let mut next_global_idx: u32 = 0;
            if self.needs_strings {
                globals.global(
                    GlobalType {
                        val_type: ValType::I32,
                        mutable: true,
                        shared: false,
                    },
                    &ConstExpr::i32_const(self.data_section_size as i32),
                );
                next_global_idx += 1;
            }
            if self.needs_errors {
                self.err_code_global_idx = next_global_idx;
                globals.global(
                    GlobalType {
                        val_type: ValType::I32,
                        mutable: true,
                        shared: false,
                    },
                    &ConstExpr::i32_const(0),
                );
                next_global_idx += 1;
                // __err_msg only when strings are also available (needed to
                // allocate / pass the message ptr).
                if self.needs_strings {
                    self.err_msg_global_idx = next_global_idx;
                    globals.global(
                        GlobalType {
                            val_type: ValType::I32,
                            mutable: true,
                            shared: false,
                        },
                        &ConstExpr::i32_const(0),
                    );
                    next_global_idx += 1;
                }
            }
            self.ovf_global_idx = next_global_idx;
            globals.global(
                GlobalType {
                    val_type: ValType::I32,
                    mutable: true,
                    shared: false,
                },
                &ConstExpr::i32_const(0),
            );
            wasm_module.section(&globals);
        }

        // === Export Section ===
        let mut exports = ExportSection::new();
        for info in &sorted_funcs {
            let idx = self.func_indices[&info.name];
            exports.export(&info.name, ExportKind::Func, idx);
        }
        exports.export("memory", ExportKind::Memory, 0);
        if self.needs_strings {
            exports.export("__alloc", ExportKind::Func, self.alloc_func_idx);
            // Export the bump pointer (global 0) so the JS glue can save it
            // before a call and restore it after — an arena/scope reset that
            // reclaims transient argument memory across repeated calls (B-034).
            exports.export("__heap_ptr", ExportKind::Global, 0);
        }
        if self.needs_errors {
            exports.export("__err_code", ExportKind::Global, self.err_code_global_idx);
            if self.needs_strings {
                exports.export("__err_msg", ExportKind::Global, self.err_msg_global_idx);
            }
        }
        // #358: exactness flag — the glue checks this after every call.
        exports.export("__ovf", ExportKind::Global, self.ovf_global_idx);
        wasm_module.section(&exports);

        // === DataCount Section (required before Code when Data section exists) ===
        let has_data = !self.string_pool.is_empty();
        if has_data {
            wasm_module.section(&DataCountSection { count: 1 });
        }

        // === Element Section (Tier 6: closures) ===
        // Populate the funcref table with lambda function indices in order.
        if !self.lambdas.is_empty() {
            let mut elements = ElementSection::new();
            let func_indices: Vec<u32> = self.lambdas.iter().map(|l| l.func_idx).collect();
            elements.active(
                Some(0),
                &ConstExpr::i32_const(0),
                Elements::Functions(func_indices.into()),
            );
            wasm_module.section(&elements);
        }

        // === Code Section ===
        let mut code = CodeSection::new();
        if self.needs_strings {
            code.function(&self.emit_alloc_function());
        }
        for info in &sorted_funcs {
            let stmt = &module.body[info.stmt_index];
            let func = self.emit_function(stmt, info);
            code.function(&func);
        }
        // Tier 6: lambda function bodies.
        // Reset the per-emit lambda counter so emit_expr's Lambda case picks
        // the same indices as the collection pass.
        let lambdas_snapshot = self.lambdas.clone();
        for lam in &lambdas_snapshot {
            let func = self.emit_lambda_function(lam);
            code.function(&func);
        }
        wasm_module.section(&code);

        // === Data Section ===
        if has_data {
            let mut data = DataSection::new();
            let offset = ConstExpr::i32_const(0);
            let data_bytes = self.build_data_section_bytes();
            data.active(0, &offset, data_bytes);
            wasm_module.section(&data);
        }

        wasm_module.finish()
    }

    /// Emit the allocation of a closure struct `[i32 func_idx][i32 env_ptr]`
    /// for the lambda at `lambda_idx` (matching its position in `self.lambdas`).
    /// If the lambda has captures, also allocates the env tuple and stores
    /// captured values from the enclosing context.
    fn emit_closure_alloc(&self, lambda_idx: u32, ctx: &mut FuncContext, func: &mut Function) {
        let lam = &self.lambdas[lambda_idx as usize];
        let saved_temp = ctx.str_temps.first().copied().unwrap_or(0);

        // 1. If captures, allocate env tuple and fill it.
        let env_temp = ctx.str_temps.get(1).copied().unwrap_or(0);
        if lam.captures.is_empty() {
            func.instruction(&Instruction::I32Const(0));
            func.instruction(&Instruction::LocalSet(env_temp));
        } else {
            let env_size: u32 = lam.captures.iter().map(|c| c.ty.size_bytes()).sum();
            // Allocate env_size bytes
            func.instruction(&Instruction::I32Const(env_size as i32));
            func.instruction(&Instruction::Call(self.alloc_func_idx));
            func.instruction(&Instruction::LocalSet(env_temp));
            // Store each capture from enclosing ctx into env at its offset.
            for cap in &lam.captures {
                let (local_idx, _) = match ctx.get_local(&cap.name) {
                    Some(p) => p,
                    None => continue, // unresolved — leaves zero
                };
                func.instruction(&Instruction::LocalGet(env_temp));
                func.instruction(&Instruction::LocalGet(local_idx));
                match &cap.ty {
                    WasmType::I64 => {
                        func.instruction(&Instruction::I64Store(MemArg {
                            offset: cap.offset as u64,
                            align: 3,
                            memory_index: 0,
                        }));
                    }
                    WasmType::F64 => {
                        func.instruction(&Instruction::F64Store(MemArg {
                            offset: cap.offset as u64,
                            align: 3,
                            memory_index: 0,
                        }));
                    }
                    _ => {
                        func.instruction(&Instruction::I32Store(MemArg {
                            offset: cap.offset as u64,
                            align: 2,
                            memory_index: 0,
                        }));
                    }
                }
            }
        }

        // 2. Allocate 8-byte closure struct.
        func.instruction(&Instruction::I32Const(8));
        func.instruction(&Instruction::Call(self.alloc_func_idx));
        func.instruction(&Instruction::LocalSet(saved_temp));
        // Store func_idx at offset 0 (the table index — same as lambda_idx).
        func.instruction(&Instruction::LocalGet(saved_temp));
        func.instruction(&Instruction::I32Const(lambda_idx as i32));
        func.instruction(&Instruction::I32Store(MemArg {
            offset: 0,
            align: 2,
            memory_index: 0,
        }));
        // Store env_ptr at offset 4.
        func.instruction(&Instruction::LocalGet(saved_temp));
        func.instruction(&Instruction::LocalGet(env_temp));
        func.instruction(&Instruction::I32Store(MemArg {
            offset: 4,
            align: 2,
            memory_index: 0,
        }));
        // Push closure ptr.
        func.instruction(&Instruction::LocalGet(saved_temp));
    }

    /// Emit the WASM body for a synthesized lambda function.
    ///
    /// Calling convention: `(env_ptr: i32, ...user_params) -> ret`. `env_ptr`
    /// is local 0; user params start at local 1. If the lambda has captured
    /// variables, the body loads them from `env_ptr` at the recorded
    /// offsets. No-capture lambdas ignore env_ptr.
    fn emit_lambda_function(&self, lam: &LambdaInfo) -> Function {
        let mut ctx = FuncContext {
            locals: HashMap::new(),
            next_local: 1, // local 0 reserved for env_ptr
            return_type: lam.return_type.clone(),
            block_depth: 0,
            loop_labels: Vec::new(),
            str_temps: Vec::new(),
            sub_scratch: Vec::new(),
            sub_scratch_i64: Vec::new(),
            sub_depth: 0,
            str_saves: Vec::new(),
            str_depth: 0,
            try_depth: 0,
            captures: Vec::new(),
            ck_i64: Vec::new(),
            pw_i64: Vec::new(),
            ck_f64: Vec::new(),
        };
        // env_ptr is implicit local 0. We don't add it to ctx.locals (so user
        // code can't accidentally reference it by name), but we do record
        // captures here so emit_expr Name(c) can find them.
        ctx.captures = lam.captures.clone();
        // User params become locals 1..n
        for (name, ty) in &lam.params {
            let idx = ctx.next_local;
            ctx.next_local += 1;
            ctx.locals.insert(name.clone(), (idx, ty.clone()));
        }
        // Pre-allocate list-subscript scratch pairs (see
        // FuncContext::sub_scratch) — lambdas index lists too;
        // previously this path fell back to clobbering local 0
        // (env_ptr) via the unwrap_or(0) default. Sized from a static
        // pre-scan of the lambda body's subscript nesting (floor 4), so
        // nested reads always have their own pair. Depth is bounded by the
        // WASM_MAX_SUBSCRIPT_NESTING eligibility check.
        let sub_pairs = pyths_hir::max_subscript_depth(&lam.body).max(4);
        for i in 0..sub_pairs {
            let l = ctx.next_local;
            ctx.next_local += 1;
            ctx.locals
                .insert(format!("__subl{}", i), (l, WasmType::I32));
            let r = ctx.next_local;
            ctx.next_local += 1;
            ctx.locals
                .insert(format!("__subi{}", i), (r, WasmType::I32));
            ctx.sub_scratch.push((l, r));
            // CVE-2026-15903 (F3/F4): i64 bounds-check pair for this depth.
            let bi = ctx.next_local;
            ctx.next_local += 1;
            ctx.locals
                .insert(format!("__subn{}", i), (bi, WasmType::I64));
            let bl = ctx.next_local;
            ctx.next_local += 1;
            ctx.locals
                .insert(format!("__subL{}", i), (bl, WasmType::I64));
            ctx.sub_scratch_i64.push((bi, bl));
        }
        // #358: overflow-check scratch (4 + 3 i64, 2 f64) — lambda bodies are
        // expressions and can contain checked int arithmetic too.
        for i in 0..4 {
            let idx = ctx.next_local;
            ctx.next_local += 1;
            ctx.locals
                .insert(format!("__ck{}", i), (idx, WasmType::I64));
            ctx.ck_i64.push(idx);
        }
        for i in 0..3 {
            let idx = ctx.next_local;
            ctx.next_local += 1;
            ctx.locals
                .insert(format!("__pw{}", i), (idx, WasmType::I64));
            ctx.pw_i64.push(idx);
        }
        for i in 0..2 {
            let idx = ctx.next_local;
            ctx.next_local += 1;
            ctx.locals
                .insert(format!("__ckf{}", i), (idx, WasmType::F64));
            ctx.ck_f64.push(idx);
        }
        // Scratch non-interference — LAMBDA variant: local 0 is env_ptr, so no
        // scratch slot may alias it (the old `unwrap_or(0)` clobber bug class).
        debug_assert!(
            ctx.scratch_non_interference_violations(true).is_empty(),
            "WASM scratch interference in a lambda body: {:?}",
            ctx.scratch_non_interference_violations(true)
        );
        let mut func = Function::new(vec![
            (8, ValType::I32),
            (7, ValType::I64),
            (2, ValType::F64),
        ]);
        self.emit_expr(&lam.body, &mut ctx, &mut func);
        let body_ty = self.expr_type(&lam.body, &ctx);
        if let Some(ret_ty) = &lam.return_type {
            if body_ty != *ret_ty {
                self.emit_convert(&body_ty, ret_ty, &mut func);
            }
        }
        func.instruction(&Instruction::End);
        func
    }

    /// Emit the __alloc bump-allocator function.
    fn emit_alloc_function(&self) -> Function {
        // __alloc(size: i32) -> i32
        // Local 0: size (param)
        // Local 1: result (saved old heap_ptr)
        // Local 2: deficit / pages scratch
        let mut func = Function::new(vec![(2, ValType::I32)]);

        // result = global.get __heap_ptr
        func.instruction(&Instruction::GlobalGet(0));
        func.instruction(&Instruction::LocalTee(1));

        // __heap_ptr = result + size
        func.instruction(&Instruction::LocalGet(0));
        func.instruction(&Instruction::I32Add);
        func.instruction(&Instruction::GlobalSet(0));

        // deficit = __heap_ptr - memory.size * 65536  (bytes past the end)
        func.instruction(&Instruction::GlobalGet(0));
        func.instruction(&Instruction::MemorySize(0));
        func.instruction(&Instruction::I32Const(65536));
        func.instruction(&Instruction::I32Mul);
        func.instruction(&Instruction::I32Sub);
        func.instruction(&Instruction::LocalTee(2));
        // if deficit > 0: grow by ceil(deficit / 65536) pages (NOT just 1 — a
        // single alloc larger than 64 KiB, e.g. a 10k-element f64 list = 80 KiB,
        // needs ≥2 pages, otherwise the returned ptr is out of bounds: B-034).
        func.instruction(&Instruction::I32Const(0));
        func.instruction(&Instruction::I32GtS);
        func.instruction(&Instruction::If(wasm_encoder::BlockType::Empty));
        func.instruction(&Instruction::LocalGet(2));
        func.instruction(&Instruction::I32Const(65535));
        func.instruction(&Instruction::I32Add);
        func.instruction(&Instruction::I32Const(65536));
        func.instruction(&Instruction::I32DivU);
        func.instruction(&Instruction::MemoryGrow(0));
        func.instruction(&Instruction::Drop);
        func.instruction(&Instruction::End);

        // return result
        func.instruction(&Instruction::LocalGet(1));
        func.instruction(&Instruction::End);
        func
    }

    /// Collect all string literals from eligible function bodies into the string pool.
    fn collect_string_literals(&mut self, module: &Module, analysis: &WasmAnalysis) {
        for info in analysis.eligible.values() {
            if let StmtKind::FuncDef { body, .. } = &module.body[info.stmt_index].kind {
                self.collect_strings_from_stmts(body);
            }
        }
    }

    fn collect_strings_from_stmts(&mut self, stmts: &[Stmt]) {
        for stmt in stmts {
            self.collect_strings_from_stmt(stmt);
        }
    }

    fn collect_strings_from_stmt(&mut self, stmt: &Stmt) {
        match &stmt.kind {
            StmtKind::Assign { value, .. } => self.collect_strings_from_expr(value),
            StmtKind::AugAssign { value, .. } => self.collect_strings_from_expr(value),
            StmtKind::AnnAssign { value: Some(v), .. } => self.collect_strings_from_expr(v),
            StmtKind::Return(Some(v)) => self.collect_strings_from_expr(v),
            StmtKind::Expr(e) => self.collect_strings_from_expr(e),
            StmtKind::If {
                test,
                body,
                elif_clauses,
                else_body,
            } => {
                self.collect_strings_from_expr(test);
                self.collect_strings_from_stmts(body);
                for (t, b) in elif_clauses {
                    self.collect_strings_from_expr(t);
                    self.collect_strings_from_stmts(b);
                }
                if let Some(eb) = else_body {
                    self.collect_strings_from_stmts(eb);
                }
            }
            StmtKind::While {
                test,
                body,
                else_body,
            } => {
                self.collect_strings_from_expr(test);
                self.collect_strings_from_stmts(body);
                if let Some(eb) = else_body {
                    self.collect_strings_from_stmts(eb);
                }
            }
            StmtKind::For {
                body, else_body, ..
            } => {
                self.collect_strings_from_stmts(body);
                if let Some(eb) = else_body {
                    self.collect_strings_from_stmts(eb);
                }
            }
            // raise X("msg") and except handler bodies — collect strings from
            // the message argument so __err_msg can reference a pooled literal.
            StmtKind::Raise(Some(e), _) => self.collect_strings_from_expr(e),
            StmtKind::Try { body, handlers, .. } => {
                self.collect_strings_from_stmts(body);
                for h in handlers {
                    self.collect_strings_from_stmts(&h.body);
                }
            }
            _ => {}
        }
    }

    fn collect_strings_from_expr(&mut self, expr: &Expr) {
        match &expr.kind {
            ExprKind::StringLiteral(s) => self.add_string_to_pool(s),
            ExprKind::FString { parts } => {
                for part in parts {
                    match part {
                        FStringPart::Literal(s) => self.add_string_to_pool(s),
                        FStringPart::Expr(e) => self.collect_strings_from_expr(e),
                    }
                }
            }
            ExprKind::BinOp { left, right, .. } => {
                self.collect_strings_from_expr(left);
                self.collect_strings_from_expr(right);
            }
            ExprKind::UnaryOp { operand, .. } => self.collect_strings_from_expr(operand),
            ExprKind::Call { func, args, .. } => {
                self.collect_strings_from_expr(func);
                for arg in args {
                    self.collect_strings_from_expr(arg);
                }
            }
            ExprKind::Compare { left, comparisons } => {
                self.collect_strings_from_expr(left);
                for (_, e) in comparisons {
                    self.collect_strings_from_expr(e);
                }
            }
            ExprKind::IfExpr {
                test,
                body,
                else_body,
            } => {
                self.collect_strings_from_expr(test);
                self.collect_strings_from_expr(body);
                self.collect_strings_from_expr(else_body);
            }
            ExprKind::Subscript { value, index, .. } => {
                self.collect_strings_from_expr(value);
                self.collect_strings_from_expr(index);
            }
            ExprKind::Attribute { value, .. } => {
                self.collect_strings_from_expr(value);
            }
            _ => {}
        }
    }

    fn add_string_to_pool(&mut self, s: &str) {
        if !self.string_dedup.contains_key(s) {
            // Reserve offset 0 in the data section as a sentinel meaning
            // "no string" — the first real string starts at offset 4.
            // This is necessary so __err_msg = 0 unambiguously means "no
            // message" (without this, the very first pooled string would
            // alias offset 0 and look identical to "unset").
            if self.data_section_size == 0 {
                self.data_section_size = 4;
            }
            let offset = self.data_section_size;
            self.string_dedup.insert(s.to_string(), offset);
            self.string_pool.push((s.to_string(), offset));
            self.data_section_size += 4 + s.len() as u32;
        }
    }

    fn build_data_section_bytes(&self) -> Vec<u8> {
        let mut data = Vec::with_capacity(self.data_section_size as usize);
        // Reserve 4 leading zero bytes (offset 0) — the sentinel for "no string".
        if self.data_section_size > 0 {
            data.extend_from_slice(&[0u8; 4]);
        }
        for (s, _) in &self.string_pool {
            let bytes = s.as_bytes();
            data.extend_from_slice(&(bytes.len() as i32).to_le_bytes());
            data.extend_from_slice(bytes);
        }
        data
    }

    /// Emit a single WASM function.
    fn emit_function(&self, stmt: &Stmt, info: &WasmFuncInfo) -> Function {
        let body = match &stmt.kind {
            StmtKind::FuncDef { body, .. } => body,
            _ => unreachable!("Expected FuncDef"),
        };

        // Set up context: params become locals 0..n-1
        let mut ctx = FuncContext {
            locals: HashMap::new(),
            next_local: 0,
            return_type: to_wasm_type(&info.return_type),
            block_depth: 0,
            loop_labels: Vec::new(),
            str_temps: Vec::new(),
            sub_scratch: Vec::new(),
            sub_scratch_i64: Vec::new(),
            sub_depth: 0,
            str_saves: Vec::new(),
            str_depth: 0,
            try_depth: 0,
            captures: Vec::new(),
            ck_i64: Vec::new(),
            pw_i64: Vec::new(),
            ck_f64: Vec::new(),
        };

        for (name, ty) in &info.params {
            let wt = to_wasm_type(ty).unwrap();
            let idx = ctx.next_local;
            ctx.next_local += 1;
            ctx.locals.insert(name.clone(), (idx, wt));
        }

        // Pre-scan body for local variables
        let extra_locals = self.collect_locals(body, &ctx);
        for (name, wt) in &extra_locals {
            if !ctx.locals.contains_key(name) {
                let idx = ctx.next_local;
                ctx.next_local += 1;
                ctx.locals.insert(name.clone(), (idx, wt.clone()));
            }
        }

        // Pre-allocate list-subscript scratch pairs (depth-indexed; see
        // FuncContext::sub_scratch). WASM locals are declared in the function
        // header before the body is walked, so the pool must be sized up front
        // from a static pre-scan of the maximum subscript nesting. A floor of 8
        // preserves the historical allocation for shallow functions; deeper
        // nesting grows the pool exactly. Functions nesting past
        // WASM_MAX_SUBSCRIPT_NESTING were already rejected in
        // `pyths_hir::wasm_analysis::check_body`, so this count is bounded.
        let sub_pairs = pyths_hir::max_subscript_depth_in_stmts(body).max(8);
        for i in 0..sub_pairs {
            let l = ctx.next_local;
            ctx.next_local += 1;
            ctx.locals
                .insert(format!("__subl{}", i), (l, WasmType::I32));
            let r = ctx.next_local;
            ctx.next_local += 1;
            ctx.locals
                .insert(format!("__subi{}", i), (r, WasmType::I32));
            ctx.sub_scratch.push((l, r));
            // CVE-2026-15903 (F3/F4): i64 bounds-check pair for this depth.
            let bi = ctx.next_local;
            ctx.next_local += 1;
            ctx.locals
                .insert(format!("__subn{}", i), (bi, WasmType::I64));
            let bl = ctx.next_local;
            ctx.next_local += 1;
            ctx.locals
                .insert(format!("__subL{}", i), (bl, WasmType::I64));
            ctx.sub_scratch_i64.push((bi, bl));
        }

        // #358: pre-allocate overflow-check scratch locals. Binary checked
        // ops use __ck0..3; the exact integer-pow loop uses __pw0..2 (it
        // calls the checked-mul sequence, which uses __ck, internally);
        // on-stack float mod/floordiv use __ckf0..1.
        for i in 0..4 {
            let idx = ctx.next_local;
            ctx.next_local += 1;
            ctx.locals
                .insert(format!("__ck{}", i), (idx, WasmType::I64));
            ctx.ck_i64.push(idx);
        }
        for i in 0..3 {
            let idx = ctx.next_local;
            ctx.next_local += 1;
            ctx.locals
                .insert(format!("__pw{}", i), (idx, WasmType::I64));
            ctx.pw_i64.push(idx);
        }
        for i in 0..2 {
            let idx = ctx.next_local;
            ctx.next_local += 1;
            ctx.locals
                .insert(format!("__ckf{}", i), (idx, WasmType::F64));
            ctx.ck_f64.push(idx);
        }

        // Pre-allocate string temp locals if this module uses strings
        if self.needs_strings {
            // 8 operation temps
            for i in 0..8 {
                let name = format!("__t{}", i);
                let idx = ctx.next_local;
                ctx.next_local += 1;
                ctx.locals.insert(name, (idx, WasmType::I32));
                ctx.str_temps.push(idx);
            }
            // 4 save slots for nesting
            for i in 0..4 {
                let name = format!("__sv{}", i);
                let idx = ctx.next_local;
                ctx.next_local += 1;
                ctx.locals.insert(name, (idx, WasmType::I32));
                ctx.str_saves.push(idx);
            }
        }

        // Build function: declare extra locals (params are implicit)
        let param_count = info.params.len() as u32;
        let local_declarations: Vec<(u32, ValType)> = {
            let mut decls: Vec<(u32, ValType)> = Vec::new();
            // Collect extra locals sorted by index
            let mut extra: Vec<(u32, WasmType)> = ctx
                .locals
                .values()
                .filter(|(idx, _)| *idx >= param_count)
                .cloned()
                .collect();
            extra.sort_by_key(|(idx, _)| *idx);

            // Group consecutive locals of the same type
            for (_, wt) in extra {
                let vt = wt.to_val_type();
                if let Some(last) = decls.last_mut() {
                    if last.1 == vt {
                        last.0 += 1;
                        continue;
                    }
                }
                decls.push((1, vt));
            }
            decls
        };

        // Validate scratch non-interference now that every local (params,
        // pre-scanned body vars, and all scratch pools) is allocated — the
        // k14/env_ptr aliasing class cannot survive this. Holds by
        // construction; checked so a refactor cannot silently break it.
        debug_assert!(
            ctx.scratch_non_interference_violations(false).is_empty(),
            "WASM scratch interference in a function body: {:?}",
            ctx.scratch_non_interference_violations(false)
        );

        let mut func = Function::new(local_declarations);

        // Emit body
        for s in body {
            self.emit_stmt(s, &mut ctx, &mut func);
        }

        // WASM requires functions to end with a value matching the return type.
        // If all code paths return explicitly, this is unreachable but still needed
        // for WASM validation. Emit a default value for typed functions.
        self.emit_sentinel_for(&ctx.return_type, &mut func);

        func.instruction(&Instruction::End);
        func
    }

    /// Pre-scan a function body to find all local variable declarations.
    fn collect_locals(&self, body: &[Stmt], ctx: &FuncContext) -> Vec<(String, WasmType)> {
        let mut locals: Vec<(String, WasmType)> = Vec::new();
        let mut seen: std::collections::HashSet<String> = ctx.locals.keys().cloned().collect();

        self.scan_locals_in_stmts(body, &mut locals, &mut seen);

        // Tier 6 HoF: pre-allocate scratch locals when builtins need them.
        // Avoids the chicken-and-egg of adding locals during emit (which
        // would not be declared in the function header).
        // sorted(): pre-allocate one key local per supported element
        // type. Picking the right one at emit time is a runtime cost
        // of zero (only one is actually written/read per call); pre-
        // allocating all three lets us avoid late-allocation against
        // the function header.
        if Self::body_calls_named(body, "sorted") {
            if !seen.contains("__sort_key") {
                locals.push(("__sort_key".to_string(), WasmType::I64));
                seen.insert("__sort_key".to_string());
            }
            if !seen.contains("__sort_key_f64") {
                locals.push(("__sort_key_f64".to_string(), WasmType::F64));
                seen.insert("__sort_key_f64".to_string());
            }
            if !seen.contains("__sort_key_i32") {
                locals.push(("__sort_key_i32".to_string(), WasmType::I32));
                seen.insert("__sort_key_i32".to_string());
            }
        }
        // reduce() accumulator: i64 (most common) and f64 (for float folds).
        if Self::body_calls_named(body, "reduce") {
            if !seen.contains("__reduce_i64") {
                locals.push(("__reduce_i64".to_string(), WasmType::I64));
                seen.insert("__reduce_i64".to_string());
            }
            if !seen.contains("__reduce_f64") {
                locals.push(("__reduce_f64".to_string(), WasmType::F64));
                seen.insert("__reduce_f64".to_string());
            }
        }

        locals
    }

    fn body_calls_named(body: &[Stmt], target: &str) -> bool {
        body.iter().any(|s| Self::stmt_calls_named(s, target))
    }

    fn stmt_calls_named(stmt: &Stmt, target: &str) -> bool {
        match &stmt.kind {
            StmtKind::Expr(e) => Self::expr_calls_named(e, target),
            StmtKind::Assign { value, .. } => Self::expr_calls_named(value, target),
            StmtKind::AugAssign { value, .. } => Self::expr_calls_named(value, target),
            StmtKind::AnnAssign { value: Some(v), .. } => Self::expr_calls_named(v, target),
            StmtKind::Return(Some(v)) => Self::expr_calls_named(v, target),
            StmtKind::If {
                test,
                body,
                elif_clauses,
                else_body,
            } => {
                Self::expr_calls_named(test, target)
                    || Self::body_calls_named(body, target)
                    || elif_clauses.iter().any(|(t, b)| {
                        Self::expr_calls_named(t, target) || Self::body_calls_named(b, target)
                    })
                    || else_body
                        .as_ref()
                        .is_some_and(|b| Self::body_calls_named(b, target))
            }
            StmtKind::While {
                body, else_body, ..
            }
            | StmtKind::For {
                body, else_body, ..
            } => {
                Self::body_calls_named(body, target)
                    || else_body
                        .as_ref()
                        .is_some_and(|b| Self::body_calls_named(b, target))
            }
            _ => false,
        }
    }

    fn expr_calls_named(expr: &Expr, target: &str) -> bool {
        match &expr.kind {
            ExprKind::Call { func, args, .. } => {
                let is_target = matches!(&func.kind, ExprKind::Name(n) if n == target);
                is_target || args.iter().any(|a| Self::expr_calls_named(a, target))
            }
            ExprKind::BinOp { left, right, .. } => {
                Self::expr_calls_named(left, target) || Self::expr_calls_named(right, target)
            }
            ExprKind::UnaryOp { operand, .. } => Self::expr_calls_named(operand, target),
            ExprKind::Subscript { value, index, .. } => {
                Self::expr_calls_named(value, target) || Self::expr_calls_named(index, target)
            }
            ExprKind::IfExpr {
                test,
                body,
                else_body,
            } => {
                Self::expr_calls_named(test, target)
                    || Self::expr_calls_named(body, target)
                    || Self::expr_calls_named(else_body, target)
            }
            _ => false,
        }
    }

    // `sorted()` usage-detection helpers. Retained as a reference
    // walker pattern; the active eligibility analysis lives in
    // `pyths_hir::wasm_analysis` and handles `sorted` there.
    #[allow(dead_code)]
    fn body_calls_sorted(body: &[Stmt]) -> bool {
        body.iter().any(Self::stmt_calls_sorted)
    }

    #[allow(dead_code)]
    fn stmt_calls_sorted(stmt: &Stmt) -> bool {
        match &stmt.kind {
            StmtKind::Expr(e) => Self::expr_calls_sorted(e),
            StmtKind::Assign { value, .. } => Self::expr_calls_sorted(value),
            StmtKind::AugAssign { value, .. } => Self::expr_calls_sorted(value),
            StmtKind::AnnAssign { value: Some(v), .. } => Self::expr_calls_sorted(v),
            StmtKind::Return(Some(v)) => Self::expr_calls_sorted(v),
            StmtKind::If {
                test,
                body,
                elif_clauses,
                else_body,
            } => {
                Self::expr_calls_sorted(test)
                    || Self::body_calls_sorted(body)
                    || elif_clauses
                        .iter()
                        .any(|(t, b)| Self::expr_calls_sorted(t) || Self::body_calls_sorted(b))
                    || else_body
                        .as_ref()
                        .is_some_and(|b| Self::body_calls_sorted(b))
            }
            StmtKind::While {
                body, else_body, ..
            }
            | StmtKind::For {
                body, else_body, ..
            } => {
                Self::body_calls_sorted(body)
                    || else_body
                        .as_ref()
                        .is_some_and(|b| Self::body_calls_sorted(b))
            }
            _ => false,
        }
    }

    #[allow(dead_code)]
    fn expr_calls_sorted(expr: &Expr) -> bool {
        match &expr.kind {
            ExprKind::Call { func, args, .. } => {
                let is_sorted = matches!(&func.kind, ExprKind::Name(n) if n == "sorted");
                is_sorted || args.iter().any(Self::expr_calls_sorted)
            }
            ExprKind::BinOp { left, right, .. } => {
                Self::expr_calls_sorted(left) || Self::expr_calls_sorted(right)
            }
            ExprKind::UnaryOp { operand, .. } => Self::expr_calls_sorted(operand),
            ExprKind::Subscript { value, index, .. } => {
                Self::expr_calls_sorted(value) || Self::expr_calls_sorted(index)
            }
            ExprKind::IfExpr {
                test,
                body,
                else_body,
            } => {
                Self::expr_calls_sorted(test)
                    || Self::expr_calls_sorted(body)
                    || Self::expr_calls_sorted(else_body)
            }
            _ => false,
        }
    }

    fn scan_locals_in_stmts(
        &self,
        stmts: &[Stmt],
        locals: &mut Vec<(String, WasmType)>,
        seen: &mut std::collections::HashSet<String>,
    ) {
        for stmt in stmts {
            self.scan_locals_in_stmt(stmt, locals, seen);
        }
    }

    fn scan_locals_in_stmt(
        &self,
        stmt: &Stmt,
        locals: &mut Vec<(String, WasmType)>,
        seen: &mut std::collections::HashSet<String>,
    ) {
        match &stmt.kind {
            StmtKind::Assign { targets, value } => {
                for t in targets {
                    match &t.kind {
                        ExprKind::Name(name) => {
                            if !seen.contains(name) {
                                let wt = self.infer_wasm_type_with_locals(value, locals);
                                locals.push((name.clone(), wt));
                                seen.insert(name.clone());
                            }
                        }
                        // Tuple-unpack target: pre-allocate each name local.
                        ExprKind::Tuple(elts) => {
                            // Try to type each binding from the value's tuple shape.
                            let val_ty = self.infer_wasm_type_with_locals(value, locals);
                            let elt_types: Vec<WasmType> = match val_ty {
                                WasmType::PtrTuple(ts) => ts,
                                _ => vec![WasmType::I64; elts.len()],
                            };
                            for (i, e) in elts.iter().enumerate() {
                                if let ExprKind::Name(name) = &e.kind {
                                    if !seen.contains(name) {
                                        let ty = if i < elt_types.len() {
                                            elt_types[i].clone()
                                        } else {
                                            WasmType::I64
                                        };
                                        locals.push((name.clone(), ty));
                                        seen.insert(name.clone());
                                    }
                                }
                            }
                        }
                        _ => {}
                    }
                }
            }
            StmtKind::AnnAssign {
                target, annotation, ..
            } => {
                if let ExprKind::Name(name) = &target.kind {
                    if !seen.contains(name) {
                        let ty = resolve_type(annotation);
                        let wt = to_wasm_type(&ty).unwrap_or(WasmType::I64);
                        locals.push((name.clone(), wt));
                        seen.insert(name.clone());
                    }
                }
            }
            StmtKind::For {
                target,
                iter,
                body,
                else_body,
                ..
            } => {
                if let ExprKind::Name(name) = &target.kind {
                    if !seen.contains(name) {
                        // For range() the loop var is i64; for list iteration
                        // we infer the element type from the iterable's literal
                        // shape (best-effort).
                        let var_ty = match self.infer_wasm_type_from_expr(iter) {
                            WasmType::PtrList(inner) => (*inner).clone(),
                            _ => WasmType::I64,
                        };
                        locals.push((name.clone(), var_ty));
                        seen.insert(name.clone());
                    }
                    // for-range temps
                    let stop_name = format!("__stop_{}", name);
                    if !seen.contains(&stop_name) {
                        locals.push((stop_name.clone(), WasmType::I64));
                        seen.insert(stop_name);
                    }
                    let step_name = format!("__step_{}", name);
                    if !seen.contains(&step_name) {
                        locals.push((step_name.clone(), WasmType::I64));
                        seen.insert(step_name);
                    }
                    // for-list temps
                    let i_name = format!("__for_i_{}", name);
                    if !seen.contains(&i_name) {
                        locals.push((i_name.clone(), WasmType::I32));
                        seen.insert(i_name);
                    }
                    let n_name = format!("__for_n_{}", name);
                    if !seen.contains(&n_name) {
                        locals.push((n_name.clone(), WasmType::I32));
                        seen.insert(n_name);
                    }
                }
                self.scan_locals_in_stmts(body, locals, seen);
                if let Some(else_b) = else_body {
                    self.scan_locals_in_stmts(else_b, locals, seen);
                }
            }
            StmtKind::If {
                body,
                elif_clauses,
                else_body,
                ..
            } => {
                self.scan_locals_in_stmts(body, locals, seen);
                for (_, elif_body) in elif_clauses {
                    self.scan_locals_in_stmts(elif_body, locals, seen);
                }
                if let Some(else_b) = else_body {
                    self.scan_locals_in_stmts(else_b, locals, seen);
                }
            }
            StmtKind::While {
                body, else_body, ..
            } => {
                self.scan_locals_in_stmts(body, locals, seen);
                if let Some(else_b) = else_body {
                    self.scan_locals_in_stmts(else_b, locals, seen);
                }
            }
            _ => {}
        }
    }

    /// Locals-aware type inference used by `scan_locals_in_stmt`. When
    /// resolving a `Name`, consult the partially-built `locals` slice
    /// (assignments earlier in source order) before falling back to
    /// the param-table / I64 path. Without this, an assignment like
    /// `s = sorted(nums)` would register `s` with element type `I64`
    /// regardless of what `nums` actually held — which is correct for
    /// integer lists but produces a type-mismatched WASM module for
    /// `[3.5, 1.25, ...]` (the load/store ops pick the wrong width).
    fn infer_wasm_type_with_locals(&self, expr: &Expr, locals: &[(String, WasmType)]) -> WasmType {
        // Name → look in locals first.
        if let ExprKind::Name(name) = &expr.kind {
            if let Some((_, ty)) = locals.iter().find(|(n, _)| n == name) {
                return ty.clone();
            }
            // Fall through to the default Name path (params / I64
            // fallback). The default function handles all other expr
            // kinds, so recurse into it for non-Name expressions.
        }
        // Livermore finding (2026-07-10): a subscript READ takes the
        // container's ELEMENT type. Previously this fell through to the
        // I64 default, so `a = x[k]` on a float list declared an i64
        // local receiving an f64.load — invalid WASM (hit by 8/24 LFK
        // kernels: k02/k04/k10/k13/k14/k15/k17/k23).
        if let ExprKind::Subscript { value, index, .. } = &expr.kind {
            if !matches!(index.kind, ExprKind::Slice { .. }) {
                if let WasmType::PtrList(inner) = self.infer_wasm_type_with_locals(value, locals) {
                    return (*inner).clone();
                }
            }
        }
        // Livermore finding (2026-07-10, part 2): BinOp/UnaryOp inference
        // must recurse through the LOCALS-AWARE path — `br = ar - px[k]`
        // where `ar` is an f64 local previously fell into the
        // params-or-I64 default and declared `br` as i64 receiving an
        // f64.sub (k10/k17/k23).
        if let ExprKind::BinOp { left, op, right } = &expr.kind {
            let lt = self.infer_wasm_type_with_locals(left, locals);
            let rt = self.infer_wasm_type_with_locals(right, locals);
            return match op {
                BinOp::Div => WasmType::F64,
                BinOp::Eq | BinOp::NotEq | BinOp::Lt | BinOp::LtEq | BinOp::Gt | BinOp::GtEq => {
                    WasmType::I32
                }
                BinOp::And | BinOp::Or => WasmType::I32,
                _ => {
                    if lt == WasmType::F64 || rt == WasmType::F64 {
                        WasmType::F64
                    } else {
                        lt
                    }
                }
            };
        }
        if let ExprKind::UnaryOp { op, operand } = &expr.kind {
            return match op {
                UnaryOp::Not => WasmType::I32,
                _ => self.infer_wasm_type_with_locals(operand, locals),
            };
        }
        // For Call expressions, recurse for the argument types via the
        // locals-aware path so chained inference (`s = sorted(nums)`
        // then `t = sorted(s)`) propagates element types correctly.
        if let ExprKind::Call { func, args, .. } = &expr.kind {
            if let ExprKind::Name(name) = &func.kind {
                if matches!(name.as_str(), "sorted" | "filter") {
                    let lst_arg_idx = if name == "filter" { 1 } else { 0 };
                    if let Some(lst_arg) = args.get(lst_arg_idx) {
                        if let WasmType::PtrList(inner) =
                            self.infer_wasm_type_with_locals(lst_arg, locals)
                        {
                            return WasmType::PtrList(inner);
                        }
                    }
                }
            }
        }
        self.infer_wasm_type_from_expr(expr)
    }

    fn infer_wasm_type_from_expr(&self, expr: &Expr) -> WasmType {
        match &expr.kind {
            ExprKind::IntLiteral(_) => WasmType::I64,
            ExprKind::FloatLiteral(_) => WasmType::F64,
            ExprKind::BoolLiteral(_) => WasmType::I32,
            ExprKind::StringLiteral(_) => WasmType::Ptr,
            ExprKind::FString { .. } => WasmType::Ptr,
            // Step 2/3/4: collection literals — return full structural type so
            // locals carry element shape (needed for offset / size computation).
            ExprKind::Tuple(elts) => {
                let elt_types: Vec<WasmType> = elts
                    .iter()
                    .map(|e| self.infer_wasm_type_from_expr(e))
                    .collect();
                WasmType::PtrTuple(elt_types)
            }
            ExprKind::List(elts) => {
                let inner = if let Some(e) = elts.first() {
                    self.infer_wasm_type_from_expr(e)
                } else {
                    WasmType::I64
                };
                WasmType::PtrList(Box::new(inner))
            }
            ExprKind::Dict { .. } => {
                WasmType::PtrDict(Box::new(WasmType::Ptr), Box::new(WasmType::I64))
            }
            ExprKind::Lambda { params, body } => {
                // Derive closure signature exactly as collect_lambdas does.
                let wparams: Vec<(String, WasmType)> = params
                    .iter()
                    .map(|p| {
                        let ty = p
                            .annotation
                            .as_ref()
                            .map(|a| resolve_type(a))
                            .unwrap_or(Type::Int);
                        (p.name.clone(), to_wasm_type(&ty).unwrap_or(WasmType::I64))
                    })
                    .collect();
                let ret = Self::infer_lambda_return_type(body, &wparams);
                WasmType::PtrClosure {
                    params: wparams.into_iter().map(|(_, t)| t).collect(),
                    ret: ret.map(Box::new),
                }
            }
            ExprKind::ListComp { .. } => WasmType::PtrList(Box::new(WasmType::I64)),
            ExprKind::DictComp { .. } => {
                WasmType::PtrDict(Box::new(WasmType::Ptr), Box::new(WasmType::I64))
            }
            ExprKind::SetComp { .. } => WasmType::PtrList(Box::new(WasmType::I64)),
            ExprKind::BinOp { left, op, right } => {
                let lt = self.infer_wasm_type_from_expr(left);
                let rt = self.infer_wasm_type_from_expr(right);
                match op {
                    BinOp::Div => WasmType::F64, // Python true division always returns float
                    BinOp::Eq
                    | BinOp::NotEq
                    | BinOp::Lt
                    | BinOp::LtEq
                    | BinOp::Gt
                    | BinOp::GtEq => WasmType::I32,
                    BinOp::And | BinOp::Or => {
                        // For logical ops in numeric context, result is i32 (bool)
                        WasmType::I32
                    }
                    _ => {
                        if lt == WasmType::F64 || rt == WasmType::F64 {
                            WasmType::F64
                        } else {
                            lt
                        }
                    }
                }
            }
            ExprKind::UnaryOp { op, operand } => match op {
                UnaryOp::Not => WasmType::I32,
                _ => self.infer_wasm_type_from_expr(operand),
            },
            ExprKind::Compare { .. } => WasmType::I32,
            ExprKind::Name(name) => {
                // Look up in func_info params
                for info in self.func_info.values() {
                    for (pname, ty) in &info.params {
                        if pname == name {
                            return to_wasm_type(ty).unwrap_or(WasmType::I64);
                        }
                    }
                }
                WasmType::I64
            }
            ExprKind::Call { func, args, .. } => {
                if let ExprKind::Name(name) = &func.kind {
                    match name.as_str() {
                        "int" => WasmType::I64,
                        "float" => WasmType::F64,
                        "abs" | "min" | "max" => WasmType::F64,
                        // Tier 6 HoF: map/filter/sorted return a list; reduce
                        // returns the accumulator type (≈ init's type).
                        "map" => {
                            // Element type = lambda's ret type, if available.
                            let elem = if let Some(fn_arg) = args.first() {
                                if let WasmType::PtrClosure { ret, .. } =
                                    self.infer_wasm_type_from_expr(fn_arg)
                                {
                                    ret.map(|b| (*b).clone()).unwrap_or(WasmType::I64)
                                } else {
                                    WasmType::I64
                                }
                            } else {
                                WasmType::I64
                            };
                            WasmType::PtrList(Box::new(elem))
                        }
                        "filter" | "sorted" => {
                            // Element type = input list's element type.
                            let elem = if let Some(lst_arg) =
                                args.get(if name == "filter" { 1 } else { 0 })
                            {
                                if let WasmType::PtrList(inner) =
                                    self.infer_wasm_type_from_expr(lst_arg)
                                {
                                    (*inner).clone()
                                } else {
                                    WasmType::I64
                                }
                            } else {
                                WasmType::I64
                            };
                            WasmType::PtrList(Box::new(elem))
                        }
                        "reduce" => {
                            // Result is the accumulator type — same as init.
                            if let Some(init) = args.get(2) {
                                self.infer_wasm_type_from_expr(init)
                            } else {
                                WasmType::I64
                            }
                        }
                        _ => {
                            if self.math_aliases.contains_key(name) {
                                WasmType::F64
                            } else if let Some(info) = self.func_info.get(name.as_str()) {
                                to_wasm_type(&info.return_type).unwrap_or(WasmType::I64)
                            } else {
                                WasmType::I64
                            }
                        }
                    }
                } else {
                    WasmType::I64
                }
            }
            ExprKind::IfExpr { body, .. } => self.infer_wasm_type_from_expr(body),
            _ => WasmType::I64,
        }
    }

    // === Statement emission ===

    fn emit_stmt(&self, stmt: &Stmt, ctx: &mut FuncContext, func: &mut Function) {
        match &stmt.kind {
            StmtKind::Assign { targets, value } => {
                for target in targets {
                    match &target.kind {
                        ExprKind::Name(name) => {
                            let ty = self.expr_type(value, ctx);
                            let idx = ctx.get_or_alloc_local(name, ty);
                            self.emit_expr(value, ctx, func);
                            func.instruction(&Instruction::LocalSet(idx));
                        }
                        // Tier 5: tuple-target unpacking `a, b = t`
                        ExprKind::Tuple(elts) => {
                            // First evaluate value to a tuple ptr, save in temp.
                            let val_ty = self.expr_type(value, ctx);
                            self.emit_expr(value, ctx, func);
                            let ptr_temp = ctx.str_temps.first().copied().unwrap_or(0);
                            func.instruction(&Instruction::LocalSet(ptr_temp));
                            // Determine element types from value.
                            let elt_types: Vec<WasmType> = match &val_ty {
                                WasmType::PtrTuple(ts) => ts.clone(),
                                _ => {
                                    // Fall back: assume all i64 elements.
                                    vec![WasmType::I64; elts.len()]
                                }
                            };
                            let mut offset: u32 = 0;
                            for (i, t) in elts.iter().enumerate() {
                                if let ExprKind::Name(name) = &t.kind {
                                    let ty = if i < elt_types.len() {
                                        elt_types[i].clone()
                                    } else {
                                        WasmType::I64
                                    };
                                    let local_idx = ctx.get_or_alloc_local(name, ty.clone());
                                    func.instruction(&Instruction::LocalGet(ptr_temp));
                                    self.emit_load_at_offset(&ty, offset, func);
                                    func.instruction(&Instruction::LocalSet(local_idx));
                                    offset += ty.size_bytes();
                                } else {
                                    // Skip non-name targets in unpacking.
                                    if i < elt_types.len() {
                                        offset += elt_types[i].size_bytes();
                                    }
                                }
                            }
                        }
                        // Subscript-target assignments `lst[i] = v` — Tier 2 store.
                        ExprKind::Subscript {
                            value: container,
                            index,
                            ..
                        } => {
                            let cont_ty = self.expr_type(container, ctx);
                            match &cont_ty {
                                WasmType::PtrList(elem_ty) => {
                                    // CVE-2026-15903 fix (F1): the store previously
                                    // had NO bounds check on any branch — `a[i] = v`
                                    // wrote to `ptr + 8 + i*elem_size` unconditionally,
                                    // landing in neighbouring live objects for OOB
                                    // `i`. Mirror the read path's UNCONDITIONAL,
                                    // full-i64, negative-normalizing check and store
                                    // only on the in-bounds branch.
                                    let elem_size = elem_ty.size_bytes();
                                    let elem = (**elem_ty).clone();
                                    let (list_temp, idx_temp) =
                                        match ctx.sub_scratch.get(ctx.sub_depth) {
                                            Some(&p) => p,
                                            None => {
                                                debug_assert!(
                                                    false,
                                                    "sub_scratch pool undersized (store)"
                                                );
                                                func.instruction(&Instruction::Unreachable);
                                                return;
                                            }
                                        };
                                    let (idx64_temp, len64_temp) =
                                        match ctx.sub_scratch_i64.get(ctx.sub_depth) {
                                            Some(&p) => p,
                                            None => {
                                                debug_assert!(
                                                    false,
                                                    "sub_scratch_i64 pool undersized (store)"
                                                );
                                                func.instruction(&Instruction::Unreachable);
                                                return;
                                            }
                                        };
                                    // Save the list pointer (bump depth so nested
                                    // reads in the container take their own scratch).
                                    ctx.sub_depth += 1;
                                    self.emit_expr(container, ctx, func);
                                    ctx.sub_depth -= 1;
                                    func.instruction(&Instruction::LocalSet(list_temp));
                                    // Raw index -> shared normalize + bounds check.
                                    ctx.sub_depth += 1;
                                    self.emit_expr(index, ctx, func);
                                    ctx.sub_depth -= 1;
                                    let idx_ty = self.expr_type(index, ctx);
                                    self.emit_list_index_check(
                                        list_temp, idx_temp, idx64_temp, len64_temp, &idx_ty, func,
                                    );
                                    // if cond { raise/trap; skip store } else { store }
                                    func.instruction(&Instruction::If(
                                        wasm_encoder::BlockType::Empty,
                                    ));
                                    self.emit_index_oob(ctx, func);
                                    func.instruction(&Instruction::Else);
                                    // In-bounds address: ptr + 8 + i*elem_size.
                                    func.instruction(&Instruction::LocalGet(list_temp));
                                    func.instruction(&Instruction::I32Const(8));
                                    func.instruction(&Instruction::I32Add);
                                    func.instruction(&Instruction::LocalGet(idx_temp));
                                    if elem_size > 1 {
                                        func.instruction(&Instruction::I32Const(elem_size as i32));
                                        func.instruction(&Instruction::I32Mul);
                                    }
                                    func.instruction(&Instruction::I32Add);
                                    // Push value.
                                    ctx.sub_depth += 1;
                                    self.emit_expr(value, ctx, func);
                                    ctx.sub_depth -= 1;
                                    let val_ty = self.expr_type(value, ctx);
                                    if val_ty != elem {
                                        self.emit_convert(&val_ty, &elem, func);
                                    }
                                    // Store.
                                    match &elem {
                                        WasmType::I64 => {
                                            func.instruction(&Instruction::I64Store(MemArg {
                                                offset: 0,
                                                align: 3,
                                                memory_index: 0,
                                            }));
                                        }
                                        WasmType::F64 => {
                                            func.instruction(&Instruction::F64Store(MemArg {
                                                offset: 0,
                                                align: 3,
                                                memory_index: 0,
                                            }));
                                        }
                                        _ => {
                                            func.instruction(&Instruction::I32Store(MemArg {
                                                offset: 0,
                                                align: 2,
                                                memory_index: 0,
                                            }));
                                        }
                                    }
                                    func.instruction(&Instruction::End);
                                }
                                WasmType::PtrDict(_, _) => {
                                    // Dict subscript-set via __dict.set_str import.
                                    if let Some(&idx) = self.import_indices.get("__dict_set_str") {
                                        self.emit_expr(container, ctx, func);
                                        self.emit_expr(index, ctx, func);
                                        self.emit_expr(value, ctx, func);
                                        let val_ty = self.expr_type(value, ctx);
                                        if val_ty != WasmType::I64 {
                                            self.emit_convert(&val_ty, &WasmType::I64, func);
                                        }
                                        func.instruction(&Instruction::Call(idx));
                                    }
                                }
                                _ => {}
                            }
                        }
                        _ => {}
                    }
                }
            }

            StmtKind::AugAssign { target, op, value } => {
                if let ExprKind::Name(name) = &target.kind {
                    if let Some((idx, wt)) = ctx.get_local(name) {
                        func.instruction(&Instruction::LocalGet(idx));
                        self.emit_expr(value, ctx, func);
                        // May need type promotion
                        let rhs_type = self.expr_type(value, ctx);
                        let op_type = if wt == WasmType::F64 || rhs_type == WasmType::F64 {
                            if rhs_type != WasmType::F64 {
                                self.emit_promote(rhs_type, WasmType::F64, func);
                            }
                            WasmType::F64
                        } else {
                            wt.clone()
                        };

                        // If lhs needs promotion and op_type is F64
                        if op_type == WasmType::F64 && wt != WasmType::F64 {
                            // For now, just emit the op with lhs type (well-typed code
                            // shouldn't mix int+float aug-assign).
                            self.emit_aug_op(*op, wt, ctx, func);
                        } else {
                            self.emit_aug_op(*op, op_type, ctx, func);
                        }
                        func.instruction(&Instruction::LocalSet(idx));
                    }
                } else if matches!(&target.kind, ExprKind::Subscript { .. }) {
                    // #364: augmented assignment to a SUBSCRIPT target
                    // (`t[i] += v`, `t[i] -= v`). This previously fell through the
                    // Name-only branch and was silently DROPPED — a no-op store,
                    // the real-code silent miscompile in `checkArray`
                    // (LiveCodeBench sample_106/109). The explicit read-modify-
                    // write form `t[i] = t[i] <op> v` lowers correctly (subscript
                    // load and store both work), so desugar to it and emit that.
                    let bin = Expr::new(
                        ExprKind::BinOp {
                            left: Box::new(target.clone()),
                            op: aug_to_binop(*op),
                            right: Box::new(value.clone()),
                        },
                        stmt.span,
                    );
                    let assign = Stmt::new(
                        StmtKind::Assign {
                            targets: vec![target.clone()],
                            value: bin,
                        },
                        stmt.span,
                    );
                    self.emit_stmt(&assign, ctx, func);
                }
            }

            StmtKind::AnnAssign {
                target,
                annotation,
                value,
            } => {
                if let ExprKind::Name(name) = &target.kind {
                    let ty = resolve_type(annotation);
                    let wt = to_wasm_type(&ty).unwrap_or(WasmType::I64);
                    let idx = ctx.get_or_alloc_local(name, wt.clone());
                    if let Some(val) = value {
                        self.emit_expr(val, ctx, func);
                        // Coerce if needed
                        let val_type = self.expr_type(val, ctx);
                        if val_type != wt {
                            self.emit_convert(&val_type, &wt, func);
                        }
                        func.instruction(&Instruction::LocalSet(idx));
                    }
                }
            }

            StmtKind::Return(val) => {
                if let Some(v) = val {
                    self.emit_expr(v, ctx, func);
                    // Coerce to return type if needed
                    let expr_ty = self.expr_type(v, ctx);
                    if let Some(ret_ty) = ctx.return_type.clone() {
                        if expr_ty != ret_ty {
                            self.emit_convert(&expr_ty, &ret_ty, func);
                        }
                    }
                }
                func.instruction(&Instruction::Return);
            }

            StmtKind::If {
                test,
                body,
                elif_clauses,
                else_body,
            } => {
                self.emit_if(test, body, elif_clauses, else_body, ctx, func);
            }

            StmtKind::While {
                test,
                body,
                else_body,
            } => {
                // block $break              ; `break` targets this (skips else)
                //   [block $normal]         ; only with an `else` clause
                //     loop $continue
                //       <test>; i32.eqz; br_if $normal (or $break w/o else)
                //       <body>
                //       br $continue
                //     end
                //   [end]
                //   [<else body>]           ; runs on NORMAL exit only —
                // end                       ; a `break` branches PAST it
                // B1: label depths are computed via the label stack, not
                // hardcoded, so break/continue in the body are correct at
                // any nesting depth (under if/try/nested loops).
                // Loop-`else` (B1 family): the else body runs when the loop
                // exits because the test became false, and is SKIPPED by
                // `break` — standard Python semantics. The normal-exit test
                // branches to the inner $normal block (falling into the else
                // body), while `break` targets the outer $break block.
                let has_else = else_body.is_some();
                let break_abs = ctx.push_label();
                func.instruction(&Instruction::Block(wasm_encoder::BlockType::Empty));
                let normal_abs = if has_else {
                    let n = ctx.push_label();
                    func.instruction(&Instruction::Block(wasm_encoder::BlockType::Empty));
                    n
                } else {
                    break_abs
                };
                let continue_abs = ctx.push_label();
                func.instruction(&Instruction::Loop(wasm_encoder::BlockType::Empty));
                ctx.loop_labels.push(LoopLabels {
                    break_abs,
                    continue_abs, // while: continue re-tests at the loop header
                });

                self.emit_condition(test, ctx, func);
                func.instruction(&Instruction::I32Eqz);
                func.instruction(&Instruction::BrIf(ctx.br_depth_to(normal_abs)));

                for s in body {
                    self.emit_stmt(s, ctx, func);
                }

                func.instruction(&Instruction::Br(ctx.br_depth_to(continue_abs)));
                ctx.loop_labels.pop();
                func.instruction(&Instruction::End); // end loop
                ctx.pop_label();
                if let Some(else_b) = else_body {
                    func.instruction(&Instruction::End); // end $normal block
                    ctx.pop_label();
                    // loop_labels was popped above, so a `break` inside the
                    // else body correctly binds to an OUTER loop (CPython).
                    for s in else_b {
                        self.emit_stmt(s, ctx, func);
                    }
                }
                func.instruction(&Instruction::End); // end block
                ctx.pop_label();
            }

            StmtKind::For {
                target,
                iter,
                body,
                else_body,
                ..
            } => {
                // range() vs collection iteration
                let is_range = matches!(
                    &iter.kind,
                    ExprKind::Call { func: callee, .. }
                        if matches!(&callee.kind, ExprKind::Name(n) if n == "range")
                );
                if is_range {
                    self.emit_for_range(target, iter, body, else_body, ctx, func);
                } else {
                    let iter_ty = self.expr_type(iter, ctx);
                    if matches!(iter_ty, WasmType::PtrList(_)) {
                        self.emit_for_list(target, iter, body, else_body, ctx, func);
                    } else {
                        // Fallback: try range path (gracefully degrades).
                        self.emit_for_range(target, iter, body, else_body, ctx, func);
                    }
                }
            }

            StmtKind::Break => {
                // B1: branch to the nearest enclosing loop's break block,
                // with the relative depth computed from the label stack —
                // correct under any intervening if/try/nested blocks (the
                // old hardcoded Br(1) assumed break sat DIRECTLY in the
                // loop body and miscompiled otherwise).
                if let Some(labels) = ctx.loop_labels.last().copied() {
                    func.instruction(&Instruction::Br(ctx.br_depth_to(labels.break_abs)));
                } else {
                    // `break` outside a loop is a Python SyntaxError; the
                    // parser rejects it upstream. Trap defensively rather
                    // than emit a wild branch.
                    func.instruction(&Instruction::Unreachable);
                }
            }

            StmtKind::Continue => {
                // B1: branch to the nearest enclosing loop's continue label
                // (loop header for while; body-end block for for-loops so
                // the increment still runs), depth from the label stack.
                if let Some(labels) = ctx.loop_labels.last().copied() {
                    func.instruction(&Instruction::Br(ctx.br_depth_to(labels.continue_abs)));
                } else {
                    func.instruction(&Instruction::Unreachable);
                }
            }

            StmtKind::Pass => {
                func.instruction(&Instruction::Nop);
            }

            StmtKind::Expr(expr) => {
                // Expression statement: emit and drop result if any
                self.emit_expr(expr, ctx, func);
                let ty = self.expr_type(expr, ctx);
                // Drop the result (it's on the stack but unused)
                if ty != WasmType::I32 || !matches!(expr.kind, ExprKind::Call { .. }) {
                    // For calls that return void, nothing to drop
                    // Otherwise drop the value
                    func.instruction(&Instruction::Drop);
                }
            }

            // Tier 7: error handling
            StmtKind::Raise(Some(exc), _) => {
                self.emit_raise(exc, ctx, func);
            }
            StmtKind::Assert { test, .. } => {
                // assert cond  â†’  if not cond: raise AssertionError
                self.emit_condition(test, ctx, func);
                func.instruction(&Instruction::I32Eqz);
                func.instruction(&Instruction::If(wasm_encoder::BlockType::Empty));
                self.emit_raise_code(exception_code("AssertionError").unwrap(), ctx, func);
                func.instruction(&Instruction::End);
            }
            StmtKind::Try { body, handlers, .. } => {
                self.emit_try(body, handlers, ctx, func);
            }

            _ => {
                // Should not reach here due to analysis filtering
            }
        }
    }

    /// Emit a `raise X` statement: set __err_code (and optionally __err_msg),
    /// emit a sentinel value of the function's return type, then return.
    fn emit_raise(&self, exc: &Expr, ctx: &mut FuncContext, func: &mut Function) {
        // Extract exception name and optional message argument.
        let (name, msg_arg): (&str, Option<&Expr>) = match &exc.kind {
            ExprKind::Name(n) => (n.as_str(), None),
            ExprKind::Call {
                func: callee, args, ..
            } => {
                let n = if let ExprKind::Name(n) = &callee.kind {
                    n.as_str()
                } else {
                    "Exception"
                };
                let m = args.first();
                (n, m)
            }
            _ => ("Exception", None),
        };
        let code = exception_code(name)
            .or_else(|| self.custom_exceptions.get(name).copied())
            .unwrap_or(7);

        // If a message argument is present and the module has string
        // infrastructure, evaluate it and stash the ptr in __err_msg.
        if let Some(m) = msg_arg {
            if self.needs_strings && self.needs_errors {
                let mt = self.expr_type(m, ctx);
                if matches!(mt, WasmType::Ptr) {
                    self.emit_expr(m, ctx, func);
                    func.instruction(&Instruction::GlobalSet(self.err_msg_global_idx));
                }
            }
        }

        self.emit_raise_code(code, ctx, func);
    }

    /// Emit code to set __err_code to the given value.
    ///
    /// If outside a `try` block, additionally push a sentinel return value
    /// and execute Return so the error propagates to the caller.
    ///
    /// If inside a `try` block (`ctx.try_depth > 0`), only set the error code
    /// and let the surrounding try's dispatch decide whether to br to a
    /// handler or propagate.
    /// CVE-2026-15903 fix — the shared, UNCONDITIONAL list-subscript bounds
    /// check + Python negative-index normalization.
    ///
    /// On entry the raw index value (of `idx_ty`) is on top of the stack and
    /// `list_temp` (i32) holds the list base pointer. On exit:
    ///   * `idx_temp` (i32) holds the validated element index (only meaningful
    ///     on the in-bounds path), and
    ///   * a single i32 `cond` is left on the stack, nonzero exactly when the
    ///     (normalized) index is out of range.
    ///
    /// The index is kept at full i64 width through the whole check, so an index
    /// that would `i32.wrap_i64` back in-range (`a[2**32]`) can no longer slip
    /// past it (F3). Negative indices are normalized from the end the way
    /// CPython does (`a[-1]` → `a[len-1]`) rather than being rejected or landing
    /// on the list header (F5). Because the surviving index satisfies
    /// `0 <= i < len` against the *true* stored length, the later
    /// `i * elem_size` address math cannot i32-wrap into another object (F4).
    /// `(idx64_temp, len64_temp)` are the caller's per-`sub_depth` i64 scratch
    /// pair, so nested reads never clobber this level's index/length.
    fn emit_list_index_check(
        &self,
        list_temp: u32,
        idx_temp: u32,
        idx64_temp: u32,
        len64_temp: u32,
        idx_ty: &WasmType,
        func: &mut Function,
    ) {
        // Raw index -> i64 (no narrowing yet).
        if *idx_ty != WasmType::I64 {
            self.emit_convert(idx_ty, &WasmType::I64, func);
        }
        func.instruction(&Instruction::LocalSet(idx64_temp));
        // len -> i64 (the length header is a non-negative i32).
        func.instruction(&Instruction::LocalGet(list_temp));
        func.instruction(&Instruction::I32Load(MemArg {
            offset: 0,
            align: 2,
            memory_index: 0,
        }));
        func.instruction(&Instruction::I64ExtendI32U);
        func.instruction(&Instruction::LocalSet(len64_temp));
        // Python negative normalization: if idx < 0 { idx += len }.
        func.instruction(&Instruction::LocalGet(idx64_temp));
        func.instruction(&Instruction::I64Const(0));
        func.instruction(&Instruction::I64LtS);
        func.instruction(&Instruction::If(wasm_encoder::BlockType::Empty));
        func.instruction(&Instruction::LocalGet(idx64_temp));
        func.instruction(&Instruction::LocalGet(len64_temp));
        func.instruction(&Instruction::I64Add);
        func.instruction(&Instruction::LocalSet(idx64_temp));
        func.instruction(&Instruction::End);
        // Narrowed element index for the in-bounds path (exact once 0<=i<len).
        func.instruction(&Instruction::LocalGet(idx64_temp));
        func.instruction(&Instruction::I32WrapI64);
        func.instruction(&Instruction::LocalSet(idx_temp));
        // cond = (idx64 < 0) | (idx64 >= len64), all at full i64 width.
        func.instruction(&Instruction::LocalGet(idx64_temp));
        func.instruction(&Instruction::I64Const(0));
        func.instruction(&Instruction::I64LtS);
        func.instruction(&Instruction::LocalGet(idx64_temp));
        func.instruction(&Instruction::LocalGet(len64_temp));
        func.instruction(&Instruction::I64GeS);
        func.instruction(&Instruction::I32Or);
    }

    /// Emit the out-of-bounds arm of a list-subscript bounds check. Either
    /// raises a catchable `IndexError` (when the module carries error infra) or
    /// traps (`unreachable`) — both are memory-safe and never touch the OOB
    /// address. A trap is caught by the glue's `WebAssembly.RuntimeError`
    /// fallback net and re-run on the exact JS twin, so the observable result is
    /// still Python's `IndexError`.
    fn emit_index_oob(&self, ctx: &FuncContext, func: &mut Function) {
        if self.needs_errors {
            self.emit_raise_code(exception_code("IndexError").unwrap(), ctx, func);
        } else {
            func.instruction(&Instruction::Unreachable);
        }
    }

    fn emit_raise_code(&self, code: i32, ctx: &FuncContext, func: &mut Function) {
        func.instruction(&Instruction::I32Const(code));
        func.instruction(&Instruction::GlobalSet(self.err_code_global_idx));
        if ctx.try_depth == 0 {
            // Push sentinel and return (propagate to caller)
            self.emit_sentinel_for(&ctx.return_type, func);
            func.instruction(&Instruction::Return);
        }
        // Inside try: caller will dispatch err_code after this stmt.
    }

    /// Push a zero/null sentinel matching the function's return type.
    /// Used by raise/assert and uncaught try-except propagation.
    fn emit_sentinel_for(&self, ret: &Option<WasmType>, func: &mut Function) {
        match ret {
            None => {}
            Some(WasmType::I64) => {
                func.instruction(&Instruction::I64Const(0));
            }
            Some(WasmType::F64) => {
                func.instruction(&Instruction::F64Const(0.0));
            }
            // Bool, string ptr, list ptr, dict handle, tuple ptr, closure ptr â€” all i32 sentinels
            Some(_) => {
                func.instruction(&Instruction::I32Const(0));
            }
        }
    }

    /// Emit a try/except. Body runs inside a labeled block; if any statement
    /// inside the body raises (i.e., __err_code becomes non-zero), control
    /// branches to a per-handler block. Each handler clears __err_code and
    /// runs its body.
    ///
    /// Layout (n handlers):
    /// ```text
    ///   block $end           (depth 0 = outermost)
    ///     block $h_n
    ///       ...
    ///       block $h_1
    ///         <body with err_dispatch after each statement>
    ///         br $end       (normal exit)
    ///       end $h_1
    ///       <handler 1 body>; br $end
    ///     ...
    ///     end $h_n
    ///     <handler n body>; (br $end if not last)
    ///   end $end
    /// ```
    /// After $end, an uncaught-error check pushes a sentinel and returns if
    /// __err_code is still set.
    fn emit_try(
        &self,
        body: &[Stmt],
        handlers: &[ExceptHandler],
        ctx: &mut FuncContext,
        func: &mut Function,
    ) {
        // Open all blocks: outer $end, then n handler blocks (innermost first)
        // B1: each is a structured label the body statements sit inside —
        // track them so break/continue in the try body (or handler bodies)
        // branch past them to the right enclosing-loop label.
        func.instruction(&Instruction::Block(wasm_encoder::BlockType::Empty)); // $end
        ctx.push_label();
        for _ in 0..handlers.len() {
            func.instruction(&Instruction::Block(wasm_encoder::BlockType::Empty));
            ctx.push_label();
        }

        // Emit body with err_code checks. raise inside try only sets err_code
        // (it does not return), so the dispatch can catch it.
        ctx.try_depth += 1;
        for s in body {
            self.emit_stmt(s, ctx, func);
            self.emit_err_dispatch(handlers, func);
        }
        ctx.try_depth -= 1;

        // Normal exit from try body â€” branch to $end (skipping handlers).
        func.instruction(&Instruction::Br(handlers.len() as u32));

        // Emit each handler.
        for (i, h) in handlers.iter().enumerate() {
            func.instruction(&Instruction::End); // close $h_<i+1>
            ctx.pop_label(); // B1: handler bodies sit one label shallower
                             // Clear err_code (handler caught it)
            func.instruction(&Instruction::I32Const(0));
            func.instruction(&Instruction::GlobalSet(self.err_code_global_idx));
            // Emit handler body
            for s in &h.body {
                self.emit_stmt(s, ctx, func);
            }
            // After handler, branch to $end (skipping any outer handler blocks).
            let depth_to_end = handlers.len() as u32 - i as u32 - 1;
            if depth_to_end > 0 {
                func.instruction(&Instruction::Br(depth_to_end));
            }
        }
        func.instruction(&Instruction::End); // close $end
        ctx.pop_label();

        // Post-try uncaught-error propagation: if err_code is still set after
        // exiting the try construct, this means an error occurred that no
        // handler matched. Push a sentinel and return.
        func.instruction(&Instruction::GlobalGet(self.err_code_global_idx));
        func.instruction(&Instruction::If(wasm_encoder::BlockType::Empty));
        self.emit_sentinel_for(&ctx.return_type, func);
        func.instruction(&Instruction::Return);
        func.instruction(&Instruction::End);
    }

    /// After a potentially-raising statement, check err_code and br to the
    /// matching handler. If err_code is 0, falls through normally.
    fn emit_err_dispatch(&self, handlers: &[ExceptHandler], func: &mut Function) {
        // if err_code != 0: dispatch
        func.instruction(&Instruction::GlobalGet(self.err_code_global_idx));
        func.instruction(&Instruction::If(wasm_encoder::BlockType::Empty));
        // Extract the exception name from a single `Name`/`Call(...)` form.
        fn one_name(e: &Expr) -> String {
            match &e.kind {
                ExprKind::Name(n) => n.clone(),
                ExprKind::Call { func: callee, .. } => match &callee.kind {
                    ExprKind::Name(n) => n.clone(),
                    _ => "Exception".to_string(),
                },
                _ => "Exception".to_string(),
            }
        }
        for (i, h) in handlers.iter().enumerate() {
            // Review finding 6: the set of exceptions this handler catches. A
            // TUPLE handler `except (A, B):` catches EACH listed type — it is
            // NOT a catch-all (the old `_ => "Exception"` treated it as one,
            // so `except (ValueError, KeyError)` wrongly swallowed IndexError).
            let names: Vec<String> = match &h.exc_type {
                None => vec!["Exception".to_string()],
                Some(e) => match &e.kind {
                    ExprKind::Tuple(elts) => elts.iter().map(one_name).collect(),
                    _ => vec![one_name(e)],
                },
            };
            // We're inside the if (depth 0). Innermost handler is at depth 1,
            // next at depth 2, etc.
            let depth = (i as u32) + 1;
            if names
                .iter()
                .any(|n| n == "Exception" || n == "BaseException")
            {
                // Catch-all.
                func.instruction(&Instruction::Br(depth));
            } else {
                // Branch to this handler if err_code matches ANY listed type.
                for name in &names {
                    let code =
                        exception_code(name).or_else(|| self.custom_exceptions.get(name).copied());
                    if let Some(code) = code {
                        func.instruction(&Instruction::GlobalGet(self.err_code_global_idx));
                        func.instruction(&Instruction::I32Const(code));
                        func.instruction(&Instruction::I32Eq);
                        func.instruction(&Instruction::BrIf(depth));
                    }
                }
            }
        }
        // Unhandled error: br to $end (depth = handlers.len() + 1).
        // err_code stays set; the post-$end check will propagate.
        func.instruction(&Instruction::Br((handlers.len() as u32) + 1));
        func.instruction(&Instruction::End); // end if
    }

    fn emit_if(
        &self,
        test: &Expr,
        body: &[Stmt],
        elif_clauses: &[(Expr, Vec<Stmt>)],
        else_body: &Option<Vec<Stmt>>,
        ctx: &mut FuncContext,
        func: &mut Function,
    ) {
        self.emit_condition(test, ctx, func);
        func.instruction(&Instruction::If(wasm_encoder::BlockType::Empty));
        // B1: an `if` is a structured label — track it so break/continue
        // emitted inside the arms compute the right relative depth.
        ctx.push_label();

        for s in body {
            self.emit_stmt(s, ctx, func);
        }

        if !elif_clauses.is_empty() || else_body.is_some() {
            func.instruction(&Instruction::Else);

            if !elif_clauses.is_empty() {
                let (elif_test, elif_body) = &elif_clauses[0];
                let remaining = &elif_clauses[1..];
                self.emit_if(elif_test, elif_body, remaining, else_body, ctx, func);
            } else if let Some(else_b) = else_body {
                for s in else_b {
                    self.emit_stmt(s, ctx, func);
                }
            }
        }

        func.instruction(&Instruction::End);
        ctx.pop_label();
    }

    /// Emit a for-range loop.
    /// Emit `for x in lst:` — index-based loop from 0 to len-1.
    fn emit_for_list(
        &self,
        target: &Expr,
        iter: &Expr,
        body: &[Stmt],
        else_body: &Option<Vec<Stmt>>,
        ctx: &mut FuncContext,
        func: &mut Function,
    ) {
        let loop_var = match &target.kind {
            ExprKind::Name(n) => n.clone(),
            _ => return,
        };
        let iter_ty = self.expr_type(iter, ctx);
        let elem_ty = match &iter_ty {
            WasmType::PtrList(inner) => (**inner).clone(),
            _ => return,
        };
        let elem_size = elem_ty.size_bytes();
        let var_idx = ctx.get_or_alloc_local(&loop_var, elem_ty.clone());
        // Use str_temps[1] for the list pointer (str_temps[0] often used by emit_expr).
        let ptr_temp = ctx.str_temps.get(1).copied().unwrap_or(0);
        let i_temp_name = format!("__for_i_{}", loop_var);
        let i_idx = ctx.get_or_alloc_local(&i_temp_name, WasmType::I32);
        let n_temp_name = format!("__for_n_{}", loop_var);
        let n_idx = ctx.get_or_alloc_local(&n_temp_name, WasmType::I32);

        // Save list ptr
        self.emit_expr(iter, ctx, func);
        func.instruction(&Instruction::LocalSet(ptr_temp));
        // Save length
        func.instruction(&Instruction::LocalGet(ptr_temp));
        func.instruction(&Instruction::I32Load(MemArg {
            offset: 0,
            align: 2,
            memory_index: 0,
        }));
        func.instruction(&Instruction::LocalSet(n_idx));
        // i = 0
        func.instruction(&Instruction::I32Const(0));
        func.instruction(&Instruction::LocalSet(i_idx));

        // B1 layout: block $break / loop $top / block $continue <body> end /
        // i++ / br $top. `continue` targets $continue's end so the increment
        // STILL RUNS (a br to $top would skip it → infinite loop); `break`
        // targets $break. Depths come from the label stack, correct at any
        // nesting.
        // Loop-`else` (B1 family): with an `else` clause an extra $normal
        // block sits between $break and $top; the exhausted-iterator exit
        // branches to $normal and falls into the else body, while `break`
        // targets $break and skips it (standard Python semantics).
        let has_else = else_body.is_some();
        let break_abs = ctx.push_label();
        func.instruction(&Instruction::Block(wasm_encoder::BlockType::Empty)); // $break
        let normal_abs = if has_else {
            let n = ctx.push_label();
            func.instruction(&Instruction::Block(wasm_encoder::BlockType::Empty)); // $normal
            n
        } else {
            break_abs
        };
        let top_abs = ctx.push_label();
        func.instruction(&Instruction::Loop(wasm_encoder::BlockType::Empty)); // $top
                                                                              // if i >= n: normal exit (runs the else body when present)
        func.instruction(&Instruction::LocalGet(i_idx));
        func.instruction(&Instruction::LocalGet(n_idx));
        func.instruction(&Instruction::I32GeS);
        func.instruction(&Instruction::BrIf(ctx.br_depth_to(normal_abs)));
        // var = list[i]: load element at ptr + 8 + i * elem_size
        func.instruction(&Instruction::LocalGet(ptr_temp));
        func.instruction(&Instruction::I32Const(8));
        func.instruction(&Instruction::I32Add);
        func.instruction(&Instruction::LocalGet(i_idx));
        if elem_size > 1 {
            func.instruction(&Instruction::I32Const(elem_size as i32));
            func.instruction(&Instruction::I32Mul);
        }
        func.instruction(&Instruction::I32Add);
        self.emit_load_at_offset(&elem_ty, 0, func);
        func.instruction(&Instruction::LocalSet(var_idx));
        // body, wrapped in the $continue block
        let continue_abs = ctx.push_label();
        func.instruction(&Instruction::Block(wasm_encoder::BlockType::Empty)); // $continue
        ctx.loop_labels.push(LoopLabels {
            break_abs,
            continue_abs,
        });
        for s in body {
            self.emit_stmt(s, ctx, func);
        }
        ctx.loop_labels.pop();
        func.instruction(&Instruction::End); // end $continue
        ctx.pop_label();
        // i++
        func.instruction(&Instruction::LocalGet(i_idx));
        func.instruction(&Instruction::I32Const(1));
        func.instruction(&Instruction::I32Add);
        func.instruction(&Instruction::LocalSet(i_idx));
        func.instruction(&Instruction::Br(ctx.br_depth_to(top_abs)));
        func.instruction(&Instruction::End); // loop
        ctx.pop_label();
        if let Some(else_b) = else_body {
            func.instruction(&Instruction::End); // end $normal block
            ctx.pop_label();
            // loop_labels was popped above, so a `break` in the else body
            // correctly binds to an OUTER loop (CPython semantics).
            for s in else_b {
                self.emit_stmt(s, ctx, func);
            }
        }
        func.instruction(&Instruction::End); // block
        ctx.pop_label();
    }

    fn emit_for_range(
        &self,
        target: &Expr,
        iter: &Expr,
        body: &[Stmt],
        else_body: &Option<Vec<Stmt>>,
        ctx: &mut FuncContext,
        func: &mut Function,
    ) {
        let loop_var = match &target.kind {
            ExprKind::Name(n) => n.clone(),
            _ => return,
        };

        // Extract range arguments
        let (start, stop, step) = match &iter.kind {
            ExprKind::Call { args, .. } => match args.len() {
                1 => (None, Some(&args[0]), None),
                2 => (Some(&args[0]), Some(&args[1]), None),
                3 => (Some(&args[0]), Some(&args[1]), Some(&args[2])),
                _ => return,
            },
            _ => return,
        };

        let loop_idx = ctx.get_or_alloc_local(&loop_var, WasmType::I64);

        // Allocate temp for stop value
        let stop_name = format!("__stop_{}", loop_var);
        let stop_idx = ctx.get_or_alloc_local(&stop_name, WasmType::I64);

        let step_name = format!("__step_{}", loop_var);
        let step_idx = ctx.get_or_alloc_local(&step_name, WasmType::I64);

        // Initialize loop var
        if let Some(start_expr) = start {
            self.emit_expr(start_expr, ctx, func);
            let st = self.expr_type(start_expr, ctx);
            if st != WasmType::I64 {
                self.emit_convert(&st, &WasmType::I64, func);
            }
        } else {
            func.instruction(&Instruction::I64Const(0));
        }
        func.instruction(&Instruction::LocalSet(loop_idx));

        // Initialize stop
        self.emit_expr(stop.unwrap(), ctx, func);
        let st = self.expr_type(stop.unwrap(), ctx);
        if st != WasmType::I64 {
            self.emit_convert(&st, &WasmType::I64, func);
        }
        func.instruction(&Instruction::LocalSet(stop_idx));

        // Initialize step
        if let Some(step_expr) = step {
            self.emit_expr(step_expr, ctx, func);
            let st = self.expr_type(step_expr, ctx);
            if st != WasmType::I64 {
                self.emit_convert(&st, &WasmType::I64, func);
            }
        } else {
            func.instruction(&Instruction::I64Const(1));
        }
        func.instruction(&Instruction::LocalSet(step_idx));

        // B1 layout:
        // block $break              ; `break` targets this (skips else)
        //   [block $normal]         ; only with an `else` clause
        //     loop $top
        //       i < stop ? (i64.lt_s)
        //       i32.eqz → br_if $normal (or $break w/o else)
        //       block $continue
        //         <body>          ; continue → br $continue (increment RUNS)
        //       end
        //       i += step
        //       br $top
        //     end
        //   [end]
        //   [<else body>]           ; runs on NORMAL (exhausted) exit only
        // end
        // Depths come from the label stack, correct at any nesting.
        // Loop-`else` (B1 family): the exhausted-range exit branches to
        // $normal and falls into the else body; `break` targets $break and
        // skips it (standard Python semantics).
        let has_else = else_body.is_some();
        let break_abs = ctx.push_label();
        func.instruction(&Instruction::Block(wasm_encoder::BlockType::Empty)); // $break
        let normal_abs = if has_else {
            let n = ctx.push_label();
            func.instruction(&Instruction::Block(wasm_encoder::BlockType::Empty)); // $normal
            n
        } else {
            break_abs
        };
        let top_abs = ctx.push_label();
        func.instruction(&Instruction::Loop(wasm_encoder::BlockType::Empty)); // $top

        // Loop condition. #364: a hardcoded `loop_var < stop` skipped the entire
        // loop for a NEGATIVE step — reverse `range(n-1, -1, -1)` produced zero
        // iterations (the real-code silent miscompile in `minOperations`,
        // sample_421). Specialize on the STATIC sign of the step so the common
        // ascending-constant case (incl. implicit step=1, the Livermore hot
        // path) keeps the original single `i64.lt_s` at ZERO extra cost; a
        // constant-negative step uses `i64.gt_s`; only a variable step (unknown
        // sign) pays the runtime select.
        let static_step_sign: Option<bool> = match step {
            // None (implicit) => +1.
            None => Some(true),
            Some(e) => match &e.kind {
                ExprKind::IntLiteral(v) if *v > 0 => Some(true),
                ExprKind::IntLiteral(v) if *v < 0 => Some(false),
                ExprKind::UnaryOp {
                    op: UnaryOp::Neg,
                    operand,
                } if matches!(&operand.kind, ExprKind::IntLiteral(v) if *v > 0) => Some(false),
                _ => None,
            },
        };
        match static_step_sign {
            Some(true) => {
                // Ascending: loop_var < stop (original fast path, unchanged).
                func.instruction(&Instruction::LocalGet(loop_idx));
                func.instruction(&Instruction::LocalGet(stop_idx));
                func.instruction(&Instruction::I64LtS);
            }
            Some(false) => {
                // Descending: loop_var > stop.
                func.instruction(&Instruction::LocalGet(loop_idx));
                func.instruction(&Instruction::LocalGet(stop_idx));
                func.instruction(&Instruction::I64GtS);
            }
            None => {
                // Variable step: (step < 0) ? (loop_var > stop) : (loop_var < stop).
                func.instruction(&Instruction::LocalGet(loop_idx));
                func.instruction(&Instruction::LocalGet(stop_idx));
                func.instruction(&Instruction::I64GtS); // descending test
                func.instruction(&Instruction::LocalGet(loop_idx));
                func.instruction(&Instruction::LocalGet(stop_idx));
                func.instruction(&Instruction::I64LtS); // ascending test
                func.instruction(&Instruction::LocalGet(step_idx));
                func.instruction(&Instruction::I64Const(0));
                func.instruction(&Instruction::I64LtS); // selector: step < 0
                func.instruction(&Instruction::Select); // step<0 ? gt_s : lt_s
            }
        }
        func.instruction(&Instruction::I32Eqz);
        // Normal (exhausted-range) exit — falls into the else body if present.
        func.instruction(&Instruction::BrIf(ctx.br_depth_to(normal_abs)));

        // Body, wrapped in the $continue block
        let continue_abs = ctx.push_label();
        func.instruction(&Instruction::Block(wasm_encoder::BlockType::Empty)); // $continue
        ctx.loop_labels.push(LoopLabels {
            break_abs,
            continue_abs,
        });
        for s in body {
            self.emit_stmt(s, ctx, func);
        }
        ctx.loop_labels.pop();
        func.instruction(&Instruction::End); // end $continue
        ctx.pop_label();

        // Increment: loop_var += step
        func.instruction(&Instruction::LocalGet(loop_idx));
        func.instruction(&Instruction::LocalGet(step_idx));
        func.instruction(&Instruction::I64Add);
        func.instruction(&Instruction::LocalSet(loop_idx));

        func.instruction(&Instruction::Br(ctx.br_depth_to(top_abs))); // back to header
        func.instruction(&Instruction::End); // end loop
        ctx.pop_label();
        if let Some(else_b) = else_body {
            func.instruction(&Instruction::End); // end $normal block
            ctx.pop_label();
            // loop_labels was popped above, so a `break` in the else body
            // correctly binds to an OUTER loop (CPython semantics).
            for s in else_b {
                self.emit_stmt(s, ctx, func);
            }
        }
        func.instruction(&Instruction::End); // end block
        ctx.pop_label();
    }

    // === Expression emission ===

    fn emit_expr(&self, expr: &Expr, ctx: &mut FuncContext, func: &mut Function) {
        match &expr.kind {
            ExprKind::IntLiteral(n) => {
                func.instruction(&Instruction::I64Const(*n as i64));
            }

            ExprKind::FloatLiteral(n) => {
                func.instruction(&Instruction::F64Const(*n));
            }

            ExprKind::BoolLiteral(b) => {
                func.instruction(&Instruction::I32Const(if *b { 1 } else { 0 }));
            }

            ExprKind::StringLiteral(s) => {
                if let Some(&offset) = self.string_dedup.get(s.as_str()) {
                    func.instruction(&Instruction::I32Const(offset as i32));
                } else {
                    func.instruction(&Instruction::I32Const(0));
                }
            }

            ExprKind::FString { parts } => {
                self.emit_fstring(parts, ctx, func);
            }

            ExprKind::Name(name) => {
                // Tier 6: inside a lambda body, captures are loaded from
                // env_ptr (implicit local 0) at compile-time-known offsets.
                if let Some(cap) = ctx.captures.iter().find(|c| c.name == *name) {
                    func.instruction(&Instruction::LocalGet(0)); // env_ptr
                    self.emit_load_at_offset(&cap.ty, cap.offset, func);
                    return;
                }
                if let Some((idx, _)) = ctx.get_local(name) {
                    func.instruction(&Instruction::LocalGet(idx));
                }
            }

            ExprKind::BinOp { left, op, right } => {
                self.emit_binop(left, *op, right, ctx, func);
            }

            ExprKind::UnaryOp { op, operand } => {
                self.emit_unaryop(*op, operand, ctx, func);
            }

            ExprKind::Compare { left, comparisons } => {
                self.emit_compare(left, comparisons, ctx, func);
            }

            ExprKind::Call {
                func: callee, args, ..
            } => {
                self.emit_call(callee, args, ctx, func);
            }

            ExprKind::IfExpr {
                test,
                body,
                else_body,
            } => {
                self.emit_if_expr(test, body, else_body, ctx, func);
            }

            ExprKind::Subscript { value, index, .. } => {
                let val_type = self.expr_type(value, ctx);
                match &val_type {
                    WasmType::Ptr => {
                        self.emit_string_index(value, index, ctx, func);
                    }
                    WasmType::PtrTuple(elt_types) => {
                        // Static-offset tuple indexing — only constant integer
                        // indices are supported (Python tuples are statically
                        // shaped at compile time).
                        if let ExprKind::IntLiteral(i) = &index.kind {
                            let idx = *i as usize;
                            if idx < elt_types.len() {
                                let mut offset: u32 = 0;
                                for elt in elt_types.iter().take(idx) {
                                    offset += elt.size_bytes();
                                }
                                let ty = &elt_types[idx];
                                self.emit_expr(value, ctx, func);
                                self.emit_load_at_offset(ty, offset, func);
                                return;
                            }
                        }
                        // Non-constant index — push 0 sentinel.
                        func.instruction(&Instruction::I64Const(0));
                    }
                    WasmType::PtrList(elem_ty) => {
                        // List indexing: `lst[i]` — load element at
                        // ptr + 8 + i * elem_size. CVE-2026-15903 fix: the
                        // bounds check is now UNCONDITIONAL (no `needs_errors`
                        // gate — F2), tests the FULL i64 index before the
                        // i32 narrowing (F3/F4), and normalizes Python negative
                        // indices from the end (F5). Out of range never reaches
                        // the load; it raises `IndexError` or traps.
                        let elem_size = elem_ty.size_bytes();
                        let (list_temp, idx_temp) = match ctx.sub_scratch.get(ctx.sub_depth) {
                            Some(&pair) => pair,
                            None => {
                                // Unreachable by construction: the scratch pool
                                // is pre-sized to the body's measured subscript
                                // nesting (see `emit_function` / `emit_lambda_
                                // function`) and functions deeper than
                                // WASM_MAX_SUBSCRIPT_NESTING are rejected from
                                // WASM in `pyths_hir::wasm_analysis`. Guard
                                // defensively anyway: emit a runtime trap rather
                                // than panicking (crash) or clobbering another
                                // pair (silent miscompile — the k14 bug class).
                                debug_assert!(
                                    false,
                                    "list-subscript sub_depth {} exceeds pre-sized pool of {} — \
                                     eligibility pre-scan and emit disagree",
                                    ctx.sub_depth,
                                    ctx.sub_scratch.len()
                                );
                                func.instruction(&Instruction::Unreachable);
                                return;
                            }
                        };
                        // The parallel i64 bounds-check pair for this depth
                        // (normalized-index, length). Same pre-sizing guarantee.
                        let (idx64_temp, len64_temp) = match ctx.sub_scratch_i64.get(ctx.sub_depth)
                        {
                            Some(&pair) => pair,
                            None => {
                                debug_assert!(false, "sub_scratch_i64 pool undersized");
                                func.instruction(&Instruction::Unreachable);
                                return;
                            }
                        };

                        // Save lst ptr to temp. Emitting the container and
                        // index sub-expressions may itself contain nested
                        // list reads — bump sub_depth so they take their
                        // OWN scratch pair instead of clobbering this one
                        // (the Livermore k14 silent miscompile).
                        ctx.sub_depth += 1;
                        self.emit_expr(value, ctx, func);
                        ctx.sub_depth -= 1;
                        func.instruction(&Instruction::LocalSet(list_temp));
                        // Push the raw index (i64-width preserved) then run the
                        // shared normalize+bounds check. Leaves `cond` on stack.
                        ctx.sub_depth += 1;
                        self.emit_expr(index, ctx, func);
                        ctx.sub_depth -= 1;
                        let idx_ty = self.expr_type(index, ctx);
                        self.emit_list_index_check(
                            list_temp, idx_temp, idx64_temp, len64_temp, &idx_ty, func,
                        );

                        // `if cond { OOB } else { load }`, result-typed so the
                        // in-try case (where raise does not return) still leaves
                        // exactly one value and never falls through to the load.
                        let result_vt = elem_ty.to_val_type();
                        func.instruction(&Instruction::If(wasm_encoder::BlockType::Result(
                            result_vt,
                        )));
                        // OOB arm: raise/trap. If raise stays in-function (inside
                        // a try), leave a sentinel of the element type so the
                        // block result type is satisfied; the surrounding try
                        // dispatch discards it.
                        self.emit_index_oob(ctx, func);
                        if self.needs_errors && ctx.try_depth > 0 {
                            self.emit_sentinel_for(&Some((**elem_ty).clone()), func);
                        }
                        func.instruction(&Instruction::Else);
                        // In-bounds load: ptr + 8 + i * elem_size (i now exact).
                        func.instruction(&Instruction::LocalGet(list_temp));
                        func.instruction(&Instruction::I32Const(8));
                        func.instruction(&Instruction::I32Add);
                        func.instruction(&Instruction::LocalGet(idx_temp));
                        if elem_size > 1 {
                            func.instruction(&Instruction::I32Const(elem_size as i32));
                            func.instruction(&Instruction::I32Mul);
                        }
                        func.instruction(&Instruction::I32Add);
                        self.emit_load_at_offset(elem_ty, 0, func);
                        func.instruction(&Instruction::End);
                    }
                    WasmType::PtrDict(_, _) => {
                        // Dict indexing: __dict_get_str(handle, key_ptr)
                        if let Some(&idx) = self.import_indices.get("__dict_get_str") {
                            self.emit_expr(value, ctx, func);
                            self.emit_expr(index, ctx, func);
                            func.instruction(&Instruction::Call(idx));
                        } else {
                            func.instruction(&Instruction::I64Const(0));
                        }
                    }
                    _ => {
                        func.instruction(&Instruction::I32Const(0));
                    }
                }
            }

            ExprKind::Attribute { value, attr, .. } => {
                // Math constants like math.pi -> inline f64.const
                if let ExprKind::Name(mod_name) = &value.kind {
                    if mod_name == "math" {
                        if let Some(v) = math_constant_value(attr.as_str()) {
                            func.instruction(&Instruction::F64Const(v));
                            return;
                        }
                    }
                }
                // Unsupported attribute -- fall through to dummy
                func.instruction(&Instruction::I64Const(0));
            }

            // Step 2 -- Tier 5: Tuple literal `(a, b, c)`.
            // Layout: elements packed inline at natural alignment.
            ExprKind::Tuple(elts) => {
                self.emit_tuple_literal(elts, ctx, func);
            }

            // Step 3 -- Tier 2: List literal `[a, b, c]`.
            // Layout: [i32 length][i32 capacity][elements...]
            ExprKind::List(elts) => {
                self.emit_list_literal(elts, ctx, func);
            }

            // Step 4 -- Tier 4: Dict literal. Calls __dict.new() then
            // __dict.set_str(handle, k, v) per item. Returns the handle.
            ExprKind::Dict { items } => {
                self.emit_dict_literal(items, ctx, func);
            }

            // Step 6 -- Tier 6: Lambda. Each lambda was collected during
            // emit_module and assigned a table slot. The closure value is
            // a heap-allocated 8-byte struct `[i32 func_idx][i32 env_ptr]`.
            // For lambdas with captures, env_ptr points to a packed env
            // tuple holding the captured values. For no-capture lambdas,
            // env_ptr is 0.
            ExprKind::Lambda { .. } => {
                let idx = self.next_lambda_emit_idx.get();
                self.next_lambda_emit_idx.set(idx + 1);
                self.emit_closure_alloc(idx, ctx, func);
            }

            // Step 6 -- list/dict/set comprehension stubs.
            ExprKind::ListComp { .. } | ExprKind::DictComp { .. } | ExprKind::SetComp { .. } => {
                func.instruction(&Instruction::I32Const(0));
            }

            _ => {
                // Unsupported expression -- shouldn't reach here due to analysis
                func.instruction(&Instruction::I64Const(0));
            }
        }
    }

    /// Emit a load instruction matching `ty`'s WASM representation, reading
    /// from `[ptr + offset]` where `ptr` is on top of the stack.
    fn emit_load_at_offset(&self, ty: &WasmType, offset: u32, func: &mut Function) {
        match ty {
            WasmType::I64 => {
                func.instruction(&Instruction::I64Load(MemArg {
                    offset: offset as u64,
                    align: 3,
                    memory_index: 0,
                }));
            }
            WasmType::F64 => {
                func.instruction(&Instruction::F64Load(MemArg {
                    offset: offset as u64,
                    align: 3,
                    memory_index: 0,
                }));
            }
            _ => {
                // i32 representation (bool, ptr, list, dict, tuple, closure)
                func.instruction(&Instruction::I32Load(MemArg {
                    offset: offset as u64,
                    align: 2,
                    memory_index: 0,
                }));
            }
        }
    }

    /// Emit a dict literal: __dict.new() then __dict.set_str() per item.
    fn emit_dict_literal(
        &self,
        items: &[pyths_syntax::ast::DictItem],
        ctx: &mut FuncContext,
        func: &mut Function,
    ) {
        let new_idx = match self.import_indices.get("__dict_new") {
            Some(&i) => i,
            None => {
                func.instruction(&Instruction::I32Const(0));
                return;
            }
        };
        let set_idx = self.import_indices.get("__dict_set_str").copied();

        // handle = __dict.new()
        func.instruction(&Instruction::Call(new_idx));
        // Save handle to a temp.
        let ptr_temp = ctx.str_temps.first().copied().unwrap_or(0);
        func.instruction(&Instruction::LocalSet(ptr_temp));

        // For each KeyValue, set_str(handle, key_ptr, value_i64).
        if let Some(set_idx) = set_idx {
            for item in items {
                if let pyths_syntax::ast::DictItem::KeyValue { key, value } = item {
                    func.instruction(&Instruction::LocalGet(ptr_temp));
                    self.emit_expr(key, ctx, func);
                    self.emit_expr(value, ctx, func);
                    let val_ty = self.expr_type(value, ctx);
                    if val_ty != WasmType::I64 {
                        self.emit_convert(&val_ty, &WasmType::I64, func);
                    }
                    func.instruction(&Instruction::Call(set_idx));
                }
            }
        }
        // Push handle as the result.
        func.instruction(&Instruction::LocalGet(ptr_temp));
    }

    /// Tier 6 HoF: `map(fn, lst)` → new list of `fn(x) for x in lst`.
    /// Requires `fn` to be a closure-typed local and `lst` to be a list.
    fn emit_map(&self, args: &[Expr], ctx: &mut FuncContext, func: &mut Function) {
        if args.len() != 2 {
            func.instruction(&Instruction::I32Const(0));
            return;
        }
        let fn_ty = self.expr_type(&args[0], ctx);
        let lst_ty = self.expr_type(&args[1], ctx);
        let (sig, _params, ret) = match &fn_ty {
            WasmType::PtrClosure { params, ret } => (
                ClosureSig {
                    params: params.clone(),
                    ret: ret.as_ref().map(|b| (**b).clone()),
                },
                params.clone(),
                ret.as_ref().map(|b| (**b).clone()),
            ),
            _ => {
                func.instruction(&Instruction::I32Const(0));
                return;
            }
        };
        let in_elem = match &lst_ty {
            WasmType::PtrList(t) => (**t).clone(),
            _ => {
                func.instruction(&Instruction::I32Const(0));
                return;
            }
        };
        let out_elem = ret.unwrap_or(in_elem.clone());
        let in_size = in_elem.size_bytes();
        let out_size = out_elem.size_bytes();
        let type_idx = match self.closure_type_indices.get(&sig) {
            Some(&i) => i,
            None => {
                func.instruction(&Instruction::I32Const(0));
                return;
            }
        };
        let fn_t = ctx.str_temps.get(3).copied().unwrap_or(0);
        let lst_t = ctx.str_temps.get(4).copied().unwrap_or(0);
        let res_t = ctx.str_temps.get(5).copied().unwrap_or(0);
        let i_t = ctx.str_temps.get(6).copied().unwrap_or(0);
        let n_t = ctx.str_temps.get(7).copied().unwrap_or(0);

        // Save fn closure ptr, lst ptr.
        self.emit_expr(&args[0], ctx, func);
        func.instruction(&Instruction::LocalSet(fn_t));
        self.emit_expr(&args[1], ctx, func);
        func.instruction(&Instruction::LocalSet(lst_t));
        // n = lst.length
        func.instruction(&Instruction::LocalGet(lst_t));
        func.instruction(&Instruction::I32Load(MemArg {
            offset: 0,
            align: 2,
            memory_index: 0,
        }));
        func.instruction(&Instruction::LocalSet(n_t));
        // Allocate result list: 8 + n * out_size
        func.instruction(&Instruction::LocalGet(n_t));
        func.instruction(&Instruction::I32Const(out_size as i32));
        func.instruction(&Instruction::I32Mul);
        func.instruction(&Instruction::I32Const(8));
        func.instruction(&Instruction::I32Add);
        func.instruction(&Instruction::Call(self.alloc_func_idx));
        func.instruction(&Instruction::LocalSet(res_t));
        // Store length, capacity.
        func.instruction(&Instruction::LocalGet(res_t));
        func.instruction(&Instruction::LocalGet(n_t));
        func.instruction(&Instruction::I32Store(MemArg {
            offset: 0,
            align: 2,
            memory_index: 0,
        }));
        func.instruction(&Instruction::LocalGet(res_t));
        func.instruction(&Instruction::LocalGet(n_t));
        func.instruction(&Instruction::I32Store(MemArg {
            offset: 4,
            align: 2,
            memory_index: 0,
        }));
        // i = 0
        func.instruction(&Instruction::I32Const(0));
        func.instruction(&Instruction::LocalSet(i_t));
        // block + loop
        func.instruction(&Instruction::Block(wasm_encoder::BlockType::Empty));
        func.instruction(&Instruction::Loop(wasm_encoder::BlockType::Empty));
        // if i >= n: break
        func.instruction(&Instruction::LocalGet(i_t));
        func.instruction(&Instruction::LocalGet(n_t));
        func.instruction(&Instruction::I32GeS);
        func.instruction(&Instruction::BrIf(1));
        // Compute write address: res + 8 + i * out_size
        func.instruction(&Instruction::LocalGet(res_t));
        func.instruction(&Instruction::I32Const(8));
        func.instruction(&Instruction::I32Add);
        func.instruction(&Instruction::LocalGet(i_t));
        if out_size > 1 {
            func.instruction(&Instruction::I32Const(out_size as i32));
            func.instruction(&Instruction::I32Mul);
        }
        func.instruction(&Instruction::I32Add);
        // Push env_ptr (closure[4]).
        func.instruction(&Instruction::LocalGet(fn_t));
        func.instruction(&Instruction::I32Load(MemArg {
            offset: 4,
            align: 2,
            memory_index: 0,
        }));
        // Push lst[i]
        func.instruction(&Instruction::LocalGet(lst_t));
        func.instruction(&Instruction::I32Const(8));
        func.instruction(&Instruction::I32Add);
        func.instruction(&Instruction::LocalGet(i_t));
        if in_size > 1 {
            func.instruction(&Instruction::I32Const(in_size as i32));
            func.instruction(&Instruction::I32Mul);
        }
        func.instruction(&Instruction::I32Add);
        self.emit_load_at_offset(&in_elem, 0, func);
        // Push func_idx (closure[0]) then call_indirect
        func.instruction(&Instruction::LocalGet(fn_t));
        func.instruction(&Instruction::I32Load(MemArg {
            offset: 0,
            align: 2,
            memory_index: 0,
        }));
        func.instruction(&Instruction::CallIndirect {
            type_index: type_idx,
            table_index: 0,
        });
        // Stack: write_addr, output_value → store
        match &out_elem {
            WasmType::I64 => func.instruction(&Instruction::I64Store(MemArg {
                offset: 0,
                align: 3,
                memory_index: 0,
            })),
            WasmType::F64 => func.instruction(&Instruction::F64Store(MemArg {
                offset: 0,
                align: 3,
                memory_index: 0,
            })),
            _ => func.instruction(&Instruction::I32Store(MemArg {
                offset: 0,
                align: 2,
                memory_index: 0,
            })),
        };
        // i++
        func.instruction(&Instruction::LocalGet(i_t));
        func.instruction(&Instruction::I32Const(1));
        func.instruction(&Instruction::I32Add);
        func.instruction(&Instruction::LocalSet(i_t));
        func.instruction(&Instruction::Br(0));
        func.instruction(&Instruction::End); // loop
        func.instruction(&Instruction::End); // block
        func.instruction(&Instruction::LocalGet(res_t));
    }

    /// Tier 6 HoF: `filter(fn, lst)` → new list of `x for x in lst if fn(x)`.
    /// Allocates worst-case (full length) and trims length at the end.
    fn emit_filter(&self, args: &[Expr], ctx: &mut FuncContext, func: &mut Function) {
        if args.len() != 2 {
            func.instruction(&Instruction::I32Const(0));
            return;
        }
        let fn_ty = self.expr_type(&args[0], ctx);
        let lst_ty = self.expr_type(&args[1], ctx);
        let (sig, _params) = match &fn_ty {
            WasmType::PtrClosure { params, ret } => (
                ClosureSig {
                    params: params.clone(),
                    ret: ret.as_ref().map(|b| (**b).clone()),
                },
                params.clone(),
            ),
            _ => {
                func.instruction(&Instruction::I32Const(0));
                return;
            }
        };
        let elem = match &lst_ty {
            WasmType::PtrList(t) => (**t).clone(),
            _ => {
                func.instruction(&Instruction::I32Const(0));
                return;
            }
        };
        let elem_size = elem.size_bytes();
        let type_idx = match self.closure_type_indices.get(&sig) {
            Some(&i) => i,
            None => {
                func.instruction(&Instruction::I32Const(0));
                return;
            }
        };
        let fn_t = ctx.str_temps.get(3).copied().unwrap_or(0);
        let lst_t = ctx.str_temps.get(4).copied().unwrap_or(0);
        let res_t = ctx.str_temps.get(5).copied().unwrap_or(0);
        let i_t = ctx.str_temps.get(6).copied().unwrap_or(0);
        let n_t = ctx.str_temps.get(7).copied().unwrap_or(0);
        // out_count uses save slot
        let cnt_t = ctx.str_saves.first().copied().unwrap_or(0);

        self.emit_expr(&args[0], ctx, func);
        func.instruction(&Instruction::LocalSet(fn_t));
        self.emit_expr(&args[1], ctx, func);
        func.instruction(&Instruction::LocalSet(lst_t));
        // n = lst.length
        func.instruction(&Instruction::LocalGet(lst_t));
        func.instruction(&Instruction::I32Load(MemArg {
            offset: 0,
            align: 2,
            memory_index: 0,
        }));
        func.instruction(&Instruction::LocalSet(n_t));
        // Allocate worst-case result: 8 + n * elem_size
        func.instruction(&Instruction::LocalGet(n_t));
        func.instruction(&Instruction::I32Const(elem_size as i32));
        func.instruction(&Instruction::I32Mul);
        func.instruction(&Instruction::I32Const(8));
        func.instruction(&Instruction::I32Add);
        func.instruction(&Instruction::Call(self.alloc_func_idx));
        func.instruction(&Instruction::LocalSet(res_t));
        // capacity = n
        func.instruction(&Instruction::LocalGet(res_t));
        func.instruction(&Instruction::LocalGet(n_t));
        func.instruction(&Instruction::I32Store(MemArg {
            offset: 4,
            align: 2,
            memory_index: 0,
        }));
        // out_count = 0
        func.instruction(&Instruction::I32Const(0));
        func.instruction(&Instruction::LocalSet(cnt_t));
        // i = 0
        func.instruction(&Instruction::I32Const(0));
        func.instruction(&Instruction::LocalSet(i_t));
        func.instruction(&Instruction::Block(wasm_encoder::BlockType::Empty));
        func.instruction(&Instruction::Loop(wasm_encoder::BlockType::Empty));
        func.instruction(&Instruction::LocalGet(i_t));
        func.instruction(&Instruction::LocalGet(n_t));
        func.instruction(&Instruction::I32GeS);
        func.instruction(&Instruction::BrIf(1));
        // Push env_ptr, then x, then func_idx, call_indirect → returns bool (i32)
        func.instruction(&Instruction::LocalGet(fn_t));
        func.instruction(&Instruction::I32Load(MemArg {
            offset: 4,
            align: 2,
            memory_index: 0,
        }));
        // x = lst[i]
        func.instruction(&Instruction::LocalGet(lst_t));
        func.instruction(&Instruction::I32Const(8));
        func.instruction(&Instruction::I32Add);
        func.instruction(&Instruction::LocalGet(i_t));
        if elem_size > 1 {
            func.instruction(&Instruction::I32Const(elem_size as i32));
            func.instruction(&Instruction::I32Mul);
        }
        func.instruction(&Instruction::I32Add);
        self.emit_load_at_offset(&elem, 0, func);
        // func_idx
        func.instruction(&Instruction::LocalGet(fn_t));
        func.instruction(&Instruction::I32Load(MemArg {
            offset: 0,
            align: 2,
            memory_index: 0,
        }));
        func.instruction(&Instruction::CallIndirect {
            type_index: type_idx,
            table_index: 0,
        });
        // Result is i32 (bool). If non-zero, append x to result.
        func.instruction(&Instruction::If(wasm_encoder::BlockType::Empty));
        // Compute write addr: res + 8 + cnt * elem_size
        func.instruction(&Instruction::LocalGet(res_t));
        func.instruction(&Instruction::I32Const(8));
        func.instruction(&Instruction::I32Add);
        func.instruction(&Instruction::LocalGet(cnt_t));
        if elem_size > 1 {
            func.instruction(&Instruction::I32Const(elem_size as i32));
            func.instruction(&Instruction::I32Mul);
        }
        func.instruction(&Instruction::I32Add);
        // Push x = lst[i] again
        func.instruction(&Instruction::LocalGet(lst_t));
        func.instruction(&Instruction::I32Const(8));
        func.instruction(&Instruction::I32Add);
        func.instruction(&Instruction::LocalGet(i_t));
        if elem_size > 1 {
            func.instruction(&Instruction::I32Const(elem_size as i32));
            func.instruction(&Instruction::I32Mul);
        }
        func.instruction(&Instruction::I32Add);
        self.emit_load_at_offset(&elem, 0, func);
        // Store
        match &elem {
            WasmType::I64 => func.instruction(&Instruction::I64Store(MemArg {
                offset: 0,
                align: 3,
                memory_index: 0,
            })),
            WasmType::F64 => func.instruction(&Instruction::F64Store(MemArg {
                offset: 0,
                align: 3,
                memory_index: 0,
            })),
            _ => func.instruction(&Instruction::I32Store(MemArg {
                offset: 0,
                align: 2,
                memory_index: 0,
            })),
        };
        // cnt++
        func.instruction(&Instruction::LocalGet(cnt_t));
        func.instruction(&Instruction::I32Const(1));
        func.instruction(&Instruction::I32Add);
        func.instruction(&Instruction::LocalSet(cnt_t));
        func.instruction(&Instruction::End); // if
                                             // i++
        func.instruction(&Instruction::LocalGet(i_t));
        func.instruction(&Instruction::I32Const(1));
        func.instruction(&Instruction::I32Add);
        func.instruction(&Instruction::LocalSet(i_t));
        func.instruction(&Instruction::Br(0));
        func.instruction(&Instruction::End); // loop
        func.instruction(&Instruction::End); // block
                                             // Set length = cnt
        func.instruction(&Instruction::LocalGet(res_t));
        func.instruction(&Instruction::LocalGet(cnt_t));
        func.instruction(&Instruction::I32Store(MemArg {
            offset: 0,
            align: 2,
            memory_index: 0,
        }));
        func.instruction(&Instruction::LocalGet(res_t));
    }

    /// Tier 6 HoF: `reduce(fn, iter, init)` → fold.
    /// `result = init; for x in iter: result = fn(result, x); return result`.
    fn emit_reduce(&self, args: &[Expr], ctx: &mut FuncContext, func: &mut Function) {
        if args.len() != 3 {
            func.instruction(&Instruction::I64Const(0));
            return;
        }
        let fn_ty = self.expr_type(&args[0], ctx);
        let lst_ty = self.expr_type(&args[1], ctx);
        let init_ty = self.expr_type(&args[2], ctx);
        let lst_elem = match &lst_ty {
            WasmType::PtrList(t) => (**t).clone(),
            _ => {
                func.instruction(&Instruction::I64Const(0));
                return;
            }
        };
        let (sig, _params, ret) = match &fn_ty {
            WasmType::PtrClosure { params, ret } => {
                // Unannotated lambdas default their params to I64 in
                // `expr_type`, but the lambda's actual emitted body —
                // collected via `collect_lambdas_in_expr_scoped_with_overrides`
                // — uses the inferred `(init_ty, elem_ty)` types when
                // this call is a HoF. Reconstruct the sig from that
                // context so `closure_type_indices.get` matches the
                // registered key.
                let want_override = params.len() == 2
                    && params.iter().all(|p| matches!(p, WasmType::I64))
                    && (lst_elem != WasmType::I64 || init_ty != WasmType::I64);
                let new_params: Vec<WasmType> = if want_override {
                    vec![init_ty.clone(), lst_elem.clone()]
                } else {
                    params.clone()
                };
                let new_ret = if want_override {
                    Some(init_ty.clone())
                } else {
                    ret.as_ref().map(|b| (**b).clone())
                };
                (
                    ClosureSig {
                        params: new_params.clone(),
                        ret: new_ret.clone(),
                    },
                    new_params,
                    new_ret,
                )
            }
            _ => {
                func.instruction(&Instruction::I64Const(0));
                return;
            }
        };
        let elem = lst_elem;
        let elem_size = elem.size_bytes();
        let acc_ty = ret.unwrap_or(elem.clone());
        let type_idx = match self.closure_type_indices.get(&sig) {
            Some(&i) => i,
            None => {
                func.instruction(&Instruction::I64Const(0));
                return;
            }
        };
        let fn_t = ctx.str_temps.get(3).copied().unwrap_or(0);
        let lst_t = ctx.str_temps.get(4).copied().unwrap_or(0);
        let i_t = ctx.str_temps.get(6).copied().unwrap_or(0);
        let n_t = ctx.str_temps.get(7).copied().unwrap_or(0);
        // The accumulator uses a typed local pre-allocated by collect_locals
        // when reduce() is detected: __reduce_i64 (most common) or __reduce_f64.
        // i32 / Ptr accumulators reuse str_temps[5].
        let i32_acc = ctx.str_temps.get(5).copied().unwrap_or(0);
        let i64_acc = ctx.get_local("__reduce_i64").map(|(i, _)| i);
        let f64_acc = ctx.get_local("__reduce_f64").map(|(i, _)| i);

        self.emit_expr(&args[0], ctx, func);
        func.instruction(&Instruction::LocalSet(fn_t));
        self.emit_expr(&args[1], ctx, func);
        func.instruction(&Instruction::LocalSet(lst_t));
        func.instruction(&Instruction::LocalGet(lst_t));
        func.instruction(&Instruction::I32Load(MemArg {
            offset: 0,
            align: 2,
            memory_index: 0,
        }));
        func.instruction(&Instruction::LocalSet(n_t));

        // Pick the right accumulator local for acc_ty. Returns the local idx
        // and a setter/getter pair via match in subsequent code.
        let acc_idx = match &acc_ty {
            WasmType::I64 => i64_acc.unwrap_or(i32_acc),
            WasmType::F64 => f64_acc.unwrap_or(i32_acc),
            _ => i32_acc,
        };

        // init → acc
        self.emit_expr(&args[2], ctx, func);
        let init_ty = self.expr_type(&args[2], ctx);
        if init_ty != acc_ty {
            self.emit_convert(&init_ty, &acc_ty, func);
        }
        func.instruction(&Instruction::LocalSet(acc_idx));
        // i = 0
        func.instruction(&Instruction::I32Const(0));
        func.instruction(&Instruction::LocalSet(i_t));
        func.instruction(&Instruction::Block(wasm_encoder::BlockType::Empty));
        func.instruction(&Instruction::Loop(wasm_encoder::BlockType::Empty));
        func.instruction(&Instruction::LocalGet(i_t));
        func.instruction(&Instruction::LocalGet(n_t));
        func.instruction(&Instruction::I32GeS);
        func.instruction(&Instruction::BrIf(1));
        // call: env_ptr, acc, x, func_idx
        func.instruction(&Instruction::LocalGet(fn_t));
        func.instruction(&Instruction::I32Load(MemArg {
            offset: 4,
            align: 2,
            memory_index: 0,
        }));
        func.instruction(&Instruction::LocalGet(acc_idx));
        // x = lst[i]
        func.instruction(&Instruction::LocalGet(lst_t));
        func.instruction(&Instruction::I32Const(8));
        func.instruction(&Instruction::I32Add);
        func.instruction(&Instruction::LocalGet(i_t));
        if elem_size > 1 {
            func.instruction(&Instruction::I32Const(elem_size as i32));
            func.instruction(&Instruction::I32Mul);
        }
        func.instruction(&Instruction::I32Add);
        self.emit_load_at_offset(&elem, 0, func);
        // func_idx
        func.instruction(&Instruction::LocalGet(fn_t));
        func.instruction(&Instruction::I32Load(MemArg {
            offset: 0,
            align: 2,
            memory_index: 0,
        }));
        func.instruction(&Instruction::CallIndirect {
            type_index: type_idx,
            table_index: 0,
        });
        func.instruction(&Instruction::LocalSet(acc_idx));
        func.instruction(&Instruction::LocalGet(i_t));
        func.instruction(&Instruction::I32Const(1));
        func.instruction(&Instruction::I32Add);
        func.instruction(&Instruction::LocalSet(i_t));
        func.instruction(&Instruction::Br(0));
        func.instruction(&Instruction::End); // loop
        func.instruction(&Instruction::End); // block
        func.instruction(&Instruction::LocalGet(acc_idx));
    }

    /// Tier 6 HoF: `sorted(lst)` (no key for v1) → ascending insertion
    /// sort. Returns a new list with sorted elements.
    ///
    /// Supported element types:
    /// * `I64` — 8B signed integer, `I64Le` compare.
    /// * `F64` — 8B IEEE-754, `F64Le` compare.
    /// * `I32` (and `Bool`, since booleans are stored as i32) — 4B
    ///   signed integer, `I32Le` compare.
    ///
    /// `Ptr` (string and other pointer lists) is not yet supported —
    /// pointer-equality is the only available i32 compare, which would
    /// only sort by allocation order rather than content. Those lists
    /// pass through unchanged. Implementing lexicographic string
    /// compare in WASM is queued.
    fn emit_sorted(&self, args: &[Expr], ctx: &mut FuncContext, func: &mut Function) {
        if args.is_empty() {
            func.instruction(&Instruction::I32Const(0));
            return;
        }
        let lst_ty = self.expr_type(&args[0], ctx);
        let elem = match &lst_ty {
            WasmType::PtrList(t) => (**t).clone(),
            _ => {
                func.instruction(&Instruction::I32Const(0));
                return;
            }
        };
        // Per-type instruction selection. The list-bulk-copy and
        // pointer-arithmetic instructions are i32-typed regardless of
        // element type; only the slot-level access + compare changes.
        // Note: I32 uses align: 2 (4-byte alignment); I64/F64 use
        // align: 3 (8-byte alignment) — these match the slot widths.
        let (key_local_ty, load_instr, store_instr, le_instr) = match elem {
            WasmType::I64 => (
                WasmType::I64,
                Instruction::I64Load(MemArg {
                    offset: 0,
                    align: 3,
                    memory_index: 0,
                }),
                Instruction::I64Store(MemArg {
                    offset: 0,
                    align: 3,
                    memory_index: 0,
                }),
                Instruction::I64LeS,
            ),
            WasmType::F64 => (
                WasmType::F64,
                Instruction::F64Load(MemArg {
                    offset: 0,
                    align: 3,
                    memory_index: 0,
                }),
                Instruction::F64Store(MemArg {
                    offset: 0,
                    align: 3,
                    memory_index: 0,
                }),
                Instruction::F64Le,
            ),
            WasmType::I32 => (
                WasmType::I32,
                Instruction::I32Load(MemArg {
                    offset: 0,
                    align: 2,
                    memory_index: 0,
                }),
                Instruction::I32Store(MemArg {
                    offset: 0,
                    align: 2,
                    memory_index: 0,
                }),
                Instruction::I32LeS,
            ),
            _ => {
                // Ptr / string lists: lexicographic compare not yet
                // implemented. Pass through unchanged.
                self.emit_expr(&args[0], ctx, func);
                return;
            }
        };
        let elem_size = elem.size_bytes();
        let lst_t = ctx.str_temps.get(4).copied().unwrap_or(0);
        let res_t = ctx.str_temps.get(5).copied().unwrap_or(0);
        let i_t = ctx.str_temps.get(6).copied().unwrap_or(0);
        let n_t = ctx.str_temps.get(7).copied().unwrap_or(0);
        let j_t = ctx.str_temps.get(3).copied().unwrap_or(0);

        self.emit_expr(&args[0], ctx, func);
        func.instruction(&Instruction::LocalSet(lst_t));
        // n = length
        func.instruction(&Instruction::LocalGet(lst_t));
        func.instruction(&Instruction::I32Load(MemArg {
            offset: 0,
            align: 2,
            memory_index: 0,
        }));
        func.instruction(&Instruction::LocalSet(n_t));
        // Allocate result and copy contents.
        func.instruction(&Instruction::LocalGet(n_t));
        func.instruction(&Instruction::I32Const(elem_size as i32));
        func.instruction(&Instruction::I32Mul);
        func.instruction(&Instruction::I32Const(8));
        func.instruction(&Instruction::I32Add);
        func.instruction(&Instruction::Call(self.alloc_func_idx));
        func.instruction(&Instruction::LocalSet(res_t));
        // Copy header + bytes.
        func.instruction(&Instruction::LocalGet(res_t));
        func.instruction(&Instruction::LocalGet(n_t));
        func.instruction(&Instruction::I32Store(MemArg {
            offset: 0,
            align: 2,
            memory_index: 0,
        }));
        func.instruction(&Instruction::LocalGet(res_t));
        func.instruction(&Instruction::LocalGet(n_t));
        func.instruction(&Instruction::I32Store(MemArg {
            offset: 4,
            align: 2,
            memory_index: 0,
        }));
        // memory.copy elements
        // dest: res + 8
        func.instruction(&Instruction::LocalGet(res_t));
        func.instruction(&Instruction::I32Const(8));
        func.instruction(&Instruction::I32Add);
        // src: lst + 8
        func.instruction(&Instruction::LocalGet(lst_t));
        func.instruction(&Instruction::I32Const(8));
        func.instruction(&Instruction::I32Add);
        // size: n * elem_size
        func.instruction(&Instruction::LocalGet(n_t));
        func.instruction(&Instruction::I32Const(elem_size as i32));
        func.instruction(&Instruction::I32Mul);
        func.instruction(&Instruction::MemoryCopy {
            src_mem: 0,
            dst_mem: 0,
        });

        // Insertion sort on res[8..].
        // for i in 1..n:
        //   key = res[i]
        //   j = i - 1
        //   while j >= 0 and res[j] > key:
        //     res[j+1] = res[j]
        //     j -= 1
        //   res[j+1] = key
        // We use:
        //   i_t = i (i32), n_t = n, res_t = result ptr
        //   j_t = j (i32)
        //   key in str_saves[0] (we need an i64 — but saves are i32). Use a
        //   global temp via memory? Actually we need an i64 local. The
        //   simplest is to re-load key from res[i] each iteration of the
        //   inner while loop by tracking i (i_t) — once we displace res[i]
        //   the original value is gone. We need a dedicated i64 local.
        // Workaround: the lambda body's str_saves are i32. The function we're
        // emitting sort INTO does have access to all ctx.locals. Let me allocate
        // a fresh i64 local via get_or_alloc_local with a synthetic name.
        // The key local must match the element type so the load → set →
        // compare → store chain typechecks under wasm-validate.
        let (key_name, key_ty_clone) = match key_local_ty {
            WasmType::I64 => ("__sort_key", WasmType::I64),
            WasmType::F64 => ("__sort_key_f64", WasmType::F64),
            WasmType::I32 => ("__sort_key_i32", WasmType::I32),
            _ => ("__sort_key", WasmType::I64),
        };
        let key_idx = ctx.get_or_alloc_local(key_name, key_ty_clone);

        // i = 1
        func.instruction(&Instruction::I32Const(1));
        func.instruction(&Instruction::LocalSet(i_t));
        // outer block + loop
        func.instruction(&Instruction::Block(wasm_encoder::BlockType::Empty));
        func.instruction(&Instruction::Loop(wasm_encoder::BlockType::Empty));
        // if i >= n: break
        func.instruction(&Instruction::LocalGet(i_t));
        func.instruction(&Instruction::LocalGet(n_t));
        func.instruction(&Instruction::I32GeS);
        func.instruction(&Instruction::BrIf(1));
        // key = res[i]
        func.instruction(&Instruction::LocalGet(res_t));
        func.instruction(&Instruction::I32Const(8));
        func.instruction(&Instruction::I32Add);
        func.instruction(&Instruction::LocalGet(i_t));
        func.instruction(&Instruction::I32Const(elem_size as i32));
        func.instruction(&Instruction::I32Mul);
        func.instruction(&Instruction::I32Add);
        func.instruction(&load_instr);
        func.instruction(&Instruction::LocalSet(key_idx));
        // j = i - 1
        func.instruction(&Instruction::LocalGet(i_t));
        func.instruction(&Instruction::I32Const(1));
        func.instruction(&Instruction::I32Sub);
        func.instruction(&Instruction::LocalSet(j_t));
        // inner loop
        func.instruction(&Instruction::Block(wasm_encoder::BlockType::Empty));
        func.instruction(&Instruction::Loop(wasm_encoder::BlockType::Empty));
        // if j < 0: break
        func.instruction(&Instruction::LocalGet(j_t));
        func.instruction(&Instruction::I32Const(0));
        func.instruction(&Instruction::I32LtS);
        func.instruction(&Instruction::BrIf(1));
        // load res[j]
        func.instruction(&Instruction::LocalGet(res_t));
        func.instruction(&Instruction::I32Const(8));
        func.instruction(&Instruction::I32Add);
        func.instruction(&Instruction::LocalGet(j_t));
        func.instruction(&Instruction::I32Const(elem_size as i32));
        func.instruction(&Instruction::I32Mul);
        func.instruction(&Instruction::I32Add);
        func.instruction(&load_instr);
        // compare with key: if res[j] <= key, break
        func.instruction(&Instruction::LocalGet(key_idx));
        func.instruction(&le_instr);
        func.instruction(&Instruction::BrIf(1));
        // res[j+1] = res[j]
        // dest = res + 8 + (j+1) * 8
        func.instruction(&Instruction::LocalGet(res_t));
        func.instruction(&Instruction::I32Const(8));
        func.instruction(&Instruction::I32Add);
        func.instruction(&Instruction::LocalGet(j_t));
        func.instruction(&Instruction::I32Const(1));
        func.instruction(&Instruction::I32Add);
        func.instruction(&Instruction::I32Const(elem_size as i32));
        func.instruction(&Instruction::I32Mul);
        func.instruction(&Instruction::I32Add);
        // value = res[j]
        func.instruction(&Instruction::LocalGet(res_t));
        func.instruction(&Instruction::I32Const(8));
        func.instruction(&Instruction::I32Add);
        func.instruction(&Instruction::LocalGet(j_t));
        func.instruction(&Instruction::I32Const(elem_size as i32));
        func.instruction(&Instruction::I32Mul);
        func.instruction(&Instruction::I32Add);
        func.instruction(&load_instr);
        func.instruction(&store_instr);
        // j -= 1
        func.instruction(&Instruction::LocalGet(j_t));
        func.instruction(&Instruction::I32Const(1));
        func.instruction(&Instruction::I32Sub);
        func.instruction(&Instruction::LocalSet(j_t));
        func.instruction(&Instruction::Br(0));
        func.instruction(&Instruction::End); // inner loop
        func.instruction(&Instruction::End); // inner block
                                             // res[j+1] = key
        func.instruction(&Instruction::LocalGet(res_t));
        func.instruction(&Instruction::I32Const(8));
        func.instruction(&Instruction::I32Add);
        func.instruction(&Instruction::LocalGet(j_t));
        func.instruction(&Instruction::I32Const(1));
        func.instruction(&Instruction::I32Add);
        func.instruction(&Instruction::I32Const(elem_size as i32));
        func.instruction(&Instruction::I32Mul);
        func.instruction(&Instruction::I32Add);
        func.instruction(&Instruction::LocalGet(key_idx));
        func.instruction(&store_instr);
        // i++
        func.instruction(&Instruction::LocalGet(i_t));
        func.instruction(&Instruction::I32Const(1));
        func.instruction(&Instruction::I32Add);
        func.instruction(&Instruction::LocalSet(i_t));
        func.instruction(&Instruction::Br(0));
        func.instruction(&Instruction::End); // outer loop
        func.instruction(&Instruction::End); // outer block
        func.instruction(&Instruction::LocalGet(res_t));
    }

    /// Emit a tuple literal: alloc, store each element by offset, return ptr.
    fn emit_tuple_literal(&self, elts: &[Expr], ctx: &mut FuncContext, func: &mut Function) {
        let elt_types: Vec<WasmType> = elts.iter().map(|e| self.expr_type(e, ctx)).collect();
        let mut offsets: Vec<u32> = Vec::with_capacity(elts.len());
        let mut total: u32 = 0;
        for ty in &elt_types {
            offsets.push(total);
            total += ty.size_bytes();
        }
        if total == 0 {
            func.instruction(&Instruction::I32Const(0));
            return;
        }
        // Allocate
        func.instruction(&Instruction::I32Const(total as i32));
        func.instruction(&Instruction::Call(self.alloc_func_idx));
        let ptr_temp = ctx.str_temps.first().copied().unwrap_or(0);
        func.instruction(&Instruction::LocalSet(ptr_temp));
        // Store each element
        for (i, e) in elts.iter().enumerate() {
            let ty = &elt_types[i];
            let offset = offsets[i];
            func.instruction(&Instruction::LocalGet(ptr_temp));
            self.emit_expr(e, ctx, func);
            match ty {
                WasmType::I64 => {
                    func.instruction(&Instruction::I64Store(MemArg {
                        offset: offset as u64,
                        align: 3,
                        memory_index: 0,
                    }));
                }
                WasmType::F64 => {
                    func.instruction(&Instruction::F64Store(MemArg {
                        offset: offset as u64,
                        align: 3,
                        memory_index: 0,
                    }));
                }
                _ => {
                    func.instruction(&Instruction::I32Store(MemArg {
                        offset: offset as u64,
                        align: 2,
                        memory_index: 0,
                    }));
                }
            }
        }
        // Return ptr
        func.instruction(&Instruction::LocalGet(ptr_temp));
    }

    /// Emit a list literal: alloc `[len][cap][elements...]`, store, return ptr.
    fn emit_list_literal(&self, elts: &[Expr], ctx: &mut FuncContext, func: &mut Function) {
        let n = elts.len();
        let elem_ty = if n == 0 {
            WasmType::I64 // arbitrary; empty list has no element store
        } else {
            self.expr_type(&elts[0], ctx)
        };
        let elem_size = elem_ty.size_bytes();
        let total = 8u32 + (n as u32) * elem_size;
        let ptr_temp = ctx.str_temps.first().copied().unwrap_or(0);

        func.instruction(&Instruction::I32Const(total as i32));
        func.instruction(&Instruction::Call(self.alloc_func_idx));
        func.instruction(&Instruction::LocalSet(ptr_temp));
        // length
        func.instruction(&Instruction::LocalGet(ptr_temp));
        func.instruction(&Instruction::I32Const(n as i32));
        func.instruction(&Instruction::I32Store(MemArg {
            offset: 0,
            align: 2,
            memory_index: 0,
        }));
        // capacity
        func.instruction(&Instruction::LocalGet(ptr_temp));
        func.instruction(&Instruction::I32Const(n as i32));
        func.instruction(&Instruction::I32Store(MemArg {
            offset: 4,
            align: 2,
            memory_index: 0,
        }));
        // elements
        for (i, e) in elts.iter().enumerate() {
            let offset = 8u32 + (i as u32) * elem_size;
            func.instruction(&Instruction::LocalGet(ptr_temp));
            self.emit_expr(e, ctx, func);
            let actual = self.expr_type(e, ctx);
            if actual != elem_ty {
                self.emit_convert(&actual, &elem_ty, func);
            }
            match elem_ty {
                WasmType::I64 => {
                    func.instruction(&Instruction::I64Store(MemArg {
                        offset: offset as u64,
                        align: 3,
                        memory_index: 0,
                    }));
                }
                WasmType::F64 => {
                    func.instruction(&Instruction::F64Store(MemArg {
                        offset: offset as u64,
                        align: 3,
                        memory_index: 0,
                    }));
                }
                _ => {
                    func.instruction(&Instruction::I32Store(MemArg {
                        offset: offset as u64,
                        align: 2,
                        memory_index: 0,
                    }));
                }
            }
        }
        func.instruction(&Instruction::LocalGet(ptr_temp));
    }

    /// Emit `list_expr * count_expr` where list_expr is a PtrList and count_expr
    /// is an integer (I64 or I32). Allocates a new list with `new_len = src_len * n`
    /// elements, copies the source `n` times via `memory.copy`, and leaves the new
    /// list pointer on the WASM value stack.
    ///
    /// Memory layout (matches emit_list_literal exactly):
    ///   [i32 length][i32 capacity][elem0][elem1]...
    ///   Header = 8 bytes; element size from WasmType::size_bytes().
    fn emit_list_repeat(
        &self,
        list_expr: &Expr,
        count_expr: &Expr,
        list_ty: &WasmType,
        ctx: &mut FuncContext,
        func: &mut Function,
    ) {
        let elem_ty = match list_ty {
            WasmType::PtrList(inner) => (**inner).clone(),
            _ => return,
        };
        let elem_size = elem_ty.size_bytes() as i32;

        // Save/restore str_depth so nested string operations stay correct.
        let save_depth = ctx.str_depth;
        ctx.str_depth = save_depth + 1;

        // ── Step 1: evaluate list_expr ── save to str_saves[save_depth]
        // Using a save slot protects the pointer if count_expr evaluation
        // re-uses str_temps internally (e.g. list subscript uses str_temps[0]).
        self.emit_expr(list_expr, ctx, func);
        func.instruction(&Instruction::LocalSet(ctx.str_saves[save_depth]));

        // ── Step 2: evaluate count_expr ── convert to i32, save to str_temps[1]
        let ct = self.expr_type(count_expr, ctx);
        self.emit_expr(count_expr, ctx, func);
        if ct == WasmType::I64 {
            func.instruction(&Instruction::I32WrapI64);
        }
        func.instruction(&Instruction::LocalSet(ctx.str_temps[1])); // n (raw)

        ctx.str_depth = save_depth;

        // Move src_ptr from save slot into str_temps[0] for the computation.
        func.instruction(&Instruction::LocalGet(ctx.str_saves[save_depth]));
        func.instruction(&Instruction::LocalSet(ctx.str_temps[0])); // src_ptr

        // Copy indices into plain locals — no mutable borrow of ctx after this.
        let t0 = ctx.str_temps[0]; // src_ptr
        let t1 = ctx.str_temps[1]; // n
        let t2 = ctx.str_temps[2]; // src_len
        let t3 = ctx.str_temps[3]; // new_len  (src_len * n)
        let t4 = ctx.str_temps[4]; // dst_ptr
        let t5 = ctx.str_temps[5]; // copy_byte_count  (src_len * elem_size)
        let t6 = ctx.str_temps[6]; // loop counter i

        // ── Step 3: clamp n = max(0, n) ──
        // WASM select(val_true, val_false, cond): pops cond then val_false then val_true.
        func.instruction(&Instruction::LocalGet(t1)); // val_true  = n
        func.instruction(&Instruction::I32Const(0)); // val_false = 0
        func.instruction(&Instruction::LocalGet(t1)); // n (for comparison)
        func.instruction(&Instruction::I32Const(0));
        func.instruction(&Instruction::I32GtS); // cond = n > 0
        func.instruction(&Instruction::Select); // result = n > 0 ? n : 0
        func.instruction(&Instruction::LocalSet(t1)); // n = max(0, n)

        // ── Step 4: load src_len from list header ──
        func.instruction(&Instruction::LocalGet(t0));
        func.instruction(&Instruction::I32Load(MemArg {
            offset: 0,
            align: 2,
            memory_index: 0,
        }));
        func.instruction(&Instruction::LocalSet(t2)); // src_len

        // ── Step 5: new_len = src_len * n ──
        func.instruction(&Instruction::LocalGet(t2));
        func.instruction(&Instruction::LocalGet(t1));
        func.instruction(&Instruction::I32Mul);
        func.instruction(&Instruction::LocalSet(t3)); // new_len

        // ── Step 6: copy_byte_count = src_len * elem_size ──
        func.instruction(&Instruction::LocalGet(t2));
        func.instruction(&Instruction::I32Const(elem_size));
        func.instruction(&Instruction::I32Mul);
        func.instruction(&Instruction::LocalSet(t5)); // copy_byte_count

        // ── Step 7: allocate  new_len * elem_size + 8  bytes ──
        func.instruction(&Instruction::LocalGet(t3));
        func.instruction(&Instruction::I32Const(elem_size));
        func.instruction(&Instruction::I32Mul);
        func.instruction(&Instruction::I32Const(8));
        func.instruction(&Instruction::I32Add);
        func.instruction(&Instruction::Call(self.alloc_func_idx));
        func.instruction(&Instruction::LocalSet(t4)); // dst_ptr

        // ── Step 8: write length and capacity headers ──
        func.instruction(&Instruction::LocalGet(t4));
        func.instruction(&Instruction::LocalGet(t3));
        func.instruction(&Instruction::I32Store(MemArg {
            offset: 0,
            align: 2,
            memory_index: 0,
        }));

        func.instruction(&Instruction::LocalGet(t4));
        func.instruction(&Instruction::LocalGet(t3));
        func.instruction(&Instruction::I32Store(MemArg {
            offset: 4,
            align: 2,
            memory_index: 0,
        }));

        // ── Step 9: copy loop — repeat src_len elements n times ──
        // i = 0
        func.instruction(&Instruction::I32Const(0));
        func.instruction(&Instruction::LocalSet(t6));

        // block $break
        func.instruction(&Instruction::Block(wasm_encoder::BlockType::Empty));
        // loop $continue
        func.instruction(&Instruction::Loop(wasm_encoder::BlockType::Empty));

        // if i >= n: br $break (depth 1)
        func.instruction(&Instruction::LocalGet(t6));
        func.instruction(&Instruction::LocalGet(t1));
        func.instruction(&Instruction::I32GeS);
        func.instruction(&Instruction::BrIf(1));

        // memory.copy(dst_ptr + 8 + i * copy_byte_count, src_ptr + 8, copy_byte_count)
        // WASM memory.copy operand order on stack: dst, src, size
        func.instruction(&Instruction::LocalGet(t4)); // dst_ptr
        func.instruction(&Instruction::I32Const(8));
        func.instruction(&Instruction::I32Add);
        func.instruction(&Instruction::LocalGet(t6)); // i
        func.instruction(&Instruction::LocalGet(t5)); // copy_byte_count
        func.instruction(&Instruction::I32Mul);
        func.instruction(&Instruction::I32Add); // dst_ptr + 8 + i*cbc

        func.instruction(&Instruction::LocalGet(t0)); // src_ptr
        func.instruction(&Instruction::I32Const(8));
        func.instruction(&Instruction::I32Add); // src_ptr + 8

        func.instruction(&Instruction::LocalGet(t5)); // copy_byte_count

        func.instruction(&Instruction::MemoryCopy {
            src_mem: 0,
            dst_mem: 0,
        });

        // i++
        func.instruction(&Instruction::LocalGet(t6));
        func.instruction(&Instruction::I32Const(1));
        func.instruction(&Instruction::I32Add);
        func.instruction(&Instruction::LocalSet(t6));

        func.instruction(&Instruction::Br(0)); // br $continue
        func.instruction(&Instruction::End); // end loop
        func.instruction(&Instruction::End); // end block

        // ── Result: dst_ptr on stack ──
        func.instruction(&Instruction::LocalGet(t4));
    }

    fn emit_binop(
        &self,
        left: &Expr,
        op: BinOp,
        right: &Expr,
        ctx: &mut FuncContext,
        func: &mut Function,
    ) {
        let lt = self.expr_type(left, ctx);
        let rt = self.expr_type(right, ctx);

        // String concatenation: Ptr + Ptr
        if op == BinOp::Add && lt == WasmType::Ptr && rt == WasmType::Ptr {
            self.emit_string_concat(left, right, ctx, func);
            return;
        }

        // List repetition: PtrList * Int  or  Int * PtrList  (B-033)
        // Must be checked before the generic Mul fallthrough which would emit
        // a wrong I32Mul on two i32-shaped values.
        if op == BinOp::Mul {
            if matches!(lt, WasmType::PtrList(_)) && (rt == WasmType::I64 || rt == WasmType::I32) {
                self.emit_list_repeat(left, right, &lt, ctx, func);
                return;
            }
            if matches!(rt, WasmType::PtrList(_)) && (lt == WasmType::I64 || lt == WasmType::I32) {
                self.emit_list_repeat(right, left, &rt, ctx, func);
                return;
            }
        }

        match op {
            BinOp::Div => {
                // Python true division always returns float
                self.emit_expr(left, ctx, func);
                if lt != WasmType::F64 {
                    self.emit_convert(&lt, &WasmType::F64, func);
                }
                self.emit_expr(right, ctx, func);
                if rt != WasmType::F64 {
                    self.emit_convert(&rt, &WasmType::F64, func);
                }
                func.instruction(&Instruction::F64Div);
            }

            BinOp::Pow => {
                if lt != WasmType::F64 && rt != WasmType::F64 {
                    // int ** int stays exact in i64 (#358): square-and-multiply
                    // over checked muls. Overflow / negative exponents flag
                    // `__ovf` and the glue re-runs on the exact JS twin.
                    self.emit_expr(left, ctx, func);
                    if lt != WasmType::I64 {
                        self.emit_convert(&lt, &WasmType::I64, func);
                    }
                    self.emit_expr(right, ctx, func);
                    if rt != WasmType::I64 {
                        self.emit_convert(&rt, &WasmType::I64, func);
                    }
                    self.emit_num_op_on_stack(BinOp::Pow, &WasmType::I64, ctx, func);
                } else {
                    // Use imported math.pow (f64)
                    self.emit_expr(left, ctx, func);
                    if lt != WasmType::F64 {
                        self.emit_convert(&lt, &WasmType::F64, func);
                    }
                    self.emit_expr(right, ctx, func);
                    if rt != WasmType::F64 {
                        self.emit_convert(&rt, &WasmType::F64, func);
                    }
                    let idx = *self
                        .import_indices
                        .get("pow")
                        .expect("pow import not registered");
                    func.instruction(&Instruction::Call(idx));
                }
            }

            BinOp::FloorDiv => {
                if lt == WasmType::F64 || rt == WasmType::F64 {
                    // float floor div: floor(a / b)
                    self.emit_expr(left, ctx, func);
                    if lt != WasmType::F64 {
                        self.emit_convert(&lt, &WasmType::F64, func);
                    }
                    self.emit_expr(right, ctx, func);
                    if rt != WasmType::F64 {
                        self.emit_convert(&rt, &WasmType::F64, func);
                    }
                    func.instruction(&Instruction::F64Div);
                    func.instruction(&Instruction::F64Floor);
                } else {
                    // int floor div: exact i64 with Python floor semantics
                    // (#358 — the old f64 round-trip silently lost precision
                    // past 2^53).
                    self.emit_expr(left, ctx, func);
                    if lt != WasmType::I64 {
                        self.emit_convert(&lt, &WasmType::I64, func);
                    }
                    self.emit_expr(right, ctx, func);
                    if rt != WasmType::I64 {
                        self.emit_convert(&rt, &WasmType::I64, func);
                    }
                    self.emit_num_op_on_stack(BinOp::FloorDiv, &WasmType::I64, ctx, func);
                }
            }

            BinOp::Mod => {
                if lt == WasmType::F64 || rt == WasmType::F64 {
                    // float mod: a - floor(a/b) * b (on-stack via scratch
                    // locals -- no operand re-emission / double evaluation)
                    self.emit_expr(left, ctx, func);
                    if lt != WasmType::F64 {
                        self.emit_convert(&lt, &WasmType::F64, func);
                    }
                    self.emit_expr(right, ctx, func);
                    if rt != WasmType::F64 {
                        self.emit_convert(&rt, &WasmType::F64, func);
                    }
                    self.emit_num_op_on_stack(BinOp::Mod, &WasmType::F64, ctx, func);
                } else {
                    // Integer mod, Python semantics, exact (#358 -- the old
                    // `((a % b) + b) % b` could wrap in the intermediate add).
                    self.emit_expr(left, ctx, func);
                    if lt == WasmType::I32 {
                        self.emit_convert(&lt, &WasmType::I64, func);
                    }
                    self.emit_expr(right, ctx, func);
                    if rt == WasmType::I32 {
                        self.emit_convert(&rt, &WasmType::I64, func);
                    }
                    self.emit_num_op_on_stack(BinOp::Mod, &WasmType::I64, ctx, func);
                }
            }

            BinOp::Eq | BinOp::NotEq | BinOp::Lt | BinOp::LtEq | BinOp::Gt | BinOp::GtEq => {
                self.emit_comparison_op(left, op, right, ctx, func);
            }

            BinOp::And => {
                // Logical and: emit as (a != 0) && (b != 0) â†’ i32
                self.emit_condition(left, ctx, func);
                self.emit_condition(right, ctx, func);
                func.instruction(&Instruction::I32And);
            }

            BinOp::Or => {
                // Logical or: (a != 0) || (b != 0) â†’ i32
                self.emit_condition(left, ctx, func);
                self.emit_condition(right, ctx, func);
                func.instruction(&Instruction::I32Or);
            }

            // Standard arithmetic: Add, Sub, Mul
            _ => {
                let op_type = self.arithmetic_op_type(&lt, &rt);
                self.emit_expr(left, ctx, func);
                if lt != op_type {
                    self.emit_convert(&lt, &op_type, func);
                }
                self.emit_expr(right, ctx, func);
                if rt != op_type {
                    self.emit_convert(&rt, &op_type, func);
                }
                self.emit_num_op_on_stack(op, &op_type, ctx, func);
            }
        }
    }

    fn emit_comparison_op(
        &self,
        left: &Expr,
        op: BinOp,
        right: &Expr,
        ctx: &mut FuncContext,
        func: &mut Function,
    ) {
        let lt = self.expr_type(left, ctx);
        let rt = self.expr_type(right, ctx);

        // String equality: Ptr == Ptr or Ptr != Ptr
        if lt == WasmType::Ptr && rt == WasmType::Ptr && (op == BinOp::Eq || op == BinOp::NotEq) {
            self.emit_string_eq(left, right, ctx, func);
            if op == BinOp::NotEq {
                func.instruction(&Instruction::I32Eqz);
            }
            return;
        }

        let cmp_type = self.arithmetic_op_type(&lt, &rt);

        self.emit_expr(left, ctx, func);
        if lt != cmp_type {
            self.emit_convert(&lt, &cmp_type, func);
        }
        self.emit_expr(right, ctx, func);
        if rt != cmp_type {
            self.emit_convert(&rt, &cmp_type, func);
        }

        match cmp_type {
            WasmType::I64 => match op {
                BinOp::Eq => {
                    func.instruction(&Instruction::I64Eq);
                }
                BinOp::NotEq => {
                    func.instruction(&Instruction::I64Ne);
                }
                BinOp::Lt => {
                    func.instruction(&Instruction::I64LtS);
                }
                BinOp::LtEq => {
                    func.instruction(&Instruction::I64LeS);
                }
                BinOp::Gt => {
                    func.instruction(&Instruction::I64GtS);
                }
                BinOp::GtEq => {
                    func.instruction(&Instruction::I64GeS);
                }
                _ => {}
            },
            WasmType::F64 => match op {
                BinOp::Eq => {
                    func.instruction(&Instruction::F64Eq);
                }
                BinOp::NotEq => {
                    func.instruction(&Instruction::F64Ne);
                }
                BinOp::Lt => {
                    func.instruction(&Instruction::F64Lt);
                }
                BinOp::LtEq => {
                    func.instruction(&Instruction::F64Le);
                }
                BinOp::Gt => {
                    func.instruction(&Instruction::F64Gt);
                }
                BinOp::GtEq => {
                    func.instruction(&Instruction::F64Ge);
                }
                _ => {}
            },
            // I32 (bool), Ptr (string), or any collection/closure ptr — all i32 at the
            // WASM level, so use signed i32 comparisons.
            _ => match op {
                BinOp::Eq => {
                    func.instruction(&Instruction::I32Eq);
                }
                BinOp::NotEq => {
                    func.instruction(&Instruction::I32Ne);
                }
                BinOp::Lt => {
                    func.instruction(&Instruction::I32LtS);
                }
                BinOp::LtEq => {
                    func.instruction(&Instruction::I32LeS);
                }
                BinOp::Gt => {
                    func.instruction(&Instruction::I32GtS);
                }
                BinOp::GtEq => {
                    func.instruction(&Instruction::I32GeS);
                }
                _ => {}
            },
        }
    }

    fn emit_unaryop(
        &self,
        op: UnaryOp,
        operand: &Expr,
        ctx: &mut FuncContext,
        func: &mut Function,
    ) {
        let ot = self.expr_type(operand, ctx);

        match op {
            UnaryOp::Neg => {
                match ot {
                    WasmType::I64 => {
                        // Constant-fold negated literals (also the only way
                        // to represent -2^63 = i64::MIN exactly — the
                        // positive literal 2^63 itself is out of range).
                        if let ExprKind::IntLiteral(n) = &operand.kind {
                            func.instruction(&Instruction::I64Const((-*n) as i64));
                            return;
                        }
                        // #358: -i64::MIN overflows; flag it for the exact
                        // JS twin.
                        let a = ctx.ck_i64[0];
                        self.emit_expr(operand, ctx, func);
                        func.instruction(&Instruction::LocalTee(a));
                        func.instruction(&Instruction::I64Const(i64::MIN));
                        func.instruction(&Instruction::I64Eq);
                        func.instruction(&Instruction::If(wasm_encoder::BlockType::Empty));
                        self.emit_set_ovf(func);
                        func.instruction(&Instruction::End);
                        func.instruction(&Instruction::I64Const(0));
                        func.instruction(&Instruction::LocalGet(a));
                        func.instruction(&Instruction::I64Sub);
                    }
                    WasmType::F64 => {
                        self.emit_expr(operand, ctx, func);
                        func.instruction(&Instruction::F64Neg);
                    }
                    // Any i32-shaped value (bool, ptr, list, tuple, dict, closure)
                    _ => {
                        func.instruction(&Instruction::I32Const(0));
                        self.emit_expr(operand, ctx, func);
                        func.instruction(&Instruction::I32Sub);
                    }
                }
            }
            UnaryOp::Pos => {
                // Positive is identity
                self.emit_expr(operand, ctx, func);
            }
            UnaryOp::Not => match ot {
                WasmType::I64 => {
                    self.emit_expr(operand, ctx, func);
                    func.instruction(&Instruction::I64Eqz);
                }
                WasmType::F64 => {
                    self.emit_expr(operand, ctx, func);
                    func.instruction(&Instruction::F64Const(0.0));
                    func.instruction(&Instruction::F64Eq);
                }
                _ => {
                    self.emit_expr(operand, ctx, func);
                    func.instruction(&Instruction::I32Eqz);
                }
            },
            UnaryOp::BitNot => {
                match ot {
                    WasmType::I64 => {
                        self.emit_expr(operand, ctx, func);
                        func.instruction(&Instruction::I64Const(-1));
                        func.instruction(&Instruction::I64Xor);
                    }
                    WasmType::F64 => {
                        // BitNot on float doesn't make sense, but handle it
                        self.emit_expr(operand, ctx, func);
                    }
                    _ => {
                        self.emit_expr(operand, ctx, func);
                        func.instruction(&Instruction::I32Const(-1));
                        func.instruction(&Instruction::I32Xor);
                    }
                }
            }
        }
    }

    fn emit_compare(
        &self,
        left: &Expr,
        comparisons: &[(BinOp, Expr)],
        ctx: &mut FuncContext,
        func: &mut Function,
    ) {
        if comparisons.len() == 1 {
            let (op, right) = &comparisons[0];
            self.emit_comparison_op(left, *op, right, ctx, func);
        } else {
            // Chained comparison: a < b < c â†’ (a < b) && (b < c)
            // For simplicity, emit each pair and AND them together
            let (first_op, first_right) = &comparisons[0];
            self.emit_comparison_op(left, *first_op, first_right, ctx, func);

            let mut prev_right = first_right;
            for (op, right) in &comparisons[1..] {
                self.emit_comparison_op(prev_right, *op, right, ctx, func);
                func.instruction(&Instruction::I32And);
                prev_right = right;
            }
        }
    }

    fn emit_call(&self, callee: &Expr, args: &[Expr], ctx: &mut FuncContext, func: &mut Function) {
        if let ExprKind::Name(name) = &callee.kind {
            match name.as_str() {
                // Livermore finding (2026-07-10): int()/float() numeric
                // conversions. int(f64) truncates toward zero (Python
                // semantics) via saturating trunc; float(i64) converts.
                // Already-target-typed args are a no-op.
                "int" => {
                    assert!(!args.is_empty());
                    let arg = &args[0];
                    let at = self.expr_type(arg, ctx);
                    self.emit_expr(arg, ctx, func);
                    match at {
                        WasmType::F64 => {
                            func.instruction(&Instruction::I64TruncSatF64S);
                        }
                        WasmType::I32 => {
                            func.instruction(&Instruction::I64ExtendI32S);
                        }
                        _ => {}
                    }
                }
                "float" => {
                    assert!(!args.is_empty());
                    let arg = &args[0];
                    let at = self.expr_type(arg, ctx);
                    self.emit_expr(arg, ctx, func);
                    match at {
                        WasmType::I64 => {
                            func.instruction(&Instruction::F64ConvertI64S);
                        }
                        WasmType::I32 => {
                            func.instruction(&Instruction::F64ConvertI32S);
                        }
                        _ => {}
                    }
                }
                "abs" => {
                    assert!(!args.is_empty());
                    let arg = &args[0];
                    let at = self.expr_type(arg, ctx);
                    self.emit_expr(arg, ctx, func);
                    match at {
                        WasmType::F64 => {
                            func.instruction(&Instruction::F64Abs);
                        }
                        WasmType::I64 => {
                            // abs for i64 via a scratch local (single eval).
                            // #358: abs(i64::MIN) is not representable — flag
                            // it for the exact JS twin.
                            let a = ctx.ck_i64[0];
                            func.instruction(&Instruction::LocalTee(a));
                            func.instruction(&Instruction::I64Const(i64::MIN));
                            func.instruction(&Instruction::I64Eq);
                            func.instruction(&Instruction::If(wasm_encoder::BlockType::Empty));
                            self.emit_set_ovf(func);
                            func.instruction(&Instruction::End);
                            // select(x, -x, x >= 0): stack val1=x, val2=-x, c
                            func.instruction(&Instruction::LocalGet(a));
                            func.instruction(&Instruction::I64Const(0));
                            func.instruction(&Instruction::LocalGet(a));
                            func.instruction(&Instruction::I64Sub);
                            func.instruction(&Instruction::LocalGet(a));
                            func.instruction(&Instruction::I64Const(0));
                            func.instruction(&Instruction::I64GeS);
                            func.instruction(&Instruction::Select);
                        }
                        // Any i32-shaped (bool, ptr, list, etc.) — fall through
                        _ => {
                            self.emit_expr(arg, ctx, func);
                            func.instruction(&Instruction::I32Const(0));
                            func.instruction(&Instruction::I32Sub);
                            self.emit_expr(arg, ctx, func);
                            func.instruction(&Instruction::I32Const(0));
                            func.instruction(&Instruction::I32GeS);
                            func.instruction(&Instruction::Select);
                        }
                    }
                }
                "min" => {
                    assert!(args.len() == 2);
                    let a_type = self.expr_type(&args[0], ctx);
                    let b_type = self.expr_type(&args[1], ctx);
                    if a_type == WasmType::F64 || b_type == WasmType::F64 {
                        self.emit_expr(&args[0], ctx, func);
                        if a_type != WasmType::F64 {
                            self.emit_convert(&a_type, &WasmType::F64, func);
                        }
                        self.emit_expr(&args[1], ctx, func);
                        if b_type != WasmType::F64 {
                            self.emit_convert(&b_type, &WasmType::F64, func);
                        }
                        func.instruction(&Instruction::F64Min);
                    } else {
                        // For integers, use select: a if a <= b else b
                        self.emit_expr(&args[0], ctx, func);
                        if a_type == WasmType::I32 {
                            self.emit_convert(&a_type, &WasmType::I64, func);
                        }
                        self.emit_expr(&args[1], ctx, func);
                        if b_type == WasmType::I32 {
                            self.emit_convert(&b_type, &WasmType::I64, func);
                        }
                        // Stack: a, b
                        // Need: a, b, (a <= b)
                        self.emit_expr(&args[0], ctx, func);
                        if a_type == WasmType::I32 {
                            self.emit_convert(&a_type, &WasmType::I64, func);
                        }
                        self.emit_expr(&args[1], ctx, func);
                        if b_type == WasmType::I32 {
                            self.emit_convert(&b_type, &WasmType::I64, func);
                        }
                        func.instruction(&Instruction::I64LeS);
                        func.instruction(&Instruction::Select);
                    }
                }
                "max" => {
                    assert!(args.len() == 2);
                    let a_type = self.expr_type(&args[0], ctx);
                    let b_type = self.expr_type(&args[1], ctx);
                    if a_type == WasmType::F64 || b_type == WasmType::F64 {
                        self.emit_expr(&args[0], ctx, func);
                        if a_type != WasmType::F64 {
                            self.emit_convert(&a_type, &WasmType::F64, func);
                        }
                        self.emit_expr(&args[1], ctx, func);
                        if b_type != WasmType::F64 {
                            self.emit_convert(&b_type, &WasmType::F64, func);
                        }
                        func.instruction(&Instruction::F64Max);
                    } else {
                        self.emit_expr(&args[0], ctx, func);
                        if a_type == WasmType::I32 {
                            self.emit_convert(&a_type, &WasmType::I64, func);
                        }
                        self.emit_expr(&args[1], ctx, func);
                        if b_type == WasmType::I32 {
                            self.emit_convert(&b_type, &WasmType::I64, func);
                        }
                        self.emit_expr(&args[0], ctx, func);
                        if a_type == WasmType::I32 {
                            self.emit_convert(&a_type, &WasmType::I64, func);
                        }
                        self.emit_expr(&args[1], ctx, func);
                        if b_type == WasmType::I32 {
                            self.emit_convert(&b_type, &WasmType::I64, func);
                        }
                        func.instruction(&Instruction::I64GeS);
                        func.instruction(&Instruction::Select);
                    }
                }
                "len" => {
                    if !args.is_empty() {
                        let arg = &args[0];
                        let at = self.expr_type(arg, ctx);
                        match &at {
                            WasmType::Ptr => {
                                // String len: load 4-byte length prefix
                                self.emit_expr(arg, ctx, func);
                                func.instruction(&Instruction::I32Load(MemArg {
                                    offset: 0,
                                    align: 2,
                                    memory_index: 0,
                                }));
                                func.instruction(&Instruction::I64ExtendI32S);
                            }
                            WasmType::PtrList(_) => {
                                // List len: i32.load at ptr (length prefix)
                                self.emit_expr(arg, ctx, func);
                                func.instruction(&Instruction::I32Load(MemArg {
                                    offset: 0,
                                    align: 2,
                                    memory_index: 0,
                                }));
                                func.instruction(&Instruction::I64ExtendI32S);
                            }
                            WasmType::PtrTuple(elt_types) => {
                                // Tuple len: compile-time constant.
                                func.instruction(&Instruction::I64Const(elt_types.len() as i64));
                            }
                            WasmType::PtrDict(_, _) => {
                                // Dict len via bridge import (Step 4 wiring).
                                self.emit_expr(arg, ctx, func);
                                if let Some(&idx) = self.import_indices.get("__dict_len") {
                                    func.instruction(&Instruction::Call(idx));
                                    func.instruction(&Instruction::I64ExtendI32S);
                                } else {
                                    func.instruction(&Instruction::Drop);
                                    func.instruction(&Instruction::I64Const(0));
                                }
                            }
                            _ => {
                                self.emit_expr(arg, ctx, func);
                                func.instruction(&Instruction::Drop);
                                func.instruction(&Instruction::I64Const(0));
                            }
                        }
                    }
                }
                "range" => {
                    // range() shouldn't be called as an expression in WASM context
                    // (it's only valid in for loops, which are handled separately)
                    func.instruction(&Instruction::I64Const(0));
                }
                // Tier 6 HoF builtins. Each iterates a list and calls a
                // closure via `call_indirect`.
                "map" => self.emit_map(args, ctx, func),
                "filter" => self.emit_filter(args, ctx, func),
                "reduce" => self.emit_reduce(args, ctx, func),
                "sorted" => self.emit_sorted(args, ctx, func),
                _ => {
                    // Bare math call: `sqrt(x)` bound via `from math import sqrt`.
                    // Dispatch identically to the `math.sqrt(x)` attribute form —
                    // coerce each arg to f64 and call the registered import. A
                    // user function never shadows a math alias here (the alias map
                    // is only populated when the name was imported from math).
                    if let Some(canonical) = self.math_aliases.get(name).cloned() {
                        if let Some(arity) = math_function_arity(&canonical) {
                            for i in 0..arity as usize {
                                if i < args.len() {
                                    let at = self.expr_type(&args[i], ctx);
                                    self.emit_expr(&args[i], ctx, func);
                                    if at != WasmType::F64 {
                                        self.emit_convert(&at, &WasmType::F64, func);
                                    }
                                }
                            }
                            let idx = *self
                                .import_indices
                                .get(canonical.as_str())
                                .expect("math import index not registered");
                            func.instruction(&Instruction::Call(idx));
                            return;
                        }
                    }
                    // Tier 6: if `name` is a local of closure type, indirect-call.
                    // Closure value is an i32 pointer to `[i32 func_idx][i32 env_ptr]`.
                    // Calling convention: push env_ptr, args..., func_idx; call_indirect.
                    if let Some((local_idx, WasmType::PtrClosure { params, ret })) =
                        ctx.get_local(name)
                    {
                        let sig = ClosureSig {
                            params: params.clone(),
                            ret: ret.as_ref().map(|b| (**b).clone()),
                        };
                        if let Some(&type_idx) = self.closure_type_indices.get(&sig) {
                            // Push env_ptr (closure[4]).
                            func.instruction(&Instruction::LocalGet(local_idx));
                            func.instruction(&Instruction::I32Load(MemArg {
                                offset: 4,
                                align: 2,
                                memory_index: 0,
                            }));
                            // Push args.
                            for arg in args {
                                self.emit_expr(arg, ctx, func);
                            }
                            // Push func_idx (closure[0]).
                            func.instruction(&Instruction::LocalGet(local_idx));
                            func.instruction(&Instruction::I32Load(MemArg {
                                offset: 0,
                                align: 2,
                                memory_index: 0,
                            }));
                            func.instruction(&Instruction::CallIndirect {
                                type_index: type_idx,
                                table_index: 0,
                            });
                            return;
                        }
                    }
                    // Call to user function (regular direct call).
                    if let Some(&idx) = self.func_indices.get(name.as_str()) {
                        for arg in args {
                            self.emit_expr(arg, ctx, func);
                        }
                        func.instruction(&Instruction::Call(idx));
                    }
                }
            }
        } else if let ExprKind::Attribute { value, attr, .. } = &callee.kind {
            // Handle math.X(...) calls
            if let ExprKind::Name(mod_name) = &value.kind {
                if mod_name == "math" {
                    if let Some(arity) = math_function_arity(attr.as_str()) {
                        // Push args (all coerced to f64)
                        for i in 0..arity as usize {
                            if i < args.len() {
                                let at = self.expr_type(&args[i], ctx);
                                self.emit_expr(&args[i], ctx, func);
                                if at != WasmType::F64 {
                                    self.emit_convert(&at, &WasmType::F64, func);
                                }
                            }
                        }
                        let idx = *self
                            .import_indices
                            .get(attr.as_str())
                            .expect("math import index not registered");
                        func.instruction(&Instruction::Call(idx));
                        return;
                    }
                }
            }

            let val_type = self.expr_type(value, ctx);
            match &val_type {
                WasmType::Ptr => match attr.as_str() {
                    "upper" => self.emit_string_case(value, true, ctx, func),
                    "lower" => self.emit_string_case(value, false, ctx, func),
                    "startswith" => {
                        if !args.is_empty() {
                            self.emit_string_startswith(value, &args[0], ctx, func);
                        }
                    }
                    "endswith" => {
                        if !args.is_empty() {
                            self.emit_string_endswith(value, &args[0], ctx, func);
                        }
                    }
                    "find" => {
                        if !args.is_empty() {
                            self.emit_string_find(value, &args[0], ctx, func);
                        }
                    }
                    _ => {
                        func.instruction(&Instruction::I32Const(0));
                    }
                },
                // Tier 2: list methods.
                WasmType::PtrList(elem_ty) => {
                    let elem_size = elem_ty.size_bytes();
                    let elem = (**elem_ty).clone();
                    match attr.as_str() {
                        "append" => {
                            if args.is_empty() {
                                func.instruction(&Instruction::I32Const(0));
                                return;
                            }
                            // append(x): assumes capacity > length (no resize for v1).
                            //   addr = ptr + 8 + length * elem_size
                            //   *addr = x
                            //   length += 1
                            // We need ptr in a temp.
                            let ptr_temp = ctx.str_temps.first().copied().unwrap_or(0);
                            self.emit_expr(value, ctx, func);
                            func.instruction(&Instruction::LocalSet(ptr_temp));
                            // Compute address: ptr + 8 + length * elem_size
                            func.instruction(&Instruction::LocalGet(ptr_temp));
                            func.instruction(&Instruction::I32Const(8));
                            func.instruction(&Instruction::I32Add);
                            func.instruction(&Instruction::LocalGet(ptr_temp));
                            func.instruction(&Instruction::I32Load(MemArg {
                                offset: 0,
                                align: 2,
                                memory_index: 0,
                            }));
                            if elem_size > 1 {
                                func.instruction(&Instruction::I32Const(elem_size as i32));
                                func.instruction(&Instruction::I32Mul);
                            }
                            func.instruction(&Instruction::I32Add);
                            // Push value
                            self.emit_expr(&args[0], ctx, func);
                            let val_ty = self.expr_type(&args[0], ctx);
                            if val_ty != elem {
                                self.emit_convert(&val_ty, &elem, func);
                            }
                            // Store
                            match &elem {
                                WasmType::I64 => {
                                    func.instruction(&Instruction::I64Store(MemArg {
                                        offset: 0,
                                        align: 3,
                                        memory_index: 0,
                                    }));
                                }
                                WasmType::F64 => {
                                    func.instruction(&Instruction::F64Store(MemArg {
                                        offset: 0,
                                        align: 3,
                                        memory_index: 0,
                                    }));
                                }
                                _ => {
                                    func.instruction(&Instruction::I32Store(MemArg {
                                        offset: 0,
                                        align: 2,
                                        memory_index: 0,
                                    }));
                                }
                            }
                            // length += 1
                            func.instruction(&Instruction::LocalGet(ptr_temp));
                            func.instruction(&Instruction::LocalGet(ptr_temp));
                            func.instruction(&Instruction::I32Load(MemArg {
                                offset: 0,
                                align: 2,
                                memory_index: 0,
                            }));
                            func.instruction(&Instruction::I32Const(1));
                            func.instruction(&Instruction::I32Add);
                            func.instruction(&Instruction::I32Store(MemArg {
                                offset: 0,
                                align: 2,
                                memory_index: 0,
                            }));
                            // append returns None — push a sentinel (0) for stack
                            // balance even though Python's `lst.append(x)` is
                            // typically a stmt expression.
                            func.instruction(&Instruction::I32Const(0));
                        }
                        "pop" => {
                            // pop(): decrement length, return element at new end
                            let ptr_temp = ctx.str_temps.first().copied().unwrap_or(0);
                            self.emit_expr(value, ctx, func);
                            func.instruction(&Instruction::LocalSet(ptr_temp));
                            // new_len = length - 1
                            // Store new length back
                            func.instruction(&Instruction::LocalGet(ptr_temp));
                            func.instruction(&Instruction::LocalGet(ptr_temp));
                            func.instruction(&Instruction::I32Load(MemArg {
                                offset: 0,
                                align: 2,
                                memory_index: 0,
                            }));
                            func.instruction(&Instruction::I32Const(1));
                            func.instruction(&Instruction::I32Sub);
                            func.instruction(&Instruction::I32Store(MemArg {
                                offset: 0,
                                align: 2,
                                memory_index: 0,
                            }));
                            // Read element at new_len position
                            func.instruction(&Instruction::LocalGet(ptr_temp));
                            func.instruction(&Instruction::I32Const(8));
                            func.instruction(&Instruction::I32Add);
                            func.instruction(&Instruction::LocalGet(ptr_temp));
                            func.instruction(&Instruction::I32Load(MemArg {
                                offset: 0,
                                align: 2,
                                memory_index: 0,
                            }));
                            if elem_size > 1 {
                                func.instruction(&Instruction::I32Const(elem_size as i32));
                                func.instruction(&Instruction::I32Mul);
                            }
                            func.instruction(&Instruction::I32Add);
                            self.emit_load_at_offset(&elem, 0, func);
                        }
                        _ => {
                            func.instruction(&Instruction::I32Const(0));
                        }
                    }
                }
                // Tier 4: dict methods (via bridge imports).
                WasmType::PtrDict(_, _) => {
                    let import_name = match attr.as_str() {
                        "get" => "__dict_get_str",
                        _ => "",
                    };
                    if !import_name.is_empty() {
                        if let Some(&idx) = self.import_indices.get(import_name) {
                            self.emit_expr(value, ctx, func);
                            for arg in args {
                                self.emit_expr(arg, ctx, func);
                            }
                            func.instruction(&Instruction::Call(idx));
                            return;
                        }
                    }
                    func.instruction(&Instruction::I64Const(0));
                }
                _ => {
                    func.instruction(&Instruction::I64Const(0));
                }
            }
        }
    }

    fn emit_if_expr(
        &self,
        test: &Expr,
        body: &Expr,
        else_body: &Expr,
        ctx: &mut FuncContext,
        func: &mut Function,
    ) {
        let body_type = self.expr_type(body, ctx);
        let else_type = self.expr_type(else_body, ctx);
        let result_type = self.arithmetic_op_type(&body_type, &else_type);
        let block_type = wasm_encoder::BlockType::Result(result_type.to_val_type());

        self.emit_condition(test, ctx, func);
        func.instruction(&Instruction::If(block_type));

        self.emit_expr(body, ctx, func);
        if body_type != result_type {
            self.emit_convert(&body_type, &result_type, func);
        }

        func.instruction(&Instruction::Else);

        self.emit_expr(else_body, ctx, func);
        if else_type != result_type {
            self.emit_convert(&else_type, &result_type, func);
        }

        func.instruction(&Instruction::End);
    }

    // === Helpers ===

    /// Emit an expression and ensure the result is i32 (for use as a WASM condition).
    fn emit_condition(&self, expr: &Expr, ctx: &mut FuncContext, func: &mut Function) {
        let ty = self.expr_type(expr, ctx);
        self.emit_expr(expr, ctx, func);

        match ty {
            WasmType::I64 => {
                // i64 → i32 condition: x != 0
                func.instruction(&Instruction::I64Const(0));
                func.instruction(&Instruction::I64Ne);
            }
            WasmType::F64 => {
                // f64 → i32 condition: x != 0.0
                func.instruction(&Instruction::F64Const(0.0));
                func.instruction(&Instruction::F64Ne);
            }
            // I32 (bool), Ptr (str), or any collection/closure ptr — already i32,
            // and the truthiness convention is "non-null/non-zero".
            _ => {}
        }
    }

    /// Determine the type to use for arithmetic between two types.
    fn arithmetic_op_type(&self, lt: &WasmType, rt: &WasmType) -> WasmType {
        if *lt == WasmType::F64 || *rt == WasmType::F64 {
            WasmType::F64
        } else if *lt == WasmType::I64 || *rt == WasmType::I64 {
            WasmType::I64
        } else {
            // Both i32 (bool) — promote to i64 for arithmetic
            WasmType::I64
        }
    }

    /// Infer the WASM type of an expression given the current function context.
    fn expr_type(&self, expr: &Expr, ctx: &FuncContext) -> WasmType {
        match &expr.kind {
            ExprKind::IntLiteral(_) => WasmType::I64,
            ExprKind::FloatLiteral(_) => WasmType::F64,
            ExprKind::BoolLiteral(_) => WasmType::I32,
            ExprKind::StringLiteral(_) => WasmType::Ptr,
            ExprKind::FString { .. } => WasmType::Ptr,
            // Step 2/3/4: collection literals are i32 pointers/handles.
            // The element types live in the WasmType variants for downstream
            // consumers; here we just return the structural type.
            ExprKind::Tuple(elts) => {
                let elt_types: Vec<WasmType> =
                    elts.iter().map(|e| self.expr_type(e, ctx)).collect();
                WasmType::PtrTuple(elt_types)
            }
            ExprKind::List(elts) => {
                let inner = if let Some(e) = elts.first() {
                    self.expr_type(e, ctx)
                } else {
                    WasmType::I64
                };
                WasmType::PtrList(Box::new(inner))
            }
            ExprKind::Dict { .. } => {
                WasmType::PtrDict(Box::new(WasmType::Ptr), Box::new(WasmType::I64))
            }
            ExprKind::Lambda { params, body } => {
                let wparams: Vec<(String, WasmType)> = params
                    .iter()
                    .map(|p| {
                        let ty = p
                            .annotation
                            .as_ref()
                            .map(|a| resolve_type(a))
                            .unwrap_or(Type::Int);
                        (p.name.clone(), to_wasm_type(&ty).unwrap_or(WasmType::I64))
                    })
                    .collect();
                let ret = Self::infer_lambda_return_type(body, &wparams);
                WasmType::PtrClosure {
                    params: wparams.into_iter().map(|(_, t)| t).collect(),
                    ret: ret.map(Box::new),
                }
            }
            ExprKind::ListComp { .. } => WasmType::PtrList(Box::new(WasmType::I64)),
            ExprKind::DictComp { .. } => {
                WasmType::PtrDict(Box::new(WasmType::Ptr), Box::new(WasmType::I64))
            }
            ExprKind::SetComp { .. } => WasmType::PtrList(Box::new(WasmType::I64)),
            ExprKind::Name(name) => {
                if let Some(cap) = ctx.captures.iter().find(|c| c.name == *name) {
                    return cap.ty.clone();
                }
                ctx.get_local(name)
                    .map(|(_, wt)| wt)
                    .unwrap_or(WasmType::I64)
            }
            ExprKind::BinOp { left, op, right } => {
                match op {
                    BinOp::Div => WasmType::F64,
                    // #358: int ** int is exact i64 (checked pow loop);
                    // any float operand keeps the f64 math.pow path.
                    BinOp::Pow => {
                        let lt = self.expr_type(left, ctx);
                        let rt = self.expr_type(right, ctx);
                        if lt == WasmType::F64 || rt == WasmType::F64 {
                            WasmType::F64
                        } else {
                            WasmType::I64
                        }
                    }
                    BinOp::Eq
                    | BinOp::NotEq
                    | BinOp::Lt
                    | BinOp::LtEq
                    | BinOp::Gt
                    | BinOp::GtEq
                    | BinOp::And
                    | BinOp::Or => WasmType::I32,
                    BinOp::FloorDiv => {
                        let lt = self.expr_type(left, ctx);
                        let rt = self.expr_type(right, ctx);
                        if lt == WasmType::F64 || rt == WasmType::F64 {
                            WasmType::F64
                        } else {
                            WasmType::I64
                        }
                    }
                    BinOp::Mod => {
                        let lt = self.expr_type(left, ctx);
                        let rt = self.expr_type(right, ctx);
                        if lt == WasmType::F64 || rt == WasmType::F64 {
                            WasmType::F64
                        } else {
                            WasmType::I64
                        }
                    }
                    BinOp::Add => {
                        let lt = self.expr_type(left, ctx);
                        let rt = self.expr_type(right, ctx);
                        if lt == WasmType::Ptr || rt == WasmType::Ptr {
                            WasmType::Ptr
                        } else {
                            self.arithmetic_op_type(&lt, &rt)
                        }
                    }
                    // List repetition: PtrList * Int or Int * PtrList → PtrList (B-033)
                    BinOp::Mul => {
                        let lt = self.expr_type(left, ctx);
                        let rt = self.expr_type(right, ctx);
                        if matches!(lt, WasmType::PtrList(_))
                            && (rt == WasmType::I64 || rt == WasmType::I32)
                        {
                            lt
                        } else if matches!(rt, WasmType::PtrList(_))
                            && (lt == WasmType::I64 || lt == WasmType::I32)
                        {
                            rt
                        } else {
                            self.arithmetic_op_type(&lt, &rt)
                        }
                    }
                    _ => {
                        let lt = self.expr_type(left, ctx);
                        let rt = self.expr_type(right, ctx);
                        self.arithmetic_op_type(&lt, &rt)
                    }
                }
            }
            ExprKind::UnaryOp { op, operand } => match op {
                UnaryOp::Not => WasmType::I32,
                UnaryOp::Neg | UnaryOp::Pos => self.expr_type(operand, ctx),
                UnaryOp::BitNot => {
                    let ot = self.expr_type(operand, ctx);
                    if ot == WasmType::F64 {
                        WasmType::F64
                    } else {
                        ot
                    }
                }
            },
            ExprKind::Compare { .. } => WasmType::I32,
            ExprKind::Call {
                func: callee, args, ..
            } => {
                if let ExprKind::Name(name) = &callee.kind {
                    match name.as_str() {
                        // Livermore finding (2026-07-10): int()/float()
                        // numeric conversions (previously unhandled — the
                        // call emitted NOTHING, underflowing the operand
                        // stack; k13/k14 int-from-float indexing).
                        "int" => WasmType::I64,
                        "float" => WasmType::F64,
                        "abs" => {
                            if !args.is_empty() {
                                self.expr_type(&args[0], ctx)
                            } else {
                                WasmType::I64
                            }
                        }
                        "len" => WasmType::I64,
                        "min" | "max" => {
                            if args.len() == 2 {
                                let a = self.expr_type(&args[0], ctx);
                                let b = self.expr_type(&args[1], ctx);
                                self.arithmetic_op_type(&a, &b)
                            } else {
                                WasmType::I64
                            }
                        }
                        // Tier 6 HoF return-type inference.
                        "map" => {
                            let elem = if let Some(fn_arg) = args.first() {
                                if let WasmType::PtrClosure { ret, .. } =
                                    self.expr_type(fn_arg, ctx)
                                {
                                    ret.map(|b| (*b).clone()).unwrap_or(WasmType::I64)
                                } else {
                                    WasmType::I64
                                }
                            } else {
                                WasmType::I64
                            };
                            WasmType::PtrList(Box::new(elem))
                        }
                        "filter" | "sorted" => {
                            let lst_arg_idx = if name == "filter" { 1 } else { 0 };
                            let elem = if let Some(lst_arg) = args.get(lst_arg_idx) {
                                if let WasmType::PtrList(inner) = self.expr_type(lst_arg, ctx) {
                                    (*inner).clone()
                                } else {
                                    WasmType::I64
                                }
                            } else {
                                WasmType::I64
                            };
                            WasmType::PtrList(Box::new(elem))
                        }
                        "reduce" => {
                            if let Some(init) = args.get(2) {
                                self.expr_type(init, ctx)
                            } else {
                                WasmType::I64
                            }
                        }
                        _ => {
                            // Bare math call (`sqrt(...)` via `from math import`)
                            // always returns f64.
                            if self.math_aliases.contains_key(name) {
                                WasmType::F64
                            } else if let Some(info) = self.func_info.get(name.as_str()) {
                                to_wasm_type(&info.return_type).unwrap_or(WasmType::I64)
                            } else {
                                WasmType::I64
                            }
                        }
                    }
                } else if let ExprKind::Attribute { value, attr, .. } = &callee.kind {
                    // math.X(...) calls always return f64
                    if let ExprKind::Name(mod_name) = &value.kind {
                        if mod_name == "math" && math_function_arity(attr.as_str()).is_some() {
                            return WasmType::F64;
                        }
                    }
                    let val_type = self.expr_type(value, ctx);
                    if val_type == WasmType::Ptr {
                        match attr.as_str() {
                            "upper" | "lower" => WasmType::Ptr,
                            "find" => WasmType::I64,
                            "startswith" | "endswith" => WasmType::I32,
                            _ => WasmType::I64,
                        }
                    } else {
                        WasmType::I64
                    }
                } else {
                    WasmType::I64
                }
            }
            ExprKind::IfExpr {
                body, else_body, ..
            } => {
                let bt = self.expr_type(body, ctx);
                let et = self.expr_type(else_body, ctx);
                self.arithmetic_op_type(&bt, &et)
            }
            ExprKind::Subscript { value, index, .. } => {
                let vt = self.expr_type(value, ctx);
                match &vt {
                    // String indexing returns a 1-char string.
                    WasmType::Ptr => WasmType::Ptr,
                    // Tuple: element type at constant index.
                    WasmType::PtrTuple(elt_types) => {
                        if let ExprKind::IntLiteral(i) = &index.kind {
                            let idx = *i as usize;
                            if idx < elt_types.len() {
                                return elt_types[idx].clone();
                            }
                        }
                        WasmType::I64
                    }
                    // List: element type from list signature.
                    WasmType::PtrList(elem_ty) => (**elem_ty).clone(),
                    // Dict: value type from dict signature.
                    WasmType::PtrDict(_, v_ty) => (**v_ty).clone(),
                    _ => WasmType::I64,
                }
            }
            ExprKind::Attribute { value, attr, .. } => {
                // math.pi, math.e etc. â†’ f64
                if let ExprKind::Name(mod_name) = &value.kind {
                    if mod_name == "math" && math_constant_value(attr.as_str()).is_some() {
                        return WasmType::F64;
                    }
                }
                WasmType::I64
            }
            _ => WasmType::I64,
        }
    }

    /// Emit a type conversion instruction. Takes by reference so callers
    /// can re-use the `WasmType` afterwards (matters for collection variants
    /// that aren't `Copy`).
    fn emit_convert(&self, from: &WasmType, to: &WasmType, func: &mut Function) {
        if from == to {
            return;
        }
        // All "any-pointer" types (string ptr, list ptr, dict handle, tuple ptr,
        // closure ptr) are i32 in WASM. Conversions among them are no-ops.
        if from.is_any_ptr() && to.is_any_ptr() {
            return;
        }
        // Mixed primitive â†” ptr: treat ptrs as I32 for the conversion table.
        let from_simple = if from.is_any_ptr() {
            &WasmType::I32
        } else {
            from
        };
        let to_simple = if to.is_any_ptr() { &WasmType::I32 } else { to };
        match (from_simple, to_simple) {
            (WasmType::I64, WasmType::F64) => {
                func.instruction(&Instruction::F64ConvertI64S);
            }
            (WasmType::I32, WasmType::F64) => {
                func.instruction(&Instruction::F64ConvertI32S);
            }
            (WasmType::I32, WasmType::I64) => {
                func.instruction(&Instruction::I64ExtendI32S);
            }
            (WasmType::F64, WasmType::I64) => {
                func.instruction(&Instruction::I64TruncF64S);
            }
            (WasmType::F64, WasmType::I32) => {
                func.instruction(&Instruction::I32TruncF64S);
            }
            (WasmType::I64, WasmType::I32) => {
                func.instruction(&Instruction::I32WrapI64);
            }
            _ => {}
        }
    }

    fn emit_promote(&self, from: WasmType, to: WasmType, func: &mut Function) {
        self.emit_convert(&from, &to, func);
    }

    /// #358: set the `__ovf` exactness flag.
    fn emit_set_ovf(&self, func: &mut Function) {
        func.instruction(&Instruction::I32Const(1));
        func.instruction(&Instruction::GlobalSet(self.ovf_global_idx));
    }

    /// #358: overflow-checked i64 add/sub. Consumes `[a, b]` from the stack,
    /// leaves the (wrapping) result, and sets `__ovf` when the exact result
    /// does not fit i64. Standard sign-bit checks:
    ///   add: overflow iff `((a^c) & (b^c)) < 0` where c = a+b
    ///   sub: overflow iff `((a^b) & (a^c)) < 0` where c = a-b
    fn emit_checked_i64_addsub(&self, is_add: bool, ctx: &FuncContext, func: &mut Function) {
        let (a, b, c) = (ctx.ck_i64[0], ctx.ck_i64[1], ctx.ck_i64[2]);
        func.instruction(&Instruction::LocalSet(b));
        func.instruction(&Instruction::LocalSet(a));
        func.instruction(&Instruction::LocalGet(a));
        func.instruction(&Instruction::LocalGet(b));
        if is_add {
            func.instruction(&Instruction::I64Add);
        } else {
            func.instruction(&Instruction::I64Sub);
        }
        func.instruction(&Instruction::LocalSet(c));
        func.instruction(&Instruction::LocalGet(a));
        func.instruction(&Instruction::LocalGet(if is_add { c } else { b }));
        func.instruction(&Instruction::I64Xor);
        func.instruction(&Instruction::LocalGet(if is_add { b } else { a }));
        func.instruction(&Instruction::LocalGet(c));
        func.instruction(&Instruction::I64Xor);
        func.instruction(&Instruction::I64And);
        func.instruction(&Instruction::I64Const(0));
        func.instruction(&Instruction::I64LtS);
        func.instruction(&Instruction::If(wasm_encoder::BlockType::Empty));
        self.emit_set_ovf(func);
        func.instruction(&Instruction::End);
        func.instruction(&Instruction::LocalGet(c));
    }

    /// #358: overflow-checked i64 mul. Consumes `[a, b]`, leaves the
    /// (wrapping) product, sets `__ovf` on overflow. Division-based check:
    ///   a == 0            → never overflows
    ///   a == -1           → overflows iff b == i64::MIN
    ///   otherwise         → overflows iff (a*b)/a != b   (div_s is safe:
    ///                        a ∉ {0, -1} excludes both trap conditions)
    fn emit_checked_i64_mul(&self, ctx: &FuncContext, func: &mut Function) {
        let (a, b, _c) = (ctx.ck_i64[0], ctx.ck_i64[1], ctx.ck_i64[2]);
        func.instruction(&Instruction::LocalSet(b));
        func.instruction(&Instruction::LocalSet(a));
        // Fast path (perf: Livermore k21 was 5.8x slower with an
        // unconditional div-based check): if both operands fit in i32
        // (|x| < 2^31), the product magnitude is < 2^62 — overflow is
        // impossible, so multiply and skip the check. Index arithmetic,
        // loop counters and most real operands take this branch; the
        // division check only runs for genuinely large operands.
        // Combined in-range test: ((a + 2^31) | (b + 2^31)) >> 32 == 0
        // (unsigned) — true iff both operands are in [-2^31, 2^31).
        func.instruction(&Instruction::LocalGet(a));
        func.instruction(&Instruction::I64Const(0x8000_0000));
        func.instruction(&Instruction::I64Add);
        func.instruction(&Instruction::LocalGet(b));
        func.instruction(&Instruction::I64Const(0x8000_0000));
        func.instruction(&Instruction::I64Add);
        func.instruction(&Instruction::I64Or);
        func.instruction(&Instruction::I64Const(32));
        func.instruction(&Instruction::I64ShrU);
        func.instruction(&Instruction::I64Eqz);
        func.instruction(&Instruction::If(wasm_encoder::BlockType::Result(
            ValType::I64,
        )));
        {
            func.instruction(&Instruction::LocalGet(a));
            func.instruction(&Instruction::LocalGet(b));
            func.instruction(&Instruction::I64Mul);
        }
        func.instruction(&Instruction::Else);
        {
            self.emit_checked_i64_mul_slow(ctx, func);
        }
        func.instruction(&Instruction::End);
    }

    /// Slow path of the checked mul: full division-based overflow
    /// detection. Expects operands already in `ck[0]`/`ck[1]`; leaves the
    /// (wrapping) product on the stack.
    fn emit_checked_i64_mul_slow(&self, ctx: &FuncContext, func: &mut Function) {
        let (a, b, c) = (ctx.ck_i64[0], ctx.ck_i64[1], ctx.ck_i64[2]);
        func.instruction(&Instruction::LocalGet(a));
        func.instruction(&Instruction::LocalGet(b));
        func.instruction(&Instruction::I64Mul);
        func.instruction(&Instruction::LocalSet(c));
        // if a != 0
        func.instruction(&Instruction::LocalGet(a));
        func.instruction(&Instruction::I64Eqz);
        func.instruction(&Instruction::I32Eqz);
        func.instruction(&Instruction::If(wasm_encoder::BlockType::Empty));
        {
            // if a == -1: ovf iff b == i64::MIN
            func.instruction(&Instruction::LocalGet(a));
            func.instruction(&Instruction::I64Const(-1));
            func.instruction(&Instruction::I64Eq);
            func.instruction(&Instruction::If(wasm_encoder::BlockType::Empty));
            {
                func.instruction(&Instruction::LocalGet(b));
                func.instruction(&Instruction::I64Const(i64::MIN));
                func.instruction(&Instruction::I64Eq);
                func.instruction(&Instruction::If(wasm_encoder::BlockType::Empty));
                self.emit_set_ovf(func);
                func.instruction(&Instruction::End);
            }
            func.instruction(&Instruction::Else);
            {
                func.instruction(&Instruction::LocalGet(c));
                func.instruction(&Instruction::LocalGet(a));
                func.instruction(&Instruction::I64DivS);
                func.instruction(&Instruction::LocalGet(b));
                func.instruction(&Instruction::I64Ne);
                func.instruction(&Instruction::If(wasm_encoder::BlockType::Empty));
                self.emit_set_ovf(func);
                func.instruction(&Instruction::End);
            }
            func.instruction(&Instruction::End);
        }
        func.instruction(&Instruction::End);
        func.instruction(&Instruction::LocalGet(c));
    }

    /// #358: exact i64 shift-left. Consumes `[a, b]`, leaves the result.
    /// Python `<<` grows without bound and rejects negative counts, while
    /// WASM `i64.shl` masks the count mod 64 — so:
    ///   b < 0                → flag (ValueError on the exact JS path)
    ///   b >= 64 with a != 0  → flag (result exceeds i64)
    ///   b >= 64 with a == 0  → 0
    ///   else                 → c = a << b, flag iff (c >> b) != a
    fn emit_checked_i64_shl(&self, ctx: &FuncContext, func: &mut Function) {
        let (a, b, c) = (ctx.ck_i64[0], ctx.ck_i64[1], ctx.ck_i64[2]);
        func.instruction(&Instruction::LocalSet(b));
        func.instruction(&Instruction::LocalSet(a));
        func.instruction(&Instruction::LocalGet(b));
        func.instruction(&Instruction::I64Const(0));
        func.instruction(&Instruction::I64LtS);
        func.instruction(&Instruction::If(wasm_encoder::BlockType::Empty));
        {
            self.emit_set_ovf(func);
            func.instruction(&Instruction::I64Const(0));
            func.instruction(&Instruction::LocalSet(c));
        }
        func.instruction(&Instruction::Else);
        {
            func.instruction(&Instruction::LocalGet(b));
            func.instruction(&Instruction::I64Const(64));
            func.instruction(&Instruction::I64GeS);
            func.instruction(&Instruction::If(wasm_encoder::BlockType::Empty));
            {
                func.instruction(&Instruction::LocalGet(a));
                func.instruction(&Instruction::I64Eqz);
                func.instruction(&Instruction::I32Eqz);
                func.instruction(&Instruction::If(wasm_encoder::BlockType::Empty));
                self.emit_set_ovf(func);
                func.instruction(&Instruction::End);
                func.instruction(&Instruction::I64Const(0));
                func.instruction(&Instruction::LocalSet(c));
            }
            func.instruction(&Instruction::Else);
            {
                func.instruction(&Instruction::LocalGet(a));
                func.instruction(&Instruction::LocalGet(b));
                func.instruction(&Instruction::I64Shl);
                func.instruction(&Instruction::LocalSet(c));
                func.instruction(&Instruction::LocalGet(c));
                func.instruction(&Instruction::LocalGet(b));
                func.instruction(&Instruction::I64ShrS);
                func.instruction(&Instruction::LocalGet(a));
                func.instruction(&Instruction::I64Ne);
                func.instruction(&Instruction::If(wasm_encoder::BlockType::Empty));
                self.emit_set_ovf(func);
                func.instruction(&Instruction::End);
            }
            func.instruction(&Instruction::End);
        }
        func.instruction(&Instruction::End);
        func.instruction(&Instruction::LocalGet(c));
    }

    /// #358: exact i64 shift-right. Python `>>` saturates (b >= 64 gives
    /// 0 / -1) and rejects negative counts; WASM masks the count mod 64.
    /// Clamping the count to 63 makes the arithmetic shift exact for every
    /// non-negative count; negative counts flag for the exact JS path.
    fn emit_checked_i64_shr(&self, ctx: &FuncContext, func: &mut Function) {
        let (a, b, c) = (ctx.ck_i64[0], ctx.ck_i64[1], ctx.ck_i64[2]);
        func.instruction(&Instruction::LocalSet(b));
        func.instruction(&Instruction::LocalSet(a));
        func.instruction(&Instruction::LocalGet(b));
        func.instruction(&Instruction::I64Const(0));
        func.instruction(&Instruction::I64LtS);
        func.instruction(&Instruction::If(wasm_encoder::BlockType::Empty));
        {
            self.emit_set_ovf(func);
            func.instruction(&Instruction::I64Const(0));
            func.instruction(&Instruction::LocalSet(c));
        }
        func.instruction(&Instruction::Else);
        {
            func.instruction(&Instruction::LocalGet(b));
            func.instruction(&Instruction::I64Const(63));
            func.instruction(&Instruction::I64GtS);
            func.instruction(&Instruction::If(wasm_encoder::BlockType::Empty));
            func.instruction(&Instruction::I64Const(63));
            func.instruction(&Instruction::LocalSet(b));
            func.instruction(&Instruction::End);
            func.instruction(&Instruction::LocalGet(a));
            func.instruction(&Instruction::LocalGet(b));
            func.instruction(&Instruction::I64ShrS);
            func.instruction(&Instruction::LocalSet(c));
        }
        func.instruction(&Instruction::End);
        func.instruction(&Instruction::LocalGet(c));
    }

    /// #358: exact i64 floor division (Python semantics — floor toward
    /// -inf). Replaces the old f64 round-trip, which silently lost
    /// precision past 2^53 even for in-range values. Only i64::MIN // -1
    /// overflows (flags); division by zero traps exactly as before.
    fn emit_checked_i64_floordiv(&self, ctx: &FuncContext, func: &mut Function) {
        let (a, b, c, d) = (ctx.ck_i64[0], ctx.ck_i64[1], ctx.ck_i64[2], ctx.ck_i64[3]);
        func.instruction(&Instruction::LocalSet(b));
        func.instruction(&Instruction::LocalSet(a));
        func.instruction(&Instruction::LocalGet(a));
        func.instruction(&Instruction::I64Const(i64::MIN));
        func.instruction(&Instruction::I64Eq);
        func.instruction(&Instruction::LocalGet(b));
        func.instruction(&Instruction::I64Const(-1));
        func.instruction(&Instruction::I64Eq);
        func.instruction(&Instruction::I32And);
        func.instruction(&Instruction::If(wasm_encoder::BlockType::Empty));
        {
            self.emit_set_ovf(func);
            func.instruction(&Instruction::I64Const(0));
            func.instruction(&Instruction::LocalSet(c));
        }
        func.instruction(&Instruction::Else);
        {
            func.instruction(&Instruction::LocalGet(a));
            func.instruction(&Instruction::LocalGet(b));
            func.instruction(&Instruction::I64DivS);
            func.instruction(&Instruction::LocalSet(c));
            func.instruction(&Instruction::LocalGet(a));
            func.instruction(&Instruction::LocalGet(b));
            func.instruction(&Instruction::I64RemS);
            func.instruction(&Instruction::LocalSet(d));
            // if r != 0 and sign(a) != sign(b): q -= 1
            func.instruction(&Instruction::LocalGet(d));
            func.instruction(&Instruction::I64Eqz);
            func.instruction(&Instruction::I32Eqz);
            func.instruction(&Instruction::LocalGet(a));
            func.instruction(&Instruction::LocalGet(b));
            func.instruction(&Instruction::I64Xor);
            func.instruction(&Instruction::I64Const(0));
            func.instruction(&Instruction::I64LtS);
            func.instruction(&Instruction::I32And);
            func.instruction(&Instruction::If(wasm_encoder::BlockType::Empty));
            func.instruction(&Instruction::LocalGet(c));
            func.instruction(&Instruction::I64Const(1));
            func.instruction(&Instruction::I64Sub);
            func.instruction(&Instruction::LocalSet(c));
            func.instruction(&Instruction::End);
        }
        func.instruction(&Instruction::End);
        func.instruction(&Instruction::LocalGet(c));
    }

    /// #358: exact i64 mod (Python semantics — result takes the divisor's
    /// sign). `r = a rem b; if r != 0 and sign(r) != sign(b): r += b`.
    /// Replaces the old `((a % b) + b) % b`, whose intermediate `+ b`
    /// could itself wrap. `r += b` cannot overflow (|r| < |b|, opposite
    /// signs). i64::MIN rem -1 is defined (0) in WASM, matching Python.
    fn emit_checked_i64_mod(&self, ctx: &FuncContext, func: &mut Function) {
        let (a, b, c) = (ctx.ck_i64[0], ctx.ck_i64[1], ctx.ck_i64[2]);
        func.instruction(&Instruction::LocalSet(b));
        func.instruction(&Instruction::LocalSet(a));
        func.instruction(&Instruction::LocalGet(a));
        func.instruction(&Instruction::LocalGet(b));
        func.instruction(&Instruction::I64RemS);
        func.instruction(&Instruction::LocalSet(c));
        func.instruction(&Instruction::LocalGet(c));
        func.instruction(&Instruction::I64Eqz);
        func.instruction(&Instruction::I32Eqz);
        func.instruction(&Instruction::LocalGet(c));
        func.instruction(&Instruction::LocalGet(b));
        func.instruction(&Instruction::I64Xor);
        func.instruction(&Instruction::I64Const(0));
        func.instruction(&Instruction::I64LtS);
        func.instruction(&Instruction::I32And);
        func.instruction(&Instruction::If(wasm_encoder::BlockType::Empty));
        func.instruction(&Instruction::LocalGet(c));
        func.instruction(&Instruction::LocalGet(b));
        func.instruction(&Instruction::I64Add);
        func.instruction(&Instruction::LocalSet(c));
        func.instruction(&Instruction::End);
        func.instruction(&Instruction::LocalGet(c));
    }

    /// #358: exact integer pow (square-and-multiply over checked i64 muls).
    /// Consumes `[base, exp]`, leaves the result. Negative exponents flag
    /// (the exact result is a float — the JS twin handles it); any i64
    /// overflow inside the loop flags via the checked mul.
    ///
    /// Spurious-flag argument: `acc * base` runs only when the current
    /// exponent bit is set, so its product is a factor of the final result;
    /// `base * base` runs only while remaining-exponent > 0, in which case
    /// the final |result| >= |base^2| (|acc| >= 1 whenever base != 0, and
    /// base == 0 never overflows). So every flag is a genuine overflow.
    fn emit_checked_i64_pow(&self, ctx: &FuncContext, func: &mut Function) {
        let (base, exp, acc) = (ctx.pw_i64[0], ctx.pw_i64[1], ctx.pw_i64[2]);
        func.instruction(&Instruction::LocalSet(exp));
        func.instruction(&Instruction::LocalSet(base));
        func.instruction(&Instruction::I64Const(1));
        func.instruction(&Instruction::LocalSet(acc));
        func.instruction(&Instruction::LocalGet(exp));
        func.instruction(&Instruction::I64Const(0));
        func.instruction(&Instruction::I64LtS);
        func.instruction(&Instruction::If(wasm_encoder::BlockType::Empty));
        self.emit_set_ovf(func);
        func.instruction(&Instruction::Else);
        {
            func.instruction(&Instruction::Block(wasm_encoder::BlockType::Empty));
            func.instruction(&Instruction::Loop(wasm_encoder::BlockType::Empty));
            func.instruction(&Instruction::LocalGet(exp));
            func.instruction(&Instruction::I64Eqz);
            func.instruction(&Instruction::BrIf(1));
            // if exp & 1: acc = checked_mul(acc, base)
            func.instruction(&Instruction::LocalGet(exp));
            func.instruction(&Instruction::I64Const(1));
            func.instruction(&Instruction::I64And);
            func.instruction(&Instruction::I64Const(0));
            func.instruction(&Instruction::I64Ne);
            func.instruction(&Instruction::If(wasm_encoder::BlockType::Empty));
            func.instruction(&Instruction::LocalGet(acc));
            func.instruction(&Instruction::LocalGet(base));
            self.emit_checked_i64_mul(ctx, func);
            func.instruction(&Instruction::LocalSet(acc));
            func.instruction(&Instruction::End);
            // exp >>= 1
            func.instruction(&Instruction::LocalGet(exp));
            func.instruction(&Instruction::I64Const(1));
            func.instruction(&Instruction::I64ShrU);
            func.instruction(&Instruction::LocalSet(exp));
            // if exp != 0: base = checked_mul(base, base)
            func.instruction(&Instruction::LocalGet(exp));
            func.instruction(&Instruction::I64Eqz);
            func.instruction(&Instruction::I32Eqz);
            func.instruction(&Instruction::If(wasm_encoder::BlockType::Empty));
            func.instruction(&Instruction::LocalGet(base));
            func.instruction(&Instruction::LocalGet(base));
            self.emit_checked_i64_mul(ctx, func);
            func.instruction(&Instruction::LocalSet(base));
            func.instruction(&Instruction::End);
            func.instruction(&Instruction::Br(0));
            func.instruction(&Instruction::End);
            func.instruction(&Instruction::End);
        }
        func.instruction(&Instruction::End);
        func.instruction(&Instruction::LocalGet(acc));
    }

    /// Emit a numeric binary operation on two values already on the stack
    /// (both of type `ty`), leaving the result. i64 arithmetic is
    /// overflow-checked (#358): the result is either exact or the `__ovf`
    /// flag is set so the glue re-runs the call on the exact JS twin.
    fn emit_num_op_on_stack(
        &self,
        op: BinOp,
        ty: &WasmType,
        ctx: &FuncContext,
        func: &mut Function,
    ) {
        match ty {
            WasmType::I64 => match op {
                BinOp::Add => self.emit_checked_i64_addsub(true, ctx, func),
                BinOp::Sub => self.emit_checked_i64_addsub(false, ctx, func),
                BinOp::Mul => self.emit_checked_i64_mul(ctx, func),
                BinOp::BitAnd => {
                    func.instruction(&Instruction::I64And);
                }
                BinOp::BitOr => {
                    func.instruction(&Instruction::I64Or);
                }
                BinOp::BitXor => {
                    func.instruction(&Instruction::I64Xor);
                }
                BinOp::ShiftLeft => self.emit_checked_i64_shl(ctx, func),
                BinOp::ShiftRight => self.emit_checked_i64_shr(ctx, func),
                BinOp::FloorDiv => self.emit_checked_i64_floordiv(ctx, func),
                BinOp::Mod => self.emit_checked_i64_mod(ctx, func),
                BinOp::Pow => self.emit_checked_i64_pow(ctx, func),
                // True division on int operands (aug-assign `x /= y` on an
                // i64 local): f64 round-trip, truncated back into the local.
                BinOp::Div => {
                    let (a, b) = (ctx.ck_i64[0], ctx.ck_i64[1]);
                    func.instruction(&Instruction::LocalSet(b));
                    func.instruction(&Instruction::LocalSet(a));
                    func.instruction(&Instruction::LocalGet(a));
                    func.instruction(&Instruction::F64ConvertI64S);
                    func.instruction(&Instruction::LocalGet(b));
                    func.instruction(&Instruction::F64ConvertI64S);
                    func.instruction(&Instruction::F64Div);
                    func.instruction(&Instruction::I64TruncF64S);
                }
                _ => {}
            },
            WasmType::F64 => match op {
                BinOp::Add => {
                    func.instruction(&Instruction::F64Add);
                }
                BinOp::Sub => {
                    func.instruction(&Instruction::F64Sub);
                }
                BinOp::Mul => {
                    func.instruction(&Instruction::F64Mul);
                }
                BinOp::Div => {
                    func.instruction(&Instruction::F64Div);
                }
                BinOp::FloorDiv => {
                    func.instruction(&Instruction::F64Div);
                    func.instruction(&Instruction::F64Floor);
                }
                BinOp::Mod => {
                    // Python float mod: a - floor(a/b) * b, via scratch
                    // locals (no operand re-emission).
                    let (x, y) = (ctx.ck_f64[0], ctx.ck_f64[1]);
                    func.instruction(&Instruction::LocalSet(y));
                    func.instruction(&Instruction::LocalSet(x));
                    func.instruction(&Instruction::LocalGet(x));
                    func.instruction(&Instruction::LocalGet(x));
                    func.instruction(&Instruction::LocalGet(y));
                    func.instruction(&Instruction::F64Div);
                    func.instruction(&Instruction::F64Floor);
                    func.instruction(&Instruction::LocalGet(y));
                    func.instruction(&Instruction::F64Mul);
                    func.instruction(&Instruction::F64Sub);
                }
                BinOp::Pow => {
                    let idx = *self
                        .import_indices
                        .get("pow")
                        .expect("pow import not registered");
                    func.instruction(&Instruction::Call(idx));
                }
                _ => {}
            },
            // I32 (bool), Ptr, or any collection/closure ptr — i32 ops
            _ => match op {
                BinOp::Add => {
                    func.instruction(&Instruction::I32Add);
                }
                BinOp::Sub => {
                    func.instruction(&Instruction::I32Sub);
                }
                BinOp::Mul => {
                    func.instruction(&Instruction::I32Mul);
                }
                BinOp::BitAnd => {
                    func.instruction(&Instruction::I32And);
                }
                BinOp::BitOr => {
                    func.instruction(&Instruction::I32Or);
                }
                BinOp::BitXor => {
                    func.instruction(&Instruction::I32Xor);
                }
                BinOp::ShiftLeft => {
                    func.instruction(&Instruction::I32Shl);
                }
                BinOp::ShiftRight => {
                    func.instruction(&Instruction::I32ShrS);
                }
                _ => {}
            },
        }
    }

    fn emit_aug_op(&self, op: AugAssignOp, ty: WasmType, ctx: &FuncContext, func: &mut Function) {
        let binop = match op {
            AugAssignOp::Add => BinOp::Add,
            AugAssignOp::Sub => BinOp::Sub,
            AugAssignOp::Mul => BinOp::Mul,
            AugAssignOp::Div => BinOp::Div,
            AugAssignOp::FloorDiv => BinOp::FloorDiv,
            AugAssignOp::Mod => BinOp::Mod,
            AugAssignOp::Pow => BinOp::Pow,
            AugAssignOp::BitAnd => BinOp::BitAnd,
            AugAssignOp::BitOr => BinOp::BitOr,
            AugAssignOp::BitXor => BinOp::BitXor,
            AugAssignOp::ShiftLeft => BinOp::ShiftLeft,
            AugAssignOp::ShiftRight => BinOp::ShiftRight,
            // `@=` never reaches the numeric-WASM path (matmul is pure
            // dunder dispatch on objects; the router keeps it on JS).
            AugAssignOp::MatMul => BinOp::MatMul,
        };
        self.emit_num_op_on_stack(binop, &ty, ctx, func);
    }

    // === String operation emission ===

    /// Helper to create a MemArg.
    fn memarg(offset: u64, align: u32) -> MemArg {
        MemArg {
            offset,
            align,
            memory_index: 0,
        }
    }

    /// Emit f-string assembly: concatenate all parts.
    fn emit_fstring(&self, parts: &[FStringPart], ctx: &mut FuncContext, func: &mut Function) {
        if parts.is_empty() {
            // Empty f-string â€” alloc a 4-byte zero-length string
            func.instruction(&Instruction::I32Const(4));
            func.instruction(&Instruction::Call(self.alloc_func_idx));
            func.instruction(&Instruction::LocalTee(ctx.str_temps[0]));
            func.instruction(&Instruction::I32Const(0));
            func.instruction(&Instruction::I32Store(Self::memarg(0, 2)));
            func.instruction(&Instruction::LocalGet(ctx.str_temps[0]));
            return;
        }

        if parts.len() == 1 {
            self.emit_fstring_part(&parts[0], ctx, func);
            return;
        }

        // Emit first part
        self.emit_fstring_part(&parts[0], ctx, func);

        // Concatenate each subsequent part
        let depth = ctx.str_depth;
        for part in &parts[1..] {
            // Save accumulated result
            ctx.str_depth = depth + 1;
            func.instruction(&Instruction::LocalSet(ctx.str_saves[depth]));

            // Emit next part (may use str_temps and deeper saves)
            self.emit_fstring_part(part, ctx, func);
            func.instruction(&Instruction::LocalSet(ctx.str_temps[1]));

            // Restore accumulated result
            func.instruction(&Instruction::LocalGet(ctx.str_saves[depth]));
            func.instruction(&Instruction::LocalSet(ctx.str_temps[0]));

            // Concat from temps[0] and temps[1]
            self.emit_string_concat_from_temps(ctx, func);
        }
        ctx.str_depth = depth;
    }

    fn emit_fstring_part(&self, part: &FStringPart, ctx: &mut FuncContext, func: &mut Function) {
        match part {
            FStringPart::Literal(s) => {
                if let Some(&offset) = self.string_dedup.get(s.as_str()) {
                    func.instruction(&Instruction::I32Const(offset as i32));
                } else {
                    func.instruction(&Instruction::I32Const(0));
                }
            }
            FStringPart::Expr(e) => {
                self.emit_expr(e, ctx, func);
            }
        }
    }

    /// Emit string concatenation: left + right (both Ptr).
    fn emit_string_concat(
        &self,
        left: &Expr,
        right: &Expr,
        ctx: &mut FuncContext,
        func: &mut Function,
    ) {
        let depth = ctx.str_depth;
        ctx.str_depth = depth + 1;

        // Evaluate left, save to depth-specific save slot
        self.emit_expr(left, ctx, func);
        func.instruction(&Instruction::LocalSet(ctx.str_saves[depth]));

        // Evaluate right (may use str_temps and deeper save slots)
        self.emit_expr(right, ctx, func);
        func.instruction(&Instruction::LocalSet(ctx.str_temps[1]));

        // Move left result from save to str_temps[0]
        func.instruction(&Instruction::LocalGet(ctx.str_saves[depth]));
        func.instruction(&Instruction::LocalSet(ctx.str_temps[0]));

        ctx.str_depth = depth;

        // Do the actual concat using str_temps[0..5]
        self.emit_string_concat_from_temps(ctx, func);
    }

    /// Concat strings in str_temps[0] (ptr1) and str_temps[1] (ptr2).
    /// Uses str_temps[2..5] internally. Leaves result ptr on stack.
    fn emit_string_concat_from_temps(&self, ctx: &FuncContext, func: &mut Function) {
        let t = &ctx.str_temps;

        // Load len1
        func.instruction(&Instruction::LocalGet(t[0]));
        func.instruction(&Instruction::I32Load(Self::memarg(0, 2)));
        func.instruction(&Instruction::LocalSet(t[2])); // len1

        // Load len2
        func.instruction(&Instruction::LocalGet(t[1]));
        func.instruction(&Instruction::I32Load(Self::memarg(0, 2)));
        func.instruction(&Instruction::LocalSet(t[3])); // len2

        // total_len = len1 + len2
        func.instruction(&Instruction::LocalGet(t[2]));
        func.instruction(&Instruction::LocalGet(t[3]));
        func.instruction(&Instruction::I32Add);
        func.instruction(&Instruction::LocalSet(t[5])); // total_len

        // result = __alloc(total_len + 4)
        func.instruction(&Instruction::LocalGet(t[5]));
        func.instruction(&Instruction::I32Const(4));
        func.instruction(&Instruction::I32Add);
        func.instruction(&Instruction::Call(self.alloc_func_idx));
        func.instruction(&Instruction::LocalSet(t[4])); // result

        // Store combined length at result
        func.instruction(&Instruction::LocalGet(t[4]));
        func.instruction(&Instruction::LocalGet(t[5]));
        func.instruction(&Instruction::I32Store(Self::memarg(0, 2)));

        // memory.copy(result+4, ptr1+4, len1)
        func.instruction(&Instruction::LocalGet(t[4]));
        func.instruction(&Instruction::I32Const(4));
        func.instruction(&Instruction::I32Add);
        func.instruction(&Instruction::LocalGet(t[0]));
        func.instruction(&Instruction::I32Const(4));
        func.instruction(&Instruction::I32Add);
        func.instruction(&Instruction::LocalGet(t[2]));
        func.instruction(&Instruction::MemoryCopy {
            dst_mem: 0,
            src_mem: 0,
        });

        // memory.copy(result+4+len1, ptr2+4, len2)
        func.instruction(&Instruction::LocalGet(t[4]));
        func.instruction(&Instruction::I32Const(4));
        func.instruction(&Instruction::I32Add);
        func.instruction(&Instruction::LocalGet(t[2]));
        func.instruction(&Instruction::I32Add);
        func.instruction(&Instruction::LocalGet(t[1]));
        func.instruction(&Instruction::I32Const(4));
        func.instruction(&Instruction::I32Add);
        func.instruction(&Instruction::LocalGet(t[3]));
        func.instruction(&Instruction::MemoryCopy {
            dst_mem: 0,
            src_mem: 0,
        });

        // Push result ptr
        func.instruction(&Instruction::LocalGet(t[4]));
    }

    /// Emit string equality comparison. Leaves i32 (0 or 1) on stack.
    fn emit_string_eq(
        &self,
        left: &Expr,
        right: &Expr,
        ctx: &mut FuncContext,
        func: &mut Function,
    ) {
        let depth = ctx.str_depth;
        ctx.str_depth = depth + 1;

        self.emit_expr(left, ctx, func);
        func.instruction(&Instruction::LocalSet(ctx.str_saves[depth]));

        self.emit_expr(right, ctx, func);

        ctx.str_depth = depth;
        func.instruction(&Instruction::LocalSet(ctx.str_temps[1])); // ptr2
        func.instruction(&Instruction::LocalGet(ctx.str_saves[depth]));
        func.instruction(&Instruction::LocalSet(ctx.str_temps[0])); // ptr1

        let t = &ctx.str_temps;

        // Load len1
        func.instruction(&Instruction::LocalGet(t[0]));
        func.instruction(&Instruction::I32Load(Self::memarg(0, 2)));
        func.instruction(&Instruction::LocalSet(t[2])); // len1

        // Load len2
        func.instruction(&Instruction::LocalGet(t[1]));
        func.instruction(&Instruction::I32Load(Self::memarg(0, 2)));
        func.instruction(&Instruction::LocalSet(t[3])); // len2

        // result = 1 (assume equal)
        func.instruction(&Instruction::I32Const(1));
        func.instruction(&Instruction::LocalSet(t[4])); // result

        // if len1 != len2: result = 0, else: byte comparison
        func.instruction(&Instruction::LocalGet(t[2]));
        func.instruction(&Instruction::LocalGet(t[3]));
        func.instruction(&Instruction::I32Ne);
        func.instruction(&Instruction::If(wasm_encoder::BlockType::Empty));
        func.instruction(&Instruction::I32Const(0));
        func.instruction(&Instruction::LocalSet(t[4]));
        func.instruction(&Instruction::Else);

        // idx = 0
        func.instruction(&Instruction::I32Const(0));
        func.instruction(&Instruction::LocalSet(t[5]));

        // block $done { loop $cmp { ... } }
        func.instruction(&Instruction::Block(wasm_encoder::BlockType::Empty));
        func.instruction(&Instruction::Loop(wasm_encoder::BlockType::Empty));

        // if idx >= len1: break (all equal)
        func.instruction(&Instruction::LocalGet(t[5]));
        func.instruction(&Instruction::LocalGet(t[2]));
        func.instruction(&Instruction::I32GeU);
        func.instruction(&Instruction::BrIf(1)); // br $done

        // Compare bytes: ptr1+4+idx vs ptr2+4+idx
        func.instruction(&Instruction::LocalGet(t[0]));
        func.instruction(&Instruction::LocalGet(t[5]));
        func.instruction(&Instruction::I32Add);
        func.instruction(&Instruction::I32Load8U(Self::memarg(4, 0)));

        func.instruction(&Instruction::LocalGet(t[1]));
        func.instruction(&Instruction::LocalGet(t[5]));
        func.instruction(&Instruction::I32Add);
        func.instruction(&Instruction::I32Load8U(Self::memarg(4, 0)));

        func.instruction(&Instruction::I32Ne);
        func.instruction(&Instruction::If(wasm_encoder::BlockType::Empty));
        func.instruction(&Instruction::I32Const(0));
        func.instruction(&Instruction::LocalSet(t[4])); // result = 0
        func.instruction(&Instruction::Br(2)); // br $done
        func.instruction(&Instruction::End); // end inner if

        // idx++
        func.instruction(&Instruction::LocalGet(t[5]));
        func.instruction(&Instruction::I32Const(1));
        func.instruction(&Instruction::I32Add);
        func.instruction(&Instruction::LocalSet(t[5]));

        func.instruction(&Instruction::Br(0)); // continue loop
        func.instruction(&Instruction::End); // end loop
        func.instruction(&Instruction::End); // end block

        func.instruction(&Instruction::End); // end outer if/else

        // Push result
        func.instruction(&Instruction::LocalGet(t[4]));
    }

    /// Emit string indexing: s[i] â†’ new 1-char string.
    fn emit_string_index(
        &self,
        value: &Expr,
        index: &Expr,
        ctx: &mut FuncContext,
        func: &mut Function,
    ) {
        let depth = ctx.str_depth;
        ctx.str_depth = depth + 1;

        self.emit_expr(value, ctx, func);
        func.instruction(&Instruction::LocalSet(ctx.str_saves[depth]));

        self.emit_expr(index, ctx, func);

        ctx.str_depth = depth;

        // Convert index from i64 to i32 if needed
        let idx_type = self.expr_type(index, ctx);
        if idx_type == WasmType::I64 {
            func.instruction(&Instruction::I32WrapI64);
        }
        func.instruction(&Instruction::LocalSet(ctx.str_temps[1])); // idx

        func.instruction(&Instruction::LocalGet(ctx.str_saves[depth]));
        func.instruction(&Instruction::LocalSet(ctx.str_temps[0])); // str_ptr

        let t = &ctx.str_temps;

        // Alloc 5 bytes (4 for length + 1 for char)
        func.instruction(&Instruction::I32Const(5));
        func.instruction(&Instruction::Call(self.alloc_func_idx));
        func.instruction(&Instruction::LocalSet(t[2])); // result_ptr

        // Store length = 1
        func.instruction(&Instruction::LocalGet(t[2]));
        func.instruction(&Instruction::I32Const(1));
        func.instruction(&Instruction::I32Store(Self::memarg(0, 2)));

        // Load byte from source: ptr + 4 + idx
        func.instruction(&Instruction::LocalGet(t[0]));
        func.instruction(&Instruction::LocalGet(t[1]));
        func.instruction(&Instruction::I32Add);
        func.instruction(&Instruction::I32Load8U(Self::memarg(4, 0)));
        func.instruction(&Instruction::LocalSet(t[3])); // byte

        // Store byte to result: result_ptr + 4
        func.instruction(&Instruction::LocalGet(t[2]));
        func.instruction(&Instruction::LocalGet(t[3]));
        func.instruction(&Instruction::I32Store8(Self::memarg(4, 0)));

        // Push result
        func.instruction(&Instruction::LocalGet(t[2]));
    }

    /// Emit upper() or lower() on a string. `to_upper`: true=upper, false=lower.
    fn emit_string_case(
        &self,
        value: &Expr,
        to_upper: bool,
        ctx: &mut FuncContext,
        func: &mut Function,
    ) {
        self.emit_expr(value, ctx, func);
        func.instruction(&Instruction::LocalSet(ctx.str_temps[0])); // src_ptr

        let t = &ctx.str_temps;

        // Load length
        func.instruction(&Instruction::LocalGet(t[0]));
        func.instruction(&Instruction::I32Load(Self::memarg(0, 2)));
        func.instruction(&Instruction::LocalSet(t[1])); // len

        // Alloc new string
        func.instruction(&Instruction::LocalGet(t[1]));
        func.instruction(&Instruction::I32Const(4));
        func.instruction(&Instruction::I32Add);
        func.instruction(&Instruction::Call(self.alloc_func_idx));
        func.instruction(&Instruction::LocalSet(t[2])); // dst_ptr

        // Store length
        func.instruction(&Instruction::LocalGet(t[2]));
        func.instruction(&Instruction::LocalGet(t[1]));
        func.instruction(&Instruction::I32Store(Self::memarg(0, 2)));

        // idx = 0
        func.instruction(&Instruction::I32Const(0));
        func.instruction(&Instruction::LocalSet(t[3])); // idx

        // Loop: copy and transform
        func.instruction(&Instruction::Block(wasm_encoder::BlockType::Empty));
        func.instruction(&Instruction::Loop(wasm_encoder::BlockType::Empty));

        // if idx >= len: break
        func.instruction(&Instruction::LocalGet(t[3]));
        func.instruction(&Instruction::LocalGet(t[1]));
        func.instruction(&Instruction::I32GeU);
        func.instruction(&Instruction::BrIf(1));

        // Load byte
        func.instruction(&Instruction::LocalGet(t[0]));
        func.instruction(&Instruction::LocalGet(t[3]));
        func.instruction(&Instruction::I32Add);
        func.instruction(&Instruction::I32Load8U(Self::memarg(4, 0)));
        func.instruction(&Instruction::LocalSet(t[4])); // byte

        // Check range and transform
        if to_upper {
            // Check 0x61 <= byte <= 0x7a (lowercase a-z)
            func.instruction(&Instruction::LocalGet(t[4]));
            func.instruction(&Instruction::I32Const(0x61));
            func.instruction(&Instruction::I32GeU);
            func.instruction(&Instruction::LocalGet(t[4]));
            func.instruction(&Instruction::I32Const(0x7a));
            func.instruction(&Instruction::I32LeU);
            func.instruction(&Instruction::I32And);
            func.instruction(&Instruction::If(wasm_encoder::BlockType::Empty));
            func.instruction(&Instruction::LocalGet(t[4]));
            func.instruction(&Instruction::I32Const(0x20));
            func.instruction(&Instruction::I32Sub);
            func.instruction(&Instruction::LocalSet(t[4]));
            func.instruction(&Instruction::End);
        } else {
            // Check 0x41 <= byte <= 0x5a (uppercase A-Z)
            func.instruction(&Instruction::LocalGet(t[4]));
            func.instruction(&Instruction::I32Const(0x41));
            func.instruction(&Instruction::I32GeU);
            func.instruction(&Instruction::LocalGet(t[4]));
            func.instruction(&Instruction::I32Const(0x5a));
            func.instruction(&Instruction::I32LeU);
            func.instruction(&Instruction::I32And);
            func.instruction(&Instruction::If(wasm_encoder::BlockType::Empty));
            func.instruction(&Instruction::LocalGet(t[4]));
            func.instruction(&Instruction::I32Const(0x20));
            func.instruction(&Instruction::I32Add);
            func.instruction(&Instruction::LocalSet(t[4]));
            func.instruction(&Instruction::End);
        }

        // Store byte to dst
        func.instruction(&Instruction::LocalGet(t[2]));
        func.instruction(&Instruction::LocalGet(t[3]));
        func.instruction(&Instruction::I32Add);
        func.instruction(&Instruction::LocalGet(t[4]));
        func.instruction(&Instruction::I32Store8(Self::memarg(4, 0)));

        // idx++
        func.instruction(&Instruction::LocalGet(t[3]));
        func.instruction(&Instruction::I32Const(1));
        func.instruction(&Instruction::I32Add);
        func.instruction(&Instruction::LocalSet(t[3]));

        func.instruction(&Instruction::Br(0)); // continue
        func.instruction(&Instruction::End); // end loop
        func.instruction(&Instruction::End); // end block

        // Push result
        func.instruction(&Instruction::LocalGet(t[2]));
    }

    /// Emit startswith(prefix): returns i32 (0 or 1).
    fn emit_string_startswith(
        &self,
        value: &Expr,
        prefix: &Expr,
        ctx: &mut FuncContext,
        func: &mut Function,
    ) {
        let depth = ctx.str_depth;
        ctx.str_depth = depth + 1;

        self.emit_expr(value, ctx, func);
        func.instruction(&Instruction::LocalSet(ctx.str_saves[depth]));

        self.emit_expr(prefix, ctx, func);

        ctx.str_depth = depth;
        func.instruction(&Instruction::LocalSet(ctx.str_temps[1])); // prefix_ptr
        func.instruction(&Instruction::LocalGet(ctx.str_saves[depth]));
        func.instruction(&Instruction::LocalSet(ctx.str_temps[0])); // str_ptr

        let t = &ctx.str_temps;

        // Load lengths
        func.instruction(&Instruction::LocalGet(t[0]));
        func.instruction(&Instruction::I32Load(Self::memarg(0, 2)));
        func.instruction(&Instruction::LocalSet(t[2])); // str_len

        func.instruction(&Instruction::LocalGet(t[1]));
        func.instruction(&Instruction::I32Load(Self::memarg(0, 2)));
        func.instruction(&Instruction::LocalSet(t[3])); // prefix_len

        // result = 1
        func.instruction(&Instruction::I32Const(1));
        func.instruction(&Instruction::LocalSet(t[4]));

        // if str_len < prefix_len: result = 0
        func.instruction(&Instruction::LocalGet(t[2]));
        func.instruction(&Instruction::LocalGet(t[3]));
        func.instruction(&Instruction::I32LtU);
        func.instruction(&Instruction::If(wasm_encoder::BlockType::Empty));
        func.instruction(&Instruction::I32Const(0));
        func.instruction(&Instruction::LocalSet(t[4]));
        func.instruction(&Instruction::Else);

        // Compare first prefix_len bytes
        func.instruction(&Instruction::I32Const(0));
        func.instruction(&Instruction::LocalSet(t[5])); // idx

        func.instruction(&Instruction::Block(wasm_encoder::BlockType::Empty));
        func.instruction(&Instruction::Loop(wasm_encoder::BlockType::Empty));

        func.instruction(&Instruction::LocalGet(t[5]));
        func.instruction(&Instruction::LocalGet(t[3]));
        func.instruction(&Instruction::I32GeU);
        func.instruction(&Instruction::BrIf(1)); // all matched

        // Compare bytes
        func.instruction(&Instruction::LocalGet(t[0]));
        func.instruction(&Instruction::LocalGet(t[5]));
        func.instruction(&Instruction::I32Add);
        func.instruction(&Instruction::I32Load8U(Self::memarg(4, 0)));

        func.instruction(&Instruction::LocalGet(t[1]));
        func.instruction(&Instruction::LocalGet(t[5]));
        func.instruction(&Instruction::I32Add);
        func.instruction(&Instruction::I32Load8U(Self::memarg(4, 0)));

        func.instruction(&Instruction::I32Ne);
        func.instruction(&Instruction::If(wasm_encoder::BlockType::Empty));
        func.instruction(&Instruction::I32Const(0));
        func.instruction(&Instruction::LocalSet(t[4]));
        func.instruction(&Instruction::Br(2)); // br $done
        func.instruction(&Instruction::End);

        func.instruction(&Instruction::LocalGet(t[5]));
        func.instruction(&Instruction::I32Const(1));
        func.instruction(&Instruction::I32Add);
        func.instruction(&Instruction::LocalSet(t[5]));

        func.instruction(&Instruction::Br(0));
        func.instruction(&Instruction::End); // loop
        func.instruction(&Instruction::End); // block

        func.instruction(&Instruction::End); // else

        func.instruction(&Instruction::LocalGet(t[4]));
    }

    /// Emit endswith(suffix): returns i32 (0 or 1).
    fn emit_string_endswith(
        &self,
        value: &Expr,
        suffix: &Expr,
        ctx: &mut FuncContext,
        func: &mut Function,
    ) {
        let depth = ctx.str_depth;
        ctx.str_depth = depth + 1;

        self.emit_expr(value, ctx, func);
        func.instruction(&Instruction::LocalSet(ctx.str_saves[depth]));

        self.emit_expr(suffix, ctx, func);

        ctx.str_depth = depth;
        func.instruction(&Instruction::LocalSet(ctx.str_temps[1])); // suffix_ptr
        func.instruction(&Instruction::LocalGet(ctx.str_saves[depth]));
        func.instruction(&Instruction::LocalSet(ctx.str_temps[0])); // str_ptr

        let t = &ctx.str_temps;

        // Load lengths
        func.instruction(&Instruction::LocalGet(t[0]));
        func.instruction(&Instruction::I32Load(Self::memarg(0, 2)));
        func.instruction(&Instruction::LocalSet(t[2])); // str_len

        func.instruction(&Instruction::LocalGet(t[1]));
        func.instruction(&Instruction::I32Load(Self::memarg(0, 2)));
        func.instruction(&Instruction::LocalSet(t[3])); // suffix_len

        // result = 1
        func.instruction(&Instruction::I32Const(1));
        func.instruction(&Instruction::LocalSet(t[4]));

        // if str_len < suffix_len: result = 0
        func.instruction(&Instruction::LocalGet(t[2]));
        func.instruction(&Instruction::LocalGet(t[3]));
        func.instruction(&Instruction::I32LtU);
        func.instruction(&Instruction::If(wasm_encoder::BlockType::Empty));
        func.instruction(&Instruction::I32Const(0));
        func.instruction(&Instruction::LocalSet(t[4]));
        func.instruction(&Instruction::Else);

        // offset = str_len - suffix_len â†’ store in t[6]
        func.instruction(&Instruction::LocalGet(t[2]));
        func.instruction(&Instruction::LocalGet(t[3]));
        func.instruction(&Instruction::I32Sub);
        func.instruction(&Instruction::LocalSet(t[6])); // offset

        // Compare last suffix_len bytes
        func.instruction(&Instruction::I32Const(0));
        func.instruction(&Instruction::LocalSet(t[5])); // idx

        func.instruction(&Instruction::Block(wasm_encoder::BlockType::Empty));
        func.instruction(&Instruction::Loop(wasm_encoder::BlockType::Empty));

        func.instruction(&Instruction::LocalGet(t[5]));
        func.instruction(&Instruction::LocalGet(t[3]));
        func.instruction(&Instruction::I32GeU);
        func.instruction(&Instruction::BrIf(1));

        // Compare bytes: str[offset+idx] vs suffix[idx]
        func.instruction(&Instruction::LocalGet(t[0]));
        func.instruction(&Instruction::LocalGet(t[6]));
        func.instruction(&Instruction::I32Add);
        func.instruction(&Instruction::LocalGet(t[5]));
        func.instruction(&Instruction::I32Add);
        func.instruction(&Instruction::I32Load8U(Self::memarg(4, 0)));

        func.instruction(&Instruction::LocalGet(t[1]));
        func.instruction(&Instruction::LocalGet(t[5]));
        func.instruction(&Instruction::I32Add);
        func.instruction(&Instruction::I32Load8U(Self::memarg(4, 0)));

        func.instruction(&Instruction::I32Ne);
        func.instruction(&Instruction::If(wasm_encoder::BlockType::Empty));
        func.instruction(&Instruction::I32Const(0));
        func.instruction(&Instruction::LocalSet(t[4]));
        func.instruction(&Instruction::Br(2));
        func.instruction(&Instruction::End);

        func.instruction(&Instruction::LocalGet(t[5]));
        func.instruction(&Instruction::I32Const(1));
        func.instruction(&Instruction::I32Add);
        func.instruction(&Instruction::LocalSet(t[5]));

        func.instruction(&Instruction::Br(0));
        func.instruction(&Instruction::End); // loop
        func.instruction(&Instruction::End); // block

        func.instruction(&Instruction::End); // else

        func.instruction(&Instruction::LocalGet(t[4]));
    }

    /// Emit find(sub): returns i64 (-1 if not found, else index).
    fn emit_string_find(
        &self,
        value: &Expr,
        sub: &Expr,
        ctx: &mut FuncContext,
        func: &mut Function,
    ) {
        let depth = ctx.str_depth;
        ctx.str_depth = depth + 1;

        self.emit_expr(value, ctx, func);
        func.instruction(&Instruction::LocalSet(ctx.str_saves[depth]));

        self.emit_expr(sub, ctx, func);

        ctx.str_depth = depth;
        func.instruction(&Instruction::LocalSet(ctx.str_temps[1])); // needle_ptr
        func.instruction(&Instruction::LocalGet(ctx.str_saves[depth]));
        func.instruction(&Instruction::LocalSet(ctx.str_temps[0])); // hay_ptr

        let t = &ctx.str_temps;

        // Load lengths
        func.instruction(&Instruction::LocalGet(t[0]));
        func.instruction(&Instruction::I32Load(Self::memarg(0, 2)));
        func.instruction(&Instruction::LocalSet(t[2])); // hay_len

        func.instruction(&Instruction::LocalGet(t[1]));
        func.instruction(&Instruction::I32Load(Self::memarg(0, 2)));
        func.instruction(&Instruction::LocalSet(t[3])); // needle_len

        // result = -1
        func.instruction(&Instruction::I32Const(-1));
        func.instruction(&Instruction::LocalSet(t[4])); // result

        // i = 0
        func.instruction(&Instruction::I32Const(0));
        func.instruction(&Instruction::LocalSet(t[5])); // i (outer)

        // Outer loop: i from 0 to hay_len - needle_len
        func.instruction(&Instruction::Block(wasm_encoder::BlockType::Empty)); // $outer_done
        func.instruction(&Instruction::Loop(wasm_encoder::BlockType::Empty)); // $outer

        // if i + needle_len > hay_len: break
        func.instruction(&Instruction::LocalGet(t[5]));
        func.instruction(&Instruction::LocalGet(t[3]));
        func.instruction(&Instruction::I32Add);
        func.instruction(&Instruction::LocalGet(t[2]));
        func.instruction(&Instruction::I32GtU);
        func.instruction(&Instruction::BrIf(1)); // br $outer_done

        // Inner: compare needle bytes
        func.instruction(&Instruction::I32Const(0));
        func.instruction(&Instruction::LocalSet(t[6])); // j

        func.instruction(&Instruction::I32Const(1));
        func.instruction(&Instruction::LocalSet(t[7])); // match = 1

        func.instruction(&Instruction::Block(wasm_encoder::BlockType::Empty)); // $inner_done
        func.instruction(&Instruction::Loop(wasm_encoder::BlockType::Empty)); // $inner

        func.instruction(&Instruction::LocalGet(t[6]));
        func.instruction(&Instruction::LocalGet(t[3]));
        func.instruction(&Instruction::I32GeU);
        func.instruction(&Instruction::BrIf(1)); // all matched, br $inner_done

        // Compare hay[i+j] vs needle[j]
        func.instruction(&Instruction::LocalGet(t[0]));
        func.instruction(&Instruction::LocalGet(t[5]));
        func.instruction(&Instruction::I32Add);
        func.instruction(&Instruction::LocalGet(t[6]));
        func.instruction(&Instruction::I32Add);
        func.instruction(&Instruction::I32Load8U(Self::memarg(4, 0)));

        func.instruction(&Instruction::LocalGet(t[1]));
        func.instruction(&Instruction::LocalGet(t[6]));
        func.instruction(&Instruction::I32Add);
        func.instruction(&Instruction::I32Load8U(Self::memarg(4, 0)));

        func.instruction(&Instruction::I32Ne);
        func.instruction(&Instruction::If(wasm_encoder::BlockType::Empty));
        func.instruction(&Instruction::I32Const(0));
        func.instruction(&Instruction::LocalSet(t[7])); // match = 0
        func.instruction(&Instruction::Br(2)); // br $inner_done
        func.instruction(&Instruction::End);

        // j++
        func.instruction(&Instruction::LocalGet(t[6]));
        func.instruction(&Instruction::I32Const(1));
        func.instruction(&Instruction::I32Add);
        func.instruction(&Instruction::LocalSet(t[6]));
        func.instruction(&Instruction::Br(0)); // continue inner
        func.instruction(&Instruction::End); // end inner loop
        func.instruction(&Instruction::End); // end inner block

        // if match: result = i, break outer
        func.instruction(&Instruction::LocalGet(t[7]));
        func.instruction(&Instruction::If(wasm_encoder::BlockType::Empty));
        func.instruction(&Instruction::LocalGet(t[5]));
        func.instruction(&Instruction::LocalSet(t[4])); // result = i
        func.instruction(&Instruction::Br(2)); // br $outer_done (0=if, 1=loop, 2=block)
        func.instruction(&Instruction::End);

        // i++
        func.instruction(&Instruction::LocalGet(t[5]));
        func.instruction(&Instruction::I32Const(1));
        func.instruction(&Instruction::I32Add);
        func.instruction(&Instruction::LocalSet(t[5]));
        func.instruction(&Instruction::Br(0)); // continue outer
        func.instruction(&Instruction::End); // end outer loop
        func.instruction(&Instruction::End); // end outer block

        // Push result as i64
        func.instruction(&Instruction::LocalGet(t[4]));
        func.instruction(&Instruction::I64ExtendI32S);
    }
}

/// Check if any statement in a body uses the ** (pow) operator.
pub fn body_uses_pow(body: &[Stmt]) -> bool {
    for stmt in body {
        if stmt_uses_pow(stmt) {
            return true;
        }
    }
    false
}

fn stmt_uses_pow(stmt: &Stmt) -> bool {
    match &stmt.kind {
        StmtKind::Expr(e) => expr_uses_pow(e),
        StmtKind::Assign { value, .. } => expr_uses_pow(value),
        StmtKind::AugAssign { value, op, .. } => {
            matches!(op, AugAssignOp::Pow) || expr_uses_pow(value)
        }
        StmtKind::AnnAssign { value: Some(v), .. } => expr_uses_pow(v),
        StmtKind::Return(Some(v)) => expr_uses_pow(v),
        StmtKind::If {
            test,
            body,
            elif_clauses,
            else_body,
        } => {
            expr_uses_pow(test)
                || body_uses_pow(body)
                || elif_clauses
                    .iter()
                    .any(|(t, b)| expr_uses_pow(t) || body_uses_pow(b))
                || else_body.as_ref().is_some_and(|b| body_uses_pow(b))
        }
        StmtKind::While {
            test,
            body,
            else_body,
        } => {
            expr_uses_pow(test)
                || body_uses_pow(body)
                || else_body.as_ref().is_some_and(|b| body_uses_pow(b))
        }
        StmtKind::For {
            body, else_body, ..
        } => body_uses_pow(body) || else_body.as_ref().is_some_and(|b| body_uses_pow(b)),
        _ => false,
    }
}

fn expr_uses_pow(expr: &Expr) -> bool {
    match &expr.kind {
        ExprKind::BinOp { left, op, right } => {
            matches!(op, BinOp::Pow) || expr_uses_pow(left) || expr_uses_pow(right)
        }
        ExprKind::UnaryOp { operand, .. } => expr_uses_pow(operand),
        ExprKind::Call { args, .. } => args.iter().any(expr_uses_pow),
        ExprKind::IfExpr {
            test,
            body,
            else_body,
        } => expr_uses_pow(test) || expr_uses_pow(body) || expr_uses_pow(else_body),
        ExprKind::Compare { left, comparisons } => {
            expr_uses_pow(left) || comparisons.iter().any(|(_, e)| expr_uses_pow(e))
        }
        _ => false,
    }
}

/// Collect all math.* function names referenced in a body, plus implicit "pow"
/// when the ** operator or **= aug-assign is used.
pub fn collect_math_imports(
    body: &[Stmt],
    imports: &mut BTreeSet<String>,
    aliases: &HashMap<String, String>,
) {
    for stmt in body {
        collect_math_imports_stmt(stmt, imports, aliases);
    }
}

fn collect_math_imports_stmt(
    stmt: &Stmt,
    imports: &mut BTreeSet<String>,
    aliases: &HashMap<String, String>,
) {
    match &stmt.kind {
        StmtKind::Expr(e) => collect_math_imports_expr(e, imports, aliases),
        StmtKind::Assign { value, .. } => collect_math_imports_expr(value, imports, aliases),
        StmtKind::AugAssign { value, op, .. } => {
            if matches!(op, AugAssignOp::Pow) {
                imports.insert("pow".to_string());
            }
            collect_math_imports_expr(value, imports, aliases);
        }
        StmtKind::AnnAssign { value: Some(v), .. } => {
            collect_math_imports_expr(v, imports, aliases)
        }
        StmtKind::Return(Some(v)) => collect_math_imports_expr(v, imports, aliases),
        StmtKind::If {
            test,
            body,
            elif_clauses,
            else_body,
        } => {
            collect_math_imports_expr(test, imports, aliases);
            collect_math_imports(body, imports, aliases);
            for (t, b) in elif_clauses {
                collect_math_imports_expr(t, imports, aliases);
                collect_math_imports(b, imports, aliases);
            }
            if let Some(b) = else_body {
                collect_math_imports(b, imports, aliases);
            }
        }
        StmtKind::While {
            test,
            body,
            else_body,
        } => {
            collect_math_imports_expr(test, imports, aliases);
            collect_math_imports(body, imports, aliases);
            if let Some(b) = else_body {
                collect_math_imports(b, imports, aliases);
            }
        }
        StmtKind::For {
            iter,
            body,
            else_body,
            ..
        } => {
            collect_math_imports_expr(iter, imports, aliases);
            collect_math_imports(body, imports, aliases);
            if let Some(b) = else_body {
                collect_math_imports(b, imports, aliases);
            }
        }
        _ => {}
    }
}

fn collect_math_imports_expr(
    expr: &Expr,
    imports: &mut BTreeSet<String>,
    aliases: &HashMap<String, String>,
) {
    match &expr.kind {
        ExprKind::BinOp { left, op, right } => {
            if matches!(op, BinOp::Pow) {
                imports.insert("pow".to_string());
            }
            collect_math_imports_expr(left, imports, aliases);
            collect_math_imports_expr(right, imports, aliases);
        }
        ExprKind::UnaryOp { operand, .. } => collect_math_imports_expr(operand, imports, aliases),
        ExprKind::Compare { left, comparisons } => {
            collect_math_imports_expr(left, imports, aliases);
            for (_, e) in comparisons {
                collect_math_imports_expr(e, imports, aliases);
            }
        }
        ExprKind::IfExpr {
            test,
            body,
            else_body,
        } => {
            collect_math_imports_expr(test, imports, aliases);
            collect_math_imports_expr(body, imports, aliases);
            collect_math_imports_expr(else_body, imports, aliases);
        }
        ExprKind::Call { func, args, .. } => {
            // Detect math.X(...) calls
            if let ExprKind::Attribute { value, attr, .. } = &func.kind {
                if let ExprKind::Name(mod_name) = &value.kind {
                    if mod_name == "math" && math_function_arity(attr.as_str()).is_some() {
                        imports.insert(attr.clone());
                    }
                }
            }
            // Detect bare `sqrt(...)` calls bound via `from math import sqrt`.
            if let ExprKind::Name(n) = &func.kind {
                if let Some(canonical) = aliases.get(n) {
                    imports.insert(canonical.clone());
                }
            }
            for a in args {
                collect_math_imports_expr(a, imports, aliases);
            }
        }
        ExprKind::Subscript { value, index, .. } => {
            collect_math_imports_expr(value, imports, aliases);
            collect_math_imports_expr(index, imports, aliases);
        }
        ExprKind::Attribute { value, .. } => {
            collect_math_imports_expr(value, imports, aliases);
        }
        ExprKind::FString { parts } => {
            for part in parts {
                if let FStringPart::Expr(e) = part {
                    collect_math_imports_expr(e, imports, aliases);
                }
            }
        }
        _ => {}
    }
}

/// Check if any statement in a body uses string operations.
pub fn body_uses_strings(body: &[Stmt]) -> bool {
    body.iter().any(stmt_uses_strings)
}

/// #364: map an augmented-assignment operator to its plain binary operator, so
/// `t[i] op= v` can be desugared to `t[i] = t[i] op v` (subscript aug-assign).
fn aug_to_binop(op: AugAssignOp) -> BinOp {
    match op {
        AugAssignOp::Add => BinOp::Add,
        AugAssignOp::Sub => BinOp::Sub,
        AugAssignOp::Mul => BinOp::Mul,
        AugAssignOp::Div => BinOp::Div,
        AugAssignOp::FloorDiv => BinOp::FloorDiv,
        AugAssignOp::Mod => BinOp::Mod,
        AugAssignOp::Pow => BinOp::Pow,
        AugAssignOp::BitAnd => BinOp::BitAnd,
        AugAssignOp::BitOr => BinOp::BitOr,
        AugAssignOp::BitXor => BinOp::BitXor,
        AugAssignOp::ShiftLeft => BinOp::ShiftLeft,
        AugAssignOp::ShiftRight => BinOp::ShiftRight,
        AugAssignOp::MatMul => BinOp::MatMul,
    }
}

fn stmt_uses_strings(stmt: &Stmt) -> bool {
    match &stmt.kind {
        StmtKind::Expr(e) => expr_uses_strings(e),
        StmtKind::Assign { value, .. } => expr_uses_strings(value),
        StmtKind::AugAssign { value, .. } => expr_uses_strings(value),
        StmtKind::AnnAssign { value: Some(v), .. } => expr_uses_strings(v),
        StmtKind::Return(Some(v)) => expr_uses_strings(v),
        StmtKind::If {
            test,
            body,
            elif_clauses,
            else_body,
        } => {
            expr_uses_strings(test)
                || body_uses_strings(body)
                || elif_clauses
                    .iter()
                    .any(|(t, b)| expr_uses_strings(t) || body_uses_strings(b))
                || else_body.as_ref().is_some_and(|b| body_uses_strings(b))
        }
        StmtKind::While {
            test,
            body,
            else_body,
        } => {
            expr_uses_strings(test)
                || body_uses_strings(body)
                || else_body.as_ref().is_some_and(|b| body_uses_strings(b))
        }
        StmtKind::For {
            body, else_body, ..
        } => body_uses_strings(body) || else_body.as_ref().is_some_and(|b| body_uses_strings(b)),
        // raise X("msg") — catch the message string so __err_msg has space.
        StmtKind::Raise(Some(e), _) => expr_uses_strings(e),
        StmtKind::Try { body, handlers, .. } => {
            body_uses_strings(body) || handlers.iter().any(|h| body_uses_strings(&h.body))
        }
        _ => false,
    }
}

fn expr_uses_strings(expr: &Expr) -> bool {
    match &expr.kind {
        ExprKind::StringLiteral(_) => true,
        ExprKind::FString { .. } => true,
        // Tier 2/4/5: collection literals also need __alloc, so they trigger
        // the same string-infrastructure flags. (Sharing the allocator with
        // strings is intentional — both rely on the bump-allocator + memory
        // global emitted when needs_strings is set.)
        ExprKind::Tuple(_) => true,
        ExprKind::List(_) => true,
        ExprKind::Dict { .. } => true,
        // Closures need the allocator for the (func_idx, env_ptr) struct + env tuple.
        ExprKind::Lambda { .. } => true,
        ExprKind::BinOp { left, right, .. } => expr_uses_strings(left) || expr_uses_strings(right),
        ExprKind::UnaryOp { operand, .. } => expr_uses_strings(operand),
        ExprKind::Call { func, args, .. } => {
            // Check if calling a string method or str() builtin
            (match &func.kind {
                ExprKind::Name(n) if n == "str" => true,
                ExprKind::Attribute { value, .. } => expr_uses_strings(value),
                _ => false,
            }) || args.iter().any(expr_uses_strings)
        }
        ExprKind::IfExpr {
            test,
            body,
            else_body,
        } => expr_uses_strings(test) || expr_uses_strings(body) || expr_uses_strings(else_body),
        ExprKind::Compare { left, comparisons } => {
            expr_uses_strings(left) || comparisons.iter().any(|(_, e)| expr_uses_strings(e))
        }
        ExprKind::Subscript { value, .. } => expr_uses_strings(value),
        _ => false,
    }
}

/// Detect if any statement in a body uses dict operations.
pub fn body_uses_dicts(body: &[Stmt]) -> bool {
    body.iter().any(stmt_uses_dicts)
}

fn stmt_uses_dicts(stmt: &Stmt) -> bool {
    match &stmt.kind {
        StmtKind::Expr(e) => expr_uses_dicts(e),
        StmtKind::Assign { value, targets } => {
            expr_uses_dicts(value) || targets.iter().any(expr_uses_dicts)
        }
        StmtKind::AugAssign { value, .. } => expr_uses_dicts(value),
        StmtKind::AnnAssign { value: Some(v), .. } => expr_uses_dicts(v),
        StmtKind::Return(Some(v)) => expr_uses_dicts(v),
        StmtKind::If {
            test,
            body,
            elif_clauses,
            else_body,
        } => {
            expr_uses_dicts(test)
                || body_uses_dicts(body)
                || elif_clauses
                    .iter()
                    .any(|(t, b)| expr_uses_dicts(t) || body_uses_dicts(b))
                || else_body.as_ref().is_some_and(|b| body_uses_dicts(b))
        }
        StmtKind::While {
            test,
            body,
            else_body,
        } => {
            expr_uses_dicts(test)
                || body_uses_dicts(body)
                || else_body.as_ref().is_some_and(|b| body_uses_dicts(b))
        }
        StmtKind::For {
            iter,
            body,
            else_body,
            ..
        } => {
            expr_uses_dicts(iter)
                || body_uses_dicts(body)
                || else_body.as_ref().is_some_and(|b| body_uses_dicts(b))
        }
        _ => false,
    }
}

fn expr_uses_dicts(expr: &Expr) -> bool {
    match &expr.kind {
        ExprKind::Dict { .. } => true,
        ExprKind::DictComp { .. } => true,
        ExprKind::BinOp { left, right, .. } => expr_uses_dicts(left) || expr_uses_dicts(right),
        ExprKind::UnaryOp { operand, .. } => expr_uses_dicts(operand),
        ExprKind::Call { func, args, .. } => {
            // Method calls on dict values (.get / .keys / etc.)
            (match &func.kind {
                ExprKind::Attribute { value, attr, .. } => {
                    matches!(attr.as_str(), "get" | "keys" | "values" | "items")
                        || expr_uses_dicts(value)
                }
                _ => false,
            }) || args.iter().any(expr_uses_dicts)
        }
        ExprKind::Subscript { value, index, .. } => {
            expr_uses_dicts(value) || expr_uses_dicts(index)
        }
        ExprKind::IfExpr {
            test,
            body,
            else_body,
        } => expr_uses_dicts(test) || expr_uses_dicts(body) || expr_uses_dicts(else_body),
        ExprKind::Compare { left, comparisons } => {
            expr_uses_dicts(left) || comparisons.iter().any(|(_, e)| expr_uses_dicts(e))
        }
        _ => false,
    }
}

/// Detect if any statement in a body uses raise/assert/try.
pub fn body_uses_errors(body: &[Stmt]) -> bool {
    body.iter().any(stmt_uses_errors)
}

fn stmt_uses_errors(stmt: &Stmt) -> bool {
    match &stmt.kind {
        StmtKind::Raise(..) | StmtKind::Assert { .. } | StmtKind::Try { .. } => true,
        StmtKind::If {
            body,
            elif_clauses,
            else_body,
            ..
        } => {
            body_uses_errors(body)
                || elif_clauses.iter().any(|(_, b)| body_uses_errors(b))
                || else_body.as_ref().is_some_and(|b| body_uses_errors(b))
        }
        StmtKind::While {
            body, else_body, ..
        }
        | StmtKind::For {
            body, else_body, ..
        } => body_uses_errors(body) || else_body.as_ref().is_some_and(|b| body_uses_errors(b)),
        _ => false,
    }
}

fn return_to_val_types(ty: &Type) -> Vec<ValType> {
    match ty {
        Type::NoneType | Type::Void => vec![],
        other => to_wasm_type(other)
            .map(|w| vec![w.to_val_type()])
            .unwrap_or_default(),
    }
}

#[cfg(test)]
mod scratch_noninterference_tests {
    use super::{scratch_interference_violations, WasmType};
    use std::collections::HashMap;

    fn locals(pairs: &[(&str, u32, WasmType)]) -> HashMap<String, (u32, WasmType)> {
        pairs
            .iter()
            .map(|(n, i, t)| (n.to_string(), (*i, t.clone())))
            .collect()
    }

    #[test]
    fn distinct_allocation_is_non_interfering() {
        // The by-construction case: every name a distinct index.
        let l = locals(&[
            ("x", 0, WasmType::I64),
            ("__ck0", 1, WasmType::I64),
            ("__subl0", 2, WasmType::I32),
            ("__subi0", 3, WasmType::I32),
        ]);
        assert!(scratch_interference_violations(&l, &[1, 2, 3], false).is_empty());
    }

    #[test]
    fn aliased_index_is_rejected() {
        // Two names on the SAME local index — the k14 clobber class.
        let l = locals(&[("__ck0", 5, WasmType::I64), ("x", 5, WasmType::I64)]);
        let v = scratch_interference_violations(&l, &[5], false);
        assert!(v.iter().any(|m| m.contains("aliased")), "{v:?}");
    }

    #[test]
    fn scratch_on_env_ptr_rejected_in_lambda() {
        // A scratch slot landing on local 0 (env_ptr) in a lambda body.
        let l = locals(&[("__sub0", 0, WasmType::I32)]);
        let v = scratch_interference_violations(&l, &[0], true);
        assert!(v.iter().any(|m| m.contains("env_ptr")), "{v:?}");
        // …but local 0 is fine in a NON-lambda (it is a normal param there).
        assert!(scratch_interference_violations(&l, &[0], false).is_empty());
    }
}
