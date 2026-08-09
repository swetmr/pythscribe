<!-- exp=baseline-sonnet task=str_split_rejoin cond=psc sample=2 model=claude-sonnet-5 cost_usd=0.0097545 -->
```python
parts = "a,b,,c".split(",")
print(parts)
print("-".join([p for p in parts if p]))
```
