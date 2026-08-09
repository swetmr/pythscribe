<!-- exp=baseline-sonnet task=fraction_arith cond=ps sample=2 model=claude-sonnet-5 cost_usd=0.0097341 -->
```python
from fractions import Fraction

a = Fraction(3, 12) + Fraction(1, 6)
print(f"{a.numerator}/{a.denominator}")

b = Fraction(2, 4)
print(f"{b.numerator}/{b.denominator}")
```
