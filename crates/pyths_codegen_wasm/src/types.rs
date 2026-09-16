use pyths_types::types::{ArrayDtype, Type};
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
    /// WASM f32 — the storage/marshal type of a `float32` array element
    /// (M2 array/buffer ABI). Compute is done in f64 with narrow-on-store
    /// (settled decision, option (b)); this variant is the LOAD/STORE type for
    /// the f32 buffer. **M2a-0 stub:** added to the type machinery here; no
    /// codegen emits `f32.load`/`f32.store` yet (that is M2a-3).
    F32,
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
    /// A WASM numeric `Array[dtype, ndim]` → i32 pointer into linear memory to
    /// a typed-array header (`[dtype][ndim][shape…]` padded to ×8) + a
    /// C-contiguous element region (M2 array/buffer ABI). The element width is
    /// `dtype.size_bytes()`.
    ///
    /// **M2a-0 stub:** representable (so `to_wasm_type` never returns `None`
    /// for an admitted array once admission lands in M2a-1) but no codegen
    /// branch loads/stores/indexes it yet — the dedicated `PtrArray` subscript
    /// / `len` / marshalling branches are M2a-3.
    PtrArray { dtype: ArrayDtype, ndim: u32 },
}

impl WasmType {
    pub fn to_val_type(&self) -> ValType {
        match self {
            WasmType::I64 => ValType::I64,
            WasmType::F64 => ValType::F64,
            WasmType::F32 => ValType::F32,
            WasmType::I32 | WasmType::Ptr => ValType::I32,
            // All collection / closure types are i32 pointers into linear memory
            // (or i32 handles into JS-side maps, for dicts). An array is also an
            // i32 pointer into linear memory.
            WasmType::PtrList(_)
            | WasmType::PtrDict(_, _)
            | WasmType::PtrTuple(_)
            | WasmType::PtrClosure { .. }
            | WasmType::PtrArray { .. } => ValType::I32,
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
                | WasmType::PtrArray { .. }
        )
    }

    /// Size in bytes when stored inline (e.g. as a tuple field).
    pub fn size_bytes(&self) -> u32 {
        match self {
            WasmType::I64 | WasmType::F64 => 8,
            WasmType::F32 => 4,
            WasmType::I32 | WasmType::Ptr => 4,
            // A pointer (list/dict/tuple/closure/array handle) is 4 bytes inline.
            WasmType::PtrList(_)
            | WasmType::PtrDict(_, _)
            | WasmType::PtrTuple(_)
            | WasmType::PtrClosure { .. }
            | WasmType::PtrArray { .. } => 4,
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
        // A numeric array lowers to an i32 pointer to its typed-array buffer.
        // Always representable, so `wasm_admission_sound`
        // (`is_wasm_eligible ⇒ to_wasm_type.is_some()`) holds the moment
        // admission starts accepting arrays (M2a-1). No behavior is attached
        // here — the codegen `PtrArray` branch is M2a-3.
        Type::Array(dtype, ndim) => Some(WasmType::PtrArray {
            dtype: *dtype,
            ndim: *ndim,
        }),
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

#[cfg(test)]
mod tests {
    use super::*;
    use pyths_types::types::ArrayDtype;

    /// M2a-0: the width/repr machinery for the array element kinds. uint8=1,
    /// i32/f32=4, i64/f64=8; f32 is a real WASM value type; PtrArray is an
    /// i32 pointer.
    #[test]
    fn array_dtype_sizes_and_f32_repr() {
        assert_eq!(ArrayDtype::Uint8.size_bytes(), 1);
        assert_eq!(ArrayDtype::Int32.size_bytes(), 4);
        assert_eq!(ArrayDtype::Float32.size_bytes(), 4);
        assert_eq!(ArrayDtype::Int64.size_bytes(), 8);
        assert_eq!(ArrayDtype::Float64.size_bytes(), 8);

        // F32 is a first-class WASM value type (LOAD/STORE type of f32 arrays).
        assert_eq!(WasmType::F32.to_val_type(), ValType::F32);
        assert_eq!(WasmType::F32.size_bytes(), 4);
        assert!(!WasmType::F32.is_any_ptr());
    }

    #[test]
    fn array_lowers_to_ptr_array_for_every_dtype() {
        for dt in [
            ArrayDtype::Int32,
            ArrayDtype::Int64,
            ArrayDtype::Float32,
            ArrayDtype::Float64,
            ArrayDtype::Uint8,
        ] {
            for ndim in [1u32, 2] {
                let w = to_wasm_type(&Type::Array(dt, ndim)).expect("array is representable");
                assert_eq!(w, WasmType::PtrArray { dtype: dt, ndim });
                // An array is an i32 pointer into linear memory.
                assert_eq!(w.to_val_type(), ValType::I32);
                assert_eq!(w.size_bytes(), 4);
                assert!(w.is_any_ptr());
            }
        }
    }

    #[test]
    fn from_spelling_roundtrips_the_alphabet_and_refuses_others() {
        for dt in [
            ArrayDtype::Int32,
            ArrayDtype::Int64,
            ArrayDtype::Float32,
            ArrayDtype::Float64,
            ArrayDtype::Uint8,
        ] {
            assert_eq!(ArrayDtype::from_spelling(dt.spelling()), Some(dt));
        }
        assert_eq!(ArrayDtype::from_spelling("float16"), None);
        assert_eq!(ArrayDtype::from_spelling("int8"), None);
        assert_eq!(ArrayDtype::from_spelling("complex64"), None);
    }
}
