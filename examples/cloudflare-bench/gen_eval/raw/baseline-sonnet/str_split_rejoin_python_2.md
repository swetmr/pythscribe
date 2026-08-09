<!-- exp=baseline-sonnet task=str_split_rejoin cond=python sample=2 model=claude-sonnet-5 cost_usd=0.0088314 -->
```python
s = "a,b,,c"
parts = s.split(",")
print(parts)
print("-".join(p for p in parts if p))
```
