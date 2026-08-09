<!-- exp=baseline-haiku task=set_ops cond=python sample=5 model=claude-haiku-4-5-20251001 cost_usd=0.0047494 -->
```python
set1 = {1, 2, 3, 4, 5}
set2 = {4, 5, 6, 7, 8}

print(sorted(set1 | set2))
print(sorted(set1 & set2))
print(sorted(set1 - set2))
```
