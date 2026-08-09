<!-- exp=baseline-haiku task=fraction_arith cond=psc sample=1 model=claude-haiku-4-5-20251001 cost_usd=0.0142033 -->
```python
from fractions import Fraction

s = Fraction(3, 12) + Fraction(1, 6)
print(f"{s.numerator}/{s.denominator}")

r = Fraction(2, 4)
print(f"{r.numerator}/{r.denominator}")
```
