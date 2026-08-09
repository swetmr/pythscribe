<!-- exp=baseline-haiku task=fraction_arith cond=ps sample=5 model=claude-haiku-4-5-20251001 cost_usd=0.006121000000000001 -->
```python
from fractions import Fraction

sum_fraction = Fraction(3, 12) + Fraction(1, 6)
print(f"{sum_fraction.numerator}/{sum_fraction.denominator}")

reduced_fraction = Fraction(2, 4)
print(f"{reduced_fraction.numerator}/{reduced_fraction.denominator}")
```
