<!-- exp=baseline-haiku task=dict_invert cond=python sample=5 model=claude-haiku-4-5-20251001 cost_usd=0.0057771 -->
```python
original_dict = {"a": 1, "b": 2, "c": 3}
inverted_dict = {v: k for k, v in original_dict.items()}
print(inverted_dict)
```
