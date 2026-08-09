<!-- exp=baseline-sonnet task=str_split_rejoin cond=psc sample=1 model=claude-sonnet-5 cost_usd=0.037443 -->
```python
parts = "a,b,,c".split(",")
print(parts)
print("-".join([p for p in parts if p]))
```
