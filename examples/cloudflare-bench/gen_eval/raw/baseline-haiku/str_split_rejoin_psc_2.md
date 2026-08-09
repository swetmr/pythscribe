<!-- exp=baseline-haiku task=str_split_rejoin cond=psc sample=2 model=claude-haiku-4-5-20251001 cost_usd=0.006389700000000001 -->
```python
s = "a,b,,c"
parts = s.split(",")
print(parts)
print("-".join(p for p in parts if p))
```
