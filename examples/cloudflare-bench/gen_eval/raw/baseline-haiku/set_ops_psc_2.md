<!-- exp=baseline-haiku task=set_ops cond=psc sample=2 model=claude-haiku-4-5-20251001 cost_usd=0.0069167 -->
```python
set1, set2 = {1, 2, 3, 4, 5}, {4, 5, 6, 7, 8}
print(sorted(set1 | set2))
print(sorted(set1 & set2))
print(sorted(set1 - set2))
```
