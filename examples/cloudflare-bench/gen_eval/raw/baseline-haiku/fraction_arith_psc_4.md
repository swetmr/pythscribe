<!-- exp=baseline-haiku task=fraction_arith cond=psc sample=4 model=claude-haiku-4-5-20251001 cost_usd=0.0055352000000000005 -->
```python
from fractions import Fraction

sum_fractions = Fraction(3, 12) + Fraction(1, 6)
print(f"{sum_fractions.numerator}/{sum_fractions.denominator}")

reduced = Fraction(2, 4)
print(f"{reduced.numerator}/{reduced.denominator}")
```
