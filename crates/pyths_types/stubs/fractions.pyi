# Type stubs for `fractions` — exact rational arithmetic
# (runtime/src/stdlib/fractions.js).
#
# A faithful subset of CPython's `fractions.Fraction`: construction from
# int, Fraction, str ("3/10", "0.5", "7"), and float (exact binary
# expansion); `+ - * /` and `**` with an integer exponent; comparisons
# (including against int/float); `numerator`/`denominator`; str/repr;
# `float()` conversion.
#
# Mixing with `float` returns a plain float (not a Fraction), matching
# CPython's fallback behavior.

from typing import Any, Optional, Union


class Fraction:
    numerator: int
    denominator: int

    def __init__(self, numerator: Union[int, str, float, "Fraction"] = 0, denominator: Optional[int] = None) -> None:
        ...

    def __add__(self, other: Any) -> Any:
        ...

    def __radd__(self, other: Any) -> Any:
        ...

    def __sub__(self, other: Any) -> Any:
        ...

    def __rsub__(self, other: Any) -> Any:
        ...

    def __mul__(self, other: Any) -> Any:
        ...

    def __rmul__(self, other: Any) -> Any:
        ...

    def __truediv__(self, other: Any) -> Any:
        ...

    def __rtruediv__(self, other: Any) -> Any:
        ...

    def __pow__(self, exp: int) -> "Fraction":
        ...

    def __neg__(self) -> "Fraction":
        ...

    def __abs__(self) -> "Fraction":
        ...

    def __pos__(self) -> "Fraction":
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

    def __str__(self) -> str:
        ...

    def __repr__(self) -> str:
        ...
