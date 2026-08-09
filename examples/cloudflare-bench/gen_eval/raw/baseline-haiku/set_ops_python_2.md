<!-- exp=baseline-haiku task=set_ops cond=python sample=2 model=claude-haiku-4-5-20251001 cost_usd=0.0038644 -->
```python
set_a = {1, 2, 3, 4, 5}
set_b = {4, 5, 6, 7, 8}

print(sorted(set_a | set_b))
print(sorted(set_a & set_b))
print(sorted(set_a - set_b))
```
