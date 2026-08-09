<!-- exp=baseline-sonnet task=dict_get_default cond=ps sample=1 model=claude-sonnet-5 cost_usd=0.027474000000000002 -->
```python
d = {"x": 10}
a = d.get("x", -1)
b = d.get("y", -1)
print(f"{a} {b}")
```
