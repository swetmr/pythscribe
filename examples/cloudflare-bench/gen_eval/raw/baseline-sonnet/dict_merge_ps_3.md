<!-- exp=baseline-sonnet task=dict_merge cond=ps sample=3 model=claude-sonnet-5 cost_usd=0.009225 -->
```python
d1 = {"a": 1, "b": 2}
d2 = {"b": 20, "c": 30}
merged = {**d1, **d2}
print(merged)
```
