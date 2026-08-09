<!-- exp=baseline-haiku task=fraction_arith cond=python sample=1 model=claude-haiku-4-5-20251001 cost_usd=0.006112299999999999 -->
```python
from fractions import Fraction

print(f"{(Fraction(3, 12) + Fraction(1, 6)).numerator}/{(Fraction(3, 12) + Fraction(1, 6)).denominator}")
print(f"{Fraction(2, 4).numerator}/{Fraction(2, 4).denominator}")
```
