pub mod wasm_analysis;

pub use wasm_analysis::{
    analyze_module, class_registry, exception_code, exception_name_from_expr,
    is_numeric_kernel_param, is_scalar_wasm_return, is_wasm_eligible, max_subscript_depth,
    max_subscript_depth_in_stmts, ExceptionClass, WasmAnalysis, WasmFuncInfo,
    WASM_MAX_SUBSCRIPT_NESTING,
};
