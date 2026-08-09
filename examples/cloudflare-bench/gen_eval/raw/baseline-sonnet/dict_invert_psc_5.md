<!-- exp=baseline-sonnet task=dict_invert cond=psc sample=5 model=claude-sonnet-5 cost_usd=0.010441800000000001 -->
```python
d = {"a": 1, "b": 2, "c": 3}
inv = {v: k for k, v in d.items()}
print(inv)
```
