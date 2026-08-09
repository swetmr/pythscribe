<!-- exp=baseline-sonnet task=fraction_arith cond=psc sample=3 model=claude-sonnet-5 cost_usd=0.010448100000000002 -->
```python
from fractions import Fraction

a = Fraction(3, 12) + Fraction(1, 6)
b = Fraction(2, 4)
print(f"{a.numerator}/{a.denominator}")
print(f"{b.numerator}/{b.denominator}")
```
