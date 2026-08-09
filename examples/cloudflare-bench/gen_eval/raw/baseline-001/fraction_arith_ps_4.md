<!-- exp=baseline-001 task=fraction_arith cond=ps sample=4 model=claude-opus-4-8 cost_usd=0.016118999999999998 -->
```python
from fractions import Fraction

total = Fraction(3, 12) + Fraction(1, 6)
print(f"{total.numerator}/{total.denominator}")

half = Fraction(2, 4)
print(f"{half.numerator}/{half.denominator}")
```
