<!-- exp=baseline-001 task=str_split_rejoin cond=ps sample=5 model=claude-opus-4-8 cost_usd=0.010363 -->
```python
pieces = "a,b,,c".split(",")
print(pieces)
print("-".join([p for p in pieces if p]))
```
