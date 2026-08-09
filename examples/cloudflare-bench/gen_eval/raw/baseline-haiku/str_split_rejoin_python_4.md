<!-- exp=baseline-haiku task=str_split_rejoin cond=python sample=4 model=claude-haiku-4-5-20251001 cost_usd=0.0047474 -->
```python
s = "a,b,,c"
parts = s.split(',')
print(parts)
print('-'.join(p for p in parts if p))
```
