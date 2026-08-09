<!-- exp=baseline-haiku task=str_split_rejoin cond=python sample=2 model=claude-haiku-4-5-20251001 cost_usd=0.0030574 -->
```python
s = "a,b,,c"
parts = s.split(',')
print(parts)
print('-'.join(filter(None, parts)))
```
