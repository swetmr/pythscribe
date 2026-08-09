use pyths_types::types::Type;
use wasm_encoder::ValType;

/// WASM-level types for PythScribe values.
///
/// Note: this is `Clone` (not `Copy`) because the collection variants carry
/// element-type metadata in `Vec`/`Box`. Pass references where possible; clone
/// at boundaries.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum WasmType {
    /// Python `int` → WASM i64
    I64,
    /// Python `float` → WASM f64
    F64,
    /// Python `bool` → WASM i32
    I32,
    /// Python `str` → WASM i32 pointer into linear memory.
    /// String layout: `[i32 byte_length][UTF-8 bytes...]`
    Ptr,
    /// Python `List[T]` → WASM i32 pointer to `[i32 length][i32 capacity][T...]`.
    PtrList(Box<WasmType>),
    /// Python `Dict[K, V]` → i32 handle to a JS-side `Map`. Key and value
    /// types are needed only at call sites (to pick the right import suffix);
    /// we store them so the bridge can route correctly.
    PtrDict(Box<WasmType>, Box<WasmType>),
    /// Python `Tuple[T0, T1, ...]` → i32 pointer to packed elements.
    PtrTuple(Vec<WasmType>),
    /// Python `Callable[[P0, P1, ...], R]` → i32 pointer to
    /// `[i32 func_idx][i32 env_ptr]`.
    PtrClosure {
        params: Vec<WasmType>,
        ret: Option<Box<WasmType>>,
    },
}

impl WasmType {
    pub fn to_val_type(&self) -> ValType {
        match self {
            WasmType::I64 => ValType::I64,
            WasmType::F64 => ValType::F64,
            WasmType::I32 | WasmType::Ptr => ValType::I32,
            // All collection / closure types are i32 pointers into linear memory
            // (or i32 handles into JS-side maps, for dicts).
            WasmType::PtrList(_)
            | WasmType::PtrDict(_, _)
            | WasmType::PtrTuple(_)
            | WasmType::PtrClosure { .. } => ValType::I32,
        }
    }

    /// Whether this type is a pointer into linear memory.
    pub fn is_ptr(&self) -> bool {
        matches!(self, WasmType::Ptr)
    }

    /// Whether this type is any kind of i32-pointer (string, list, tuple,
    /// dict handle, closure handle). Useful for places that previously asked
    /// "is this Ptr" but now also want to accept collections.
    pub fn is_any_ptr(&self) -> bool {
        matches!(
            self,
            WasmType::Ptr
                | WasmType::PtrList(_)
                | WasmType::PtrDict(_, _)
                | WasmType::PtrTuple(_)
                | WasmType::PtrClosure { .. }
        )
    }

    /// Size in bytes when stored inline (e.g. as a tuple field).
    pub fn size_bytes(&self) -> u32 {
        match self {
            WasmType::I64 | WasmType::F64 => 8,
            WasmType::I32 | WasmType::Ptr => 4,
            WasmType::PtrList(_)
            | WasmType::PtrDict(_, _)
            | WasmType::PtrTuple(_)
            | WasmType::PtrClosure { .. } => 4,
        }
    }
}

/// Convert a PythScribe type to a WASM type, if supported.
///
/// Recurses into collection types: a `List[int]` becomes `PtrList(I64)`; a
/// `Tuple[int, str]` becomes `PtrTuple([I64, Ptr])`. Returns `None` if any
/// nested type is unsupported. `Type::Any` inside a collection is treated
/// as opaque `Ptr` (i32) — needed for bare `list`/`dict`/`tuple` annotations.
pub fn to_wasm_type(ty: &Type) -> Option<WasmType> {
    match ty {
        Type::Int => Some(WasmType::I64),
        Type::Float => Some(WasmType::F64),
        Type::Bool => Some(WasmType::I32),
        Type::Str => Some(WasmType::Ptr),
        Type::List(inner) => to_wasm_inner(inner).map(|t| WasmType::PtrList(Box::new(t))),
        Type::Set(inner) => to_wasm_inner(inner).map(|t| WasmType::PtrList(Box::new(t))),
        Type::Dict(k, v) => {
            let kt = to_wasm_inner(k)?;
            let vt = to_wasm_inner(v)?;
            Some(WasmType::PtrDict(Box::new(kt), Box::new(vt)))
        }
        Type::Tuple(types) => {
            let elts: Option<Vec<_>> = types.iter().map(to_wasm_inner).collect();
            elts.map(WasmType::PtrTuple)
        }
        Type::Callable(params, ret) => {
            let p: Option<Vec<_>> = params.iter().map(to_wasm_inner).collect();
            let p = p?;
            let r = match &**ret {
                Type::NoneType | Type::Void => None,
                other => Some(Box::new(to_wasm_inner(other)?)),
            };
            Some(WasmType::PtrClosure { params: p, ret: r })
        }
        _ => None,
    }
}

/// Element-type conversion — like `to_wasm_type`, but `Type::Any` lowers to
/// opaque `Ptr` (i32) so bare `list`/`dict`/`tuple` annotations are accepted.
fn to_wasm_inner(ty: &Type) -> Option<WasmType> {
    if matches!(ty, Type::Any) {
        return Some(WasmType::Ptr);
    }
    to_wasm_type(ty)
}

/// Convert a PythScribe return type to WASM result types.
pub fn return_to_wasm(ty: &Type) -> Option<Vec<ValType>> {
    match ty {
        Type::NoneType | Type::Void => Some(vec![]),
        other => to_wasm_type(other).map(|w| vec![w.to_val_type()]),
    }
}
