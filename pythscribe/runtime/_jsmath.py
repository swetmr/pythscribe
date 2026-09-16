"""The `math.*` host functions a compiled kernel may import, with ECMAScript `Math` semantics.

The compiler lowers `math.<fn>(...)` inside a `@wasm` kernel to a WASM IMPORT `math.<fn>`
(`crates/pyths_codegen_wasm/src/bridge.rs::math_import_js`, every one `f64... -> f64`), which
the browser glue satisfies with `Math.<fn>`. The server path satisfies the same imports HERE.
Two rules, both load-bearing for "same function, browser or server":

1. JS semantics, not Python's: `Math.sqrt(-1)` is NaN where `math.sqrt(-1)` raises,
   `Math.log(0)` is -Infinity, `Math.exp(1000)` is Infinity, `Math.pow(0, -1)` is Infinity.
   A host function that RAISED would abort the WASM call with a trap the browser never sees.
2. The set below is EXACTLY the codegen's list (pinned by
   `tests/pythscribe/test_runtime.py::test_host_imports_match_codegen`). A kernel needing any
   other import is refused at link time (`SandboxViolation`) -- no ambient capability, ever.

Honesty note: bit-for-bit identity between the browser and the server holds by construction
for kernels WITHOUT host imports (WASM f64/i64 arithmetic is fully specified). A kernel that
calls `math.sin` & co. inherits the host's libm: V8's fdlibm port in the tab, this module
(CPython's platform libm) on the server. Those may differ in the last ulp for transcendental
functions; `ServerKernel.host_imports` names what a kernel depends on so a caller can tell.
"""
from __future__ import annotations

import math
from typing import Callable

_NAN = float("nan")
_INF = float("inf")


def _is_odd_integer(y: float) -> bool:
    return math.isfinite(y) and y == math.floor(y) and (abs(y) % 2.0) == 1.0


def js_pow(x: float, y: float) -> float:
    """ECMAScript Number::exponentiate (the `Math.pow` algorithm), edge cases first."""
    if math.isnan(y):
        return _NAN
    if y == 0.0:
        return 1.0
    if math.isnan(x):
        return _NAN
    if math.isinf(y):
        ax = abs(x)
        if ax == 1.0:
            return _NAN
        if ax > 1.0:
            return _INF if y > 0 else 0.0
        return 0.0 if y > 0 else _INF
    if math.isinf(x):
        if x > 0:
            return _INF if y > 0 else 0.0
        odd = _is_odd_integer(y)
        if y > 0:
            return -_INF if odd else _INF
        return -0.0 if odd else 0.0
    if x == 0.0:
        neg = math.copysign(1.0, x) < 0
        odd = _is_odd_integer(y)
        if y > 0:
            return -0.0 if (neg and odd) else 0.0
        return -_INF if (neg and odd) else _INF
    if x < 0 and not (math.isfinite(y) and y == math.floor(y)):
        return _NAN
    try:
        return math.pow(x, y)
    except OverflowError:
        return -_INF if (x < 0 and _is_odd_integer(y)) else _INF
    except ValueError:  # pragma: no cover - every domain case is handled above
        return _NAN


def js_sqrt(x: float) -> float:
    if math.isnan(x) or x < 0:
        return _NAN
    return math.sqrt(x)  # sqrt(-0.0) == -0.0, sqrt(inf) == inf, as in JS


def _nan_on_domain(f: Callable[[float], float]) -> Callable[[float], float]:
    def g(x: float) -> float:
        try:
            return f(x)
        except (ValueError, OverflowError):
            return _NAN

    g.__name__ = f.__name__
    return g


def js_log(x: float) -> float:
    if math.isnan(x) or x < 0:
        return _NAN
    if x == 0.0:
        return -_INF
    if math.isinf(x):
        return _INF
    return math.log(x)


def js_log2(x: float) -> float:
    if math.isnan(x) or x < 0:
        return _NAN
    if x == 0.0:
        return -_INF
    if math.isinf(x):
        return _INF
    return math.log2(x)


def js_log10(x: float) -> float:
    if math.isnan(x) or x < 0:
        return _NAN
    if x == 0.0:
        return -_INF
    if math.isinf(x):
        return _INF
    return math.log10(x)


def js_exp(x: float) -> float:
    if math.isnan(x):
        return _NAN
    if math.isinf(x):
        return _INF if x > 0 else 0.0
    try:
        return math.exp(x)
    except OverflowError:
        return _INF


def js_ceil(x: float) -> float:
    if not math.isfinite(x):
        return x
    r = float(math.ceil(x))
    # Math.ceil(-0.5) is -0 and Math.ceil(-0) is -0: a zero result carries x's sign (opus m1.5 r1/B4)
    return math.copysign(0.0, x) if r == 0.0 else r


def js_floor(x: float) -> float:
    if not math.isfinite(x):
        return x
    r = float(math.floor(x))
    return math.copysign(0.0, x) if r == 0.0 else r  # Math.floor(-0) is -0; Math.floor(0.5) is +0


def js_atan2(y: float, x: float) -> float:
    return math.atan2(y, x)  # C99 atan2 == the ECMAScript table, incl. signed zeros and infinities


# name -> (arity, implementation). EXACTLY the codegen's `math_import_js` list.
HOST_MATH: dict[str, tuple[int, Callable[..., float]]] = {
    "pow": (2, js_pow),
    "sqrt": (1, js_sqrt),
    "sin": (1, _nan_on_domain(math.sin)),  # sin(inf) is NaN in JS, ValueError in Python
    "cos": (1, _nan_on_domain(math.cos)),
    "tan": (1, _nan_on_domain(math.tan)),
    "asin": (1, _nan_on_domain(math.asin)),  # |x| > 1 -> NaN
    "acos": (1, _nan_on_domain(math.acos)),
    "atan": (1, math.atan),
    "atan2": (2, js_atan2),
    "log": (1, js_log),
    "log2": (1, js_log2),
    "log10": (1, js_log10),
    "exp": (1, js_exp),
    "ceil": (1, js_ceil),
    "floor": (1, js_floor),
    "fabs": (1, math.fabs),
}
