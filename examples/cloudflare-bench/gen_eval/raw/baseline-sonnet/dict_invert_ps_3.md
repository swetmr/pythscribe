<!-- exp=baseline-sonnet task=dict_invert cond=ps sample=3 model=claude-sonnet-5 cost_usd=0.0091128 -->
```python
d = {"a": 1, "b": 2, "c": 3}
inverted = {v: k for k, v in d.items()}
print(inverted)
```
