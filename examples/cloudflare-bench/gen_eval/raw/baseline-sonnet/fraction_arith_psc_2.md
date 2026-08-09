<!-- exp=baseline-sonnet task=fraction_arith cond=psc sample=2 model=claude-sonnet-5 cost_usd=0.0112881 -->
```python
from fractions import Fraction

s = Fraction(3, 12) + Fraction(1, 6)
print(f"{s.numerator}/{s.denominator}")
r = Fraction(2, 4)
print(f"{r.numerator}/{r.denominator}")
```
