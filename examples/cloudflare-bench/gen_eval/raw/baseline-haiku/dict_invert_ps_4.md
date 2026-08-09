<!-- exp=baseline-haiku task=dict_invert cond=ps sample=4 model=claude-haiku-4-5-20251001 cost_usd=0.0037351999999999997 -->
```python
original = {"a": 1, "b": 2, "c": 3}
inverted = {v: k for k, v in original.items()}
print(inverted)
```
