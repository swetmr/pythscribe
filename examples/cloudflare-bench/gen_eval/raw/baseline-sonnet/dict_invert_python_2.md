<!-- exp=baseline-sonnet task=dict_invert cond=python sample=2 model=claude-sonnet-5 cost_usd=0.0082437 -->
```python
d = {"a": 1, "b": 2, "c": 3}
inverted = {v: k for k, v in d.items()}
print(inverted)
```
