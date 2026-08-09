<!-- exp=baseline-haiku task=str_split_rejoin cond=ps sample=4 model=claude-haiku-4-5-20251001 cost_usd=0.0077805 -->
```python
parts = "a,b,,c".split(",")
print(parts)
print("-".join([p for p in parts if p]))
```
