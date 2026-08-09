<!-- exp=baseline-haiku task=fraction_arith cond=ps sample=3 model=claude-haiku-4-5-20251001 cost_usd=0.009056 -->
```python
from fractions import Fraction

sum_frac = Fraction(3, 12) + Fraction(1, 6)
print(f"{sum_frac.numerator}/{sum_frac.denominator}")

reduced_frac = Fraction(2, 4)
print(f"{reduced_frac.numerator}/{reduced_frac.denominator}")
```
