<!-- exp=baseline-001 task=str_split_rejoin cond=python sample=3 model=claude-opus-4-8 cost_usd=0.0097395 -->
```python
s = "a,b,,c"
pieces = s.split(",")
print(pieces)
print("-".join(p for p in pieces if p))
```
