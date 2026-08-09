<!-- exp=baseline-haiku task=str_split_rejoin cond=python sample=1 model=claude-haiku-4-5-20251001 cost_usd=0.007482300000000001 -->
```python
s = "a,b,,c"
pieces = s.split(',')
print(pieces)
print('-'.join(filter(None, pieces)))
```
