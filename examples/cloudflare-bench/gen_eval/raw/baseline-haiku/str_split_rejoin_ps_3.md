<!-- exp=baseline-haiku task=str_split_rejoin cond=ps sample=3 model=claude-haiku-4-5-20251001 cost_usd=0.0028705 -->
```python
s = "a,b,,c"
parts = s.split(",")
print(parts)
non_empty = [p for p in parts if p]
print("-".join(non_empty))
```
