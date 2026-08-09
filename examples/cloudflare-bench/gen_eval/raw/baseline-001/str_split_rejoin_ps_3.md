<!-- exp=baseline-001 task=str_split_rejoin cond=ps sample=3 model=claude-opus-4-8 cost_usd=0.010263000000000001 -->
```python
parts = "a,b,,c".split(",")
print(parts)
print("-".join(p for p in parts if p))
```
