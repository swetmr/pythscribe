<!-- exp=baseline-sonnet task=str_split_rejoin cond=ps sample=2 model=claude-sonnet-5 cost_usd=0.009055500000000001 -->
```python
s = "a,b,,c"
parts = s.split(",")
print(parts)
print("-".join([p for p in parts if p]))
```
