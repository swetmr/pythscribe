<!-- exp=baseline-haiku task=fraction_arith cond=python sample=2 model=claude-haiku-4-5-20251001 cost_usd=0.0059329000000000005 -->
```python
from fractions import Fraction

frac_sum = Fraction(3, 12) + Fraction(1, 6)
print(f"{frac_sum.numerator}/{frac_sum.denominator}")

frac_reduced = Fraction(2, 4)
print(f"{frac_reduced.numerator}/{frac_reduced.denominator}")
```
