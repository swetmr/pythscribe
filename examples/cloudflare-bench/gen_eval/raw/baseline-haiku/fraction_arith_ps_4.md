<!-- exp=baseline-haiku task=fraction_arith cond=ps sample=4 model=claude-haiku-4-5-20251001 cost_usd=0.004246 -->
```python
from fractions import Fraction

# Sum of 3/12 and 1/6
sum_frac = Fraction(3, 12) + Fraction(1, 6)
print(f"{sum_frac.numerator}/{sum_frac.denominator}")

# Reduce 2/4
reduced = Fraction(2, 4)
print(f"{reduced.numerator}/{reduced.denominator}")
```
