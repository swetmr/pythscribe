<!-- exp=baseline-sonnet task=set_ops cond=ps sample=4 model=claude-sonnet-5 cost_usd=0.0094359 -->
```python
a = {1, 2, 3, 4, 5}
b = {4, 5, 6, 7, 8}

print(sorted(a | b))
print(sorted(a & b))
print(sorted(a - b))
```
