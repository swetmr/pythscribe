<!-- exp=baseline-haiku task=dict_get_default cond=ps sample=4 model=claude-haiku-4-5-20251001 cost_usd=0.0034260999999999996 -->
```python
d = {"x": 10}
x_val = d.get("x", -1)
y_val = d.get("y", -1)
print(f"{x_val} {y_val}")
```
