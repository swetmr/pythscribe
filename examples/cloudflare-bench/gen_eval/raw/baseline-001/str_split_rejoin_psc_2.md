<!-- exp=baseline-001 task=str_split_rejoin cond=psc sample=2 model=claude-opus-4-8 cost_usd=0.011003 -->
```python
xs="a,b,,c".split(",")
print(xs)
print("-".join([p for p in xs if p]))
```
