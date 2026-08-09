<!-- exp=baseline-haiku task=fraction_arith cond=python sample=4 model=claude-haiku-4-5-20251001 cost_usd=0.0034178999999999998 -->
```python
from fractions import Fraction

result1 = Fraction(3, 12) + Fraction(1, 6)
result2 = Fraction(2, 4)

print(f"{result1.numerator}/{result1.denominator}")
print(f"{result2.numerator}/{result2.denominator}")
```
