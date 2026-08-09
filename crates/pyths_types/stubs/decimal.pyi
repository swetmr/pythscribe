# Type stubs for `decimal` — General-Decimal-Arithmetic-style exact
# decimal arithmetic (runtime/src/stdlib/decimal.js).
#
# A faithful subset of CPython's `decimal.Decimal`: string/int/float
# construction (float via the exact IEEE-754 binary expansion), `+ - * /`
# at the fixed default context (28 significant digits, ROUND_HALF_EVEN),
# comparisons, `quantize(exp, rounding=...)`, and CPython-matching
# str/repr formatting.
#
# Out of scope (raise a clear error rather than a silently-wrong value):
# custom contexts (getcontext/setcontext), precision/rounding other than
# the fixed 28-digit ROUND_HALF_EVEN default, traps/flags/signals,
# ln/exp/sqrt, FloatOperation, thread-local contexts, NaN/Infinity.

from typing import Any, Union


ROUND_CEILING: str
ROUND_DOWN: str
ROUND_FLOOR: str
ROUND_HALF_EVEN: str
ROUND_HALF_UP: str
ROUND_UP: str


class Decimal:
    def __init__(self, value: Union[str, int, float] = "0") -> None:
        ...

    def __add__(self, other: Any) -> "Decimal":
        ...

    def __radd__(self, other: Any) -> "Decimal":
        ...

    def __sub__(self, other: Any) -> "Decimal":
        ...

    def __rsub__(self, other: Any) -> "Decimal":
        ...

    def __mul__(self, other: Any) -> "Decimal":
        ...

    def __rmul__(self, other: Any) -> "Decimal":
        ...

    def __truediv__(self, other: Any) -> "Decimal":
        ...

    def __rtruediv__(self, other: Any) -> "Decimal":
        ...

    def __neg__(self) -> "Decimal":
        ...

    def __abs__(self) -> "Decimal":
        ...

    def __pos__(self) -> "Decimal":
        ...

    def __eq__(self, other: Any) -> bool:
        ...

    def __lt__(self, other: Any) -> bool:
        ...

    def __le__(self, other: Any) -> bool:
        ...

    def __gt__(self, other: Any) -> bool:
        ...

    def __ge__(self, other: Any) -> bool:
        ...

    def quantize(self, exp: "Decimal", rounding: str = ROUND_HALF_EVEN) -> "Decimal":
        ...

    def __str__(self) -> str:
        ...

    def __repr__(self) -> str:
        ...
