<!-- exp=baseline-sonnet task=str_split_rejoin cond=ps sample=1 model=claude-sonnet-5 cost_usd=0.027437999999999997 -->
```python
s = "a,b,,c"
parts = s.split(",")
print(parts)
print("-".join([p for p in parts if p]))
```
