<!-- exp=baseline-001 task=str_split_rejoin cond=python sample=1 model=claude-opus-4-8 cost_usd=0.014205500000000001 -->
```python
parts = "a,b,,c".split(",")
print(parts)
print("-".join(p for p in parts if p))
```
