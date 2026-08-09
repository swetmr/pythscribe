<!-- exp=baseline-haiku task=str_split_rejoin cond=ps sample=2 model=claude-haiku-4-5-20251001 cost_usd=0.0048855 -->
```python
s = "a,b,,c"
pieces = s.split(",")
print(pieces)
non_empty = [p for p in pieces if p]
print("-".join(non_empty))
```
