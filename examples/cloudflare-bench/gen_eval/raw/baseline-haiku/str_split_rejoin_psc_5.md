<!-- exp=baseline-haiku task=str_split_rejoin cond=psc sample=5 model=claude-haiku-4-5-20251001 cost_usd=0.009904699999999999 -->
```python
s = "a,b,,c"
print(s.split(","))
print("-".join([p for p in s.split(",") if p]))
```
