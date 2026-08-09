<!-- exp=baseline-001 task=set_ops cond=python sample=2 model=claude-opus-4-8 cost_usd=0.0113985 -->
```python
a = {1, 2, 3, 4, 5}
b = {4, 5, 6, 7, 8}
print(sorted(a | b))
print(sorted(a & b))
print(sorted(a - b))
```
