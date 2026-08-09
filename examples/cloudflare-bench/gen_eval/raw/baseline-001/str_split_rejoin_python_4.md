<!-- exp=baseline-001 task=str_split_rejoin cond=python sample=4 model=claude-opus-4-8 cost_usd=0.011164499999999999 -->
```python
parts = "a,b,,c".split(",")
print(parts)
print("-".join(p for p in parts if p))
```
