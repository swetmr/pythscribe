<!-- exp=baseline-001 task=str_split_rejoin cond=psc sample=1 model=claude-opus-4-8 cost_usd=0.0557005 -->
```python
parts = "a,b,,c".split(",")
print(parts)
print("-".join(p for p in parts if p))
```
