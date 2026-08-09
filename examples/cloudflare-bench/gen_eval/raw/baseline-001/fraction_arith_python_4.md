<!-- exp=baseline-001 task=fraction_arith cond=python sample=4 model=claude-opus-4-8 cost_usd=0.0127455 -->
```python
from fractions import Fraction

s = Fraction(3, 12) + Fraction(1, 6)
r = Fraction(2, 4)
print(f"{s.numerator}/{s.denominator}")
print(f"{r.numerator}/{r.denominator}")
```
