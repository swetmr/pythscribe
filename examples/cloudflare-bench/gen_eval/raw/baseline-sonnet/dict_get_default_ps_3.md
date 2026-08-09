<!-- exp=baseline-sonnet task=dict_get_default cond=ps sample=3 model=claude-sonnet-5 cost_usd=0.0090858 -->
```python
d = {"x": 10}
a = d.get("x", -1)
b = d.get("y", -1)
print(f"{a} {b}")
```
