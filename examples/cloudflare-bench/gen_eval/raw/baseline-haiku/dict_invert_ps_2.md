<!-- exp=baseline-haiku task=dict_invert cond=ps sample=2 model=claude-haiku-4-5-20251001 cost_usd=0.0059702 -->
```python
d = {"a": 1, "b": 2, "c": 3}
inverted = {v: k for k, v in d.items()}
print(inverted)
```
