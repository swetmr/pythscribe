<!-- exp=baseline-sonnet task=fraction_arith cond=python sample=4 model=claude-sonnet-5 cost_usd=0.009555000000000001 -->
```python
from fractions import Fraction

s = Fraction(3, 12) + Fraction(1, 6)
r = Fraction(2, 4)

print(f"{s.numerator}/{s.denominator}")
print(f"{r.numerator}/{r.denominator}")
```
