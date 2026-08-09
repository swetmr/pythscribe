<!-- exp=baseline-haiku task=set_ops cond=psc sample=3 model=claude-haiku-4-5-20251001 cost_usd=0.0043416999999999996 -->
```python
a = {1, 2, 3, 4, 5}
b = {4, 5, 6, 7, 8}
print(sorted(a | b))
print(sorted(a & b))
print(sorted(a - b))
```
