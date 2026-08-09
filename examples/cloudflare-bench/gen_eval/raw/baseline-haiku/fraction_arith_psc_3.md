<!-- exp=baseline-haiku task=fraction_arith cond=psc sample=3 model=claude-haiku-4-5-20251001 cost_usd=0.007105200000000001 -->
```python
from fractions import Fraction

sum_frac = Fraction(3, 12) + Fraction(1, 6)
print(f"{sum_frac.numerator}/{sum_frac.denominator}")

reduced = Fraction(2, 4)
print(f"{reduced.numerator}/{reduced.denominator}")
```
