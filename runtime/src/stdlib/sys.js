// PythScribe stdlib — a minimal Python `sys` (the parts that make sense on an
// AOT/edge target). Platform/runtime-introspection members (getsizeof,
// getrefcount, settrace, ...) are intentionally omitted — they have no
// meaningful edge-runtime equivalent.

// CPython on a 64-bit build: 2**63 - 1. Kept as a Number (fits comfortably
// under Number.MAX_SAFE_INTEGER-based use as a loop bound); use maxsize_big for
// the exact bigint value.
export const maxsize = 9223372036854775807;
export const float_info = { max: Number.MAX_VALUE, min: Number.MIN_VALUE, epsilon: Number.EPSILON };
export const version = "3.12 (PythScribe)";
