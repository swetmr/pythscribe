<!-- exp=baseline-sonnet task=set_ops cond=python sample=3 model=claude-sonnet-5 cost_usd=0.0091368 -->
```python
a = {1, 2, 3, 4, 5}
b = {4, 5, 6, 7, 8}
print(sorted(a | b))
print(sorted(a & b))
print(sorted(a - b))
```
