<!-- exp=baseline-sonnet task=str_split_rejoin cond=python sample=4 model=claude-sonnet-5 cost_usd=0.0086064 -->
```python
s = "a,b,,c"
parts = s.split(",")
print(parts)
print("-".join(p for p in parts if p))
```
