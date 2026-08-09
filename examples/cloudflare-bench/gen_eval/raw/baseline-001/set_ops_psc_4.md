<!-- exp=baseline-001 task=set_ops cond=psc sample=4 model=claude-opus-4-8 cost_usd=0.011786999999999999 -->
```python
a = {1, 2, 3, 4, 5}
b = {4, 5, 6, 7, 8}
print(sorted(a | b))
print(sorted(a & b))
print(sorted(a - b))
```
