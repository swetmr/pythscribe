<!-- exp=baseline-001 task=str_split_rejoin cond=python sample=5 model=claude-opus-4-8 cost_usd=0.010189499999999999 -->
```python
s = "a,b,,c"
parts = s.split(",")
print(parts)
print("-".join(p for p in parts if p))
```
