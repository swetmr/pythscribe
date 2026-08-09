<!-- exp=baseline-haiku task=dict_invert cond=python sample=1 model=claude-haiku-4-5-20251001 cost_usd=0.0132613 -->
```python
d = {"a": 1, "b": 2, "c": 3}
inverted = {v: k for v, k in d.items()}
print(inverted)
```
