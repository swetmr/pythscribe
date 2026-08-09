<!-- exp=baseline-001 task=fraction_arith cond=python sample=3 model=claude-opus-4-8 cost_usd=0.0128705 -->
```python
from fractions import Fraction

s = Fraction(3, 12) + Fraction(1, 6)
print(f"{s.numerator}/{s.denominator}")

r = Fraction(2, 4)
print(f"{r.numerator}/{r.denominator}")
```
